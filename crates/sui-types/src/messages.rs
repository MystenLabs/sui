// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{base_types::*, committee::Committee, error::*, event::Event};
use crate::certificate_proof::CertificateProof;
use crate::committee::{EpochId, ProtocolVersion};
use crate::crypto::{
    sha3_hash, AuthoritySignInfo, AuthoritySignature, AuthorityStrongQuorumSignInfo,
    Ed25519SuiSignature, EmptySignInfo, Signature, Signer, SuiSignatureInner, ToFromBytes,
};
use crate::digests::TransactionEventsDigest;
use crate::gas::GasCostSummary;
use crate::intent::{Intent, IntentMessage, IntentScope};
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages_checkpoint::{
    CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointTimestamp,
};
use crate::object::{MoveObject, Object, ObjectFormatOptions, Owner};
use crate::signature::{AuthenticatorTrait, GenericSignature};
use crate::storage::{DeleteKind, WriteKind};
use crate::{
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use byteorder::{BigEndian, ReadBytesExt};
use enum_dispatch::enum_dispatch;
use fastcrypto::encoding::Base64;
use itertools::Either;
use move_binary_format::access::ModuleAccess;
use move_binary_format::file_format::{CodeOffset, LocalIndex, TypeParameterIndex};
use move_binary_format::CompiledModule;
use move_core_types::language_storage::ModuleId;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::TypeTag,
    value::MoveStructLayout,
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write;
use std::fmt::{Debug, Display, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    hash::{Hash, Hasher},
    iter,
};
use strum::IntoStaticStr;
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use tap::Pipe;
use thiserror::Error;
use tracing::debug;

pub const DUMMY_GAS_PRICE: u64 = 1;

const BLOCKED_MOVE_FUNCTIONS: [(ObjectID, &str, &str); 0] = [];

#[cfg(test)]
#[path = "unit_tests/messages_tests.rs"]
mod messages_tests;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum CallArg {
    // contains no structs or objects
    Pure(Vec<u8>),
    // an object
    Object(ObjectArg),
    // a vector of objects
    ObjVec(Vec<ObjectArg>),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum ObjectArg {
    // A Move object, either immutable, or owned mutable.
    ImmOrOwnedObject(ObjectRef),
    // A Move object that's shared.
    // SharedObject::mutable controls whether caller asks for a mutable reference to shared object.
    SharedObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        mutable: bool,
    },
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransferObject {
    pub recipient: SuiAddress,
    pub object_ref: ObjectRef,
}

fn type_tag_validity_check(
    tag: &TypeTag,
    config: &ProtocolConfig,
    depth: u32,
    starting_count: usize,
) -> UserInputResult<usize> {
    fp_ensure!(
        depth < config.max_type_argument_depth(),
        UserInputError::SizeLimitExceeded {
            limit: "maximum type argument depth in a call transaction".to_string(),
            value: config.max_type_argument_depth().to_string()
        }
    );
    let count = 1 + match tag {
        TypeTag::Bool
        | TypeTag::U8
        | TypeTag::U64
        | TypeTag::U128
        | TypeTag::Address
        | TypeTag::Signer
        | TypeTag::U16
        | TypeTag::U32
        | TypeTag::U256 => 0,
        TypeTag::Vector(t) => {
            type_tag_validity_check(t.as_ref(), config, depth + 1, starting_count + 1)?
        }
        TypeTag::Struct(s) => s.type_params.iter().try_fold(0, |accum, t| {
            let count = accum + type_tag_validity_check(t, config, depth + 1, starting_count + 1)?;
            fp_ensure!(
                count + starting_count < config.max_type_arguments() as usize,
                UserInputError::SizeLimitExceeded {
                    limit: "maximum type arguments in a call transaction".to_string(),
                    value: config.max_type_arguments().to_string()
                }
            );
            Ok(count)
        })?,
    };
    Ok(count)
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveCall {
    pub package: ObjectID,
    pub module: Identifier,
    pub function: Identifier,
    pub type_arguments: Vec<TypeTag>,
    pub arguments: Vec<CallArg>,
}

impl MoveCall {
    pub fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        let is_blocked = BLOCKED_MOVE_FUNCTIONS.contains(&(
            self.package,
            self.module.as_str(),
            self.function.as_str(),
        ));
        fp_ensure!(!is_blocked, UserInputError::BlockedMoveFunction);
        let mut type_arguments_count = 0;
        for tag in self.type_arguments.iter() {
            type_arguments_count += type_tag_validity_check(tag, config, 1, type_arguments_count)?;
            fp_ensure!(
                type_arguments_count < config.max_type_arguments() as usize,
                UserInputError::SizeLimitExceeded {
                    limit: "maximum type arguments in a call transaction".to_string(),
                    value: config.max_type_arguments().to_string()
                }
            );
        }
        fp_ensure!(
            self.arguments.len() < config.max_arguments() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum arguments in a move call".to_string(),
                value: config.max_arguments().to_string()
            }
        );
        for a in self.arguments.iter() {
            a.validity_check(config)?;
        }
        Ok(())
    }
}

#[serde_as]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveModulePublish {
    #[serde_as(as = "Vec<Bytes>")]
    pub modules: Vec<Vec<u8>>,
}

impl MoveModulePublish {
    pub fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        fp_ensure!(
            self.modules.len() < config.max_modules_in_publish() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum modules in a publish transaction".to_string(),
                value: config.max_modules_in_publish().to_string()
            }
        );
        Ok(())
    }
}

// TODO: we can deprecate TransferSui when its callsites on RPC & SDK are
// fully replaced by PaySui and PayAllSui.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransferSui {
    pub recipient: SuiAddress,
    pub amount: Option<u64>,
}

/// Send all SUI coins to one recipient.
/// only for SUI coin and does not require a separate gas coin object either.
/// Specifically, what pay_all_sui does are:
/// 1. accumulate all SUI from input coins and deposit all SUI to the first input coin
/// 2. transfer the updated first coin to the recipient and also use this first coin as
/// gas coin object.
/// 3. the balance of the first input coin after tx is sum(input_coins) - actual_gas_cost.
/// 4. all other input coins other than the first are deleted.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct PayAllSui {
    /// The coins to be used for payment.
    pub coins: Vec<ObjectRef>,
    /// The address that will receive payment
    pub recipient: SuiAddress,
}

impl PayAllSui {
    pub fn validity_check(&self, config: &ProtocolConfig, gas: &[ObjectRef]) -> UserInputResult {
        fp_ensure!(!self.coins.is_empty(), UserInputError::EmptyInputCoins);
        fp_ensure!(gas.len() == 1, UserInputError::UnexpectedGasPaymentObject);
        fp_ensure!(
            // unwrap() is safe because coins are not empty.
            // gas is > 0 (validity_check) and == 1 (above)
            self.coins.first().unwrap() == &gas[0],
            UserInputError::UnexpectedGasPaymentObject
        );
        fp_ensure!(
            self.coins.len() < config.max_coins() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum coins in a payment transaction".to_string(),
                value: config.max_coins().to_string()
            }
        );
        Ok(())
    }
}

/// Send SUI coins to a list of addresses, following a list of amounts.
/// only for SUI coin and does not require a separate gas coin object.
/// Specifically, what pay_sui does are:
/// 1. debit each input_coin to create new coin following the order of
/// amounts and assign it to the corresponding recipient.
/// 2. accumulate all residual SUI from input coins left and deposit all SUI to the first
/// input coin, then use the first input coin as the gas coin object.
/// 3. the balance of the first input coin after tx is sum(input_coins) - sum(amounts) - actual_gas_cost
/// 4. all other input coins other than the first one are deleted.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct PaySui {
    /// The coins to be used for payment.
    pub coins: Vec<ObjectRef>,
    /// The addresses that will receive payment
    pub recipients: Vec<SuiAddress>,
    /// The amounts each recipient will receive.
    /// Must be the same length as recipients
    pub amounts: Vec<u64>,
}

impl PaySui {
    pub fn validity_check(&self, config: &ProtocolConfig, gas: &[ObjectRef]) -> UserInputResult {
        fp_ensure!(!self.coins.is_empty(), UserInputError::EmptyInputCoins);
        fp_ensure!(gas.len() == 1, UserInputError::UnexpectedGasPaymentObject);
        fp_ensure!(
            // unwrap() is safe because coins are not empty.
            // gas is > 0 (validity_check) and == 1 (above)
            self.coins.first().unwrap() == &gas[0],
            UserInputError::UnexpectedGasPaymentObject
        );
        fp_ensure!(
            self.coins.len() < config.max_coins() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum coins in a payment transaction".to_string(),
                value: config.max_coins().to_string()
            }
        );
        fp_ensure!(
            self.recipients.len() <= config.max_pay_recipients() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum recipients in a payment transaction".to_string(),
                value: config.max_pay_recipients().to_string()
            }
        );
        // TODO: was this maybe missing a check for the following, or was
        // it intentionally omitted?
        // fp_ensure!(self.recipients.len() == self.amounts.len(), ...)
        Ok(())
    }
}

/// Pay each recipient the corresponding amount using the input coins
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Pay {
    /// The coins to be used for payment
    pub coins: Vec<ObjectRef>,
    /// The addresses that will receive payment
    pub recipients: Vec<SuiAddress>,
    /// The amounts each recipient will receive.
    /// Must be the same length as recipients
    pub amounts: Vec<u64>,
}

impl Pay {
    pub fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        fp_ensure!(
            self.coins.len() < config.max_coins() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum coins in a payment transaction".to_string(),
                value: config.max_coins().to_string()
            }
        );
        fp_ensure!(
            self.recipients.len() <= config.max_pay_recipients() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum recipients in a payment transaction".to_string(),
                value: config.max_pay_recipients().to_string()
            }
        );
        // TODO: was this maybe missing a check for the following, or was
        // it intentionally omitted?
        // fp_ensure!(self.recipients.len() == self.amounts.len(), ...)
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ChangeEpoch {
    /// The next (to become) epoch ID.
    pub epoch: EpochId,
    /// The protocol version in effect in the new epoch.
    pub protocol_version: ProtocolVersion,
    /// The total amount of gas charged for storage during the epoch.
    pub storage_charge: u64,
    /// The total amount of gas charged for computation during the epoch.
    pub computation_charge: u64,
    /// The total amount of storage rebate refunded during the epoch.
    pub storage_rebate: u64,
    /// Unix timestamp when epoch started
    pub epoch_start_timestamp_ms: u64,
    /// System packages (specifically framework and move stdlib) that are written before the new
    /// epoch starts. This tracks framework upgrades on chain. When executing the ChangeEpoch txn,
    /// the validator must write out the modules below.  Modules are given in their serialized form,
    /// and include the ObjectID within their serialized form.
    pub system_packages: Vec<(SequenceNumber, Vec<Vec<u8>>)>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct GenesisTransaction {
    pub objects: Vec<GenesisObject>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum GenesisObject {
    RawObject {
        data: crate::object::Data,
        owner: crate::object::Owner,
    },
}

/// Only commit_timestamp_ms is passed to the move call currently.
/// However we include epoch and round to make sure each ConsensusCommitPrologue has a unique tx digest.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ConsensusCommitPrologue {
    /// Epoch of the commit prologue transaction
    pub epoch: u64,
    /// Consensus round of the commit
    pub round: u64,
    /// Unix timestamp from consensus
    pub commit_timestamp_ms: CheckpointTimestamp,
}

impl GenesisObject {
    pub fn id(&self) -> ObjectID {
        match self {
            GenesisObject::RawObject { data, .. } => data.id(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, IntoStaticStr)]
pub enum SingleTransactionKind {
    /// Initiate an object transfer between addresses
    TransferObject(TransferObject),
    /// Publish a new Move module
    Publish(MoveModulePublish),
    /// Call a function in a published Move module
    Call(MoveCall),
    /// Initiate a SUI coin transfer between addresses
    TransferSui(TransferSui),
    /// Pay multiple recipients using multiple input coins
    Pay(Pay),
    /// Pay multiple recipients using multiple SUI coins,
    /// no extra gas payment SUI coin is required.
    PaySui(PaySui),
    /// After paying the gas of the transaction itself, pay
    /// pay all remaining coins to the recipient.
    PayAllSui(PayAllSui),
    /// A system transaction that will update epoch information on-chain.
    /// It will only ever be executed once in an epoch.
    /// The argument is the next epoch number, which is critical
    /// because it ensures that this transaction has a unique digest.
    /// This will eventually be translated to a Move call during execution.
    /// It also doesn't require/use a gas object.
    /// A validator will not sign a transaction of this kind from outside. It only
    /// signs internally during epoch changes.
    ChangeEpoch(ChangeEpoch),
    Genesis(GenesisTransaction),
    ConsensusCommitPrologue(ConsensusCommitPrologue),
    /// A transaction that allows the interleaving of native commands and Move calls
    ProgrammableTransaction(ProgrammableTransaction),
    // .. more transaction types go here
}

impl VersionedProtocolMessage for SingleTransactionKind {
    fn check_version_supported(&self, _current_protocol_version: ProtocolVersion) -> SuiResult {
        // This code does nothing right now - it exists to cause a compiler error when new
        // enumerants are added to SingleTransactionKind.
        //
        // When we add new cases here, check that current_protocol_version does not pre-date the
        // addition of that enumerant.
        match &self {
            SingleTransactionKind::TransferObject(_)
            | SingleTransactionKind::Publish(_)
            | SingleTransactionKind::Call(_)
            | SingleTransactionKind::TransferSui(_)
            | SingleTransactionKind::Pay(_)
            | SingleTransactionKind::PaySui(_)
            | SingleTransactionKind::PayAllSui(_)
            | SingleTransactionKind::ChangeEpoch(_)
            | SingleTransactionKind::Genesis(_)
            | SingleTransactionKind::ConsensusCommitPrologue(_)
            | SingleTransactionKind::ProgrammableTransaction(_) => Ok(()),
        }
    }
}

impl CallArg {
    fn input_objects(&self) -> Vec<InputObjectKind> {
        match self {
            CallArg::Pure(_) => vec![],
            CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) => {
                vec![InputObjectKind::ImmOrOwnedMoveObject(*object_ref)]
            }
            CallArg::Object(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable,
            }) => {
                let id = *id;
                let initial_shared_version = *initial_shared_version;
                let mutable = *mutable;
                vec![InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version,
                    mutable,
                }]
            }
            CallArg::ObjVec(vec) => vec
                .iter()
                .map(|obj_arg| match obj_arg {
                    ObjectArg::ImmOrOwnedObject(object_ref) => {
                        InputObjectKind::ImmOrOwnedMoveObject(*object_ref)
                    }
                    ObjectArg::SharedObject {
                        id,
                        initial_shared_version,
                        mutable,
                    } => {
                        let id = *id;
                        let initial_shared_version = *initial_shared_version;
                        let mutable = *mutable;
                        InputObjectKind::SharedMoveObject {
                            id,
                            initial_shared_version,
                            mutable,
                        }
                    }
                })
                .collect(),
        }
    }

    pub fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        match self {
            CallArg::Pure(p) => {
                fp_ensure!(
                    p.len() < config.max_pure_argument_size() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum pure argument size".to_string(),
                        value: config.max_pure_argument_size().to_string()
                    }
                );
            }
            CallArg::Object(_) => (),
            CallArg::ObjVec(v) => {
                fp_ensure!(
                    v.len() < config.max_object_vec_argument_size() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum object vector argument size".to_string(),
                        value: config.max_object_vec_argument_size().to_string()
                    }
                );
            }
        }
        Ok(())
    }
}

impl MoveCall {
    pub fn input_objects(&self) -> Vec<InputObjectKind> {
        let MoveCall {
            arguments,
            package,
            type_arguments,
            ..
        } = self;
        // using a BTreeSet so the output of `input_objects` has a stable ordering
        let mut packages = BTreeSet::from([*package]);
        for type_argument in type_arguments {
            add_type_tag_packages(&mut packages, type_argument)
        }
        arguments
            .iter()
            .flat_map(|arg| arg.input_objects())
            .chain(packages.into_iter().map(InputObjectKind::MovePackage))
            .collect()
    }
}

// Add package IDs, `ObjectID`, for types defined in modules.
fn add_type_tag_packages(packages: &mut BTreeSet<ObjectID>, type_argument: &TypeTag) {
    let mut stack = vec![type_argument];
    while let Some(cur) = stack.pop() {
        match cur {
            TypeTag::Bool
            | TypeTag::U8
            | TypeTag::U64
            | TypeTag::U128
            | TypeTag::Address
            | TypeTag::Signer
            | TypeTag::U16
            | TypeTag::U32
            | TypeTag::U256 => (),
            TypeTag::Vector(inner) => stack.push(inner),
            TypeTag::Struct(struct_tag) => {
                packages.insert(struct_tag.address.into());
                stack.extend(struct_tag.type_params.iter())
            }
        }
    }
}

/// A series of commands where the results of one command can be used in future
/// commands
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ProgrammableTransaction {
    /// Input objects or primitive values
    pub inputs: Vec<CallArg>,
    /// The commands to be executed sequentially. A failure in any command will
    /// result in the failure of the entire transaction.
    pub commands: Vec<Command>,
}

/// A single command in a programmable transaction.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum Command {
    /// A call to either an entry or a public Move function
    MoveCall(Box<ProgrammableMoveCall>),
    /// `(Vec<forall T:key+store. T>, address)`
    /// It sends n-objects to the specified address. These objects must have store
    /// (public transfer) and either the previous owner must be an address or the object must
    /// be newly created.
    TransferObjects(Vec<Argument>, Argument),
    /// `(&mut Coin<T>, u64)` -> `Coin<T>`
    /// It splits off some amount into a new coin
    SplitCoin(Argument, Argument),
    /// `(&mut Coin<T>, Vec<Coin<T>>)`
    /// It merges n-coins into the first coin
    MergeCoins(Argument, Vec<Argument>),
    /// Publishes a Move package
    Publish(Vec<Vec<u8>>),
    /// `forall T: Vec<T> -> vector<T>`
    /// Given n-values of the same type, it constructs a vector. For non objects or an empty vector,
    /// the type tag must be specified.
    MakeMoveVec(Option<TypeTag>, Vec<Argument>),
}

/// An argument to a programmable transaction command
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum Argument {
    /// The gas coin. The gas coin can only be used by-ref, except for with
    /// `TransferObjects`, which can use it by-value.
    GasCoin,
    /// One of the input objects or primitive values (from
    /// `ProgrammableTransaction` inputs)
    Input(u16),
    /// The result of another command (from `ProgrammableTransaction` commands)
    Result(u16),
    /// Like a `Result` but it accesses a nested result. Currently, the only usage
    /// of this is to access a value from a Move call with multiple return values.
    NestedResult(u16, u16),
}

/// The command for calling a Move function, either an entry function or a public
/// function (which cannot return references).
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ProgrammableMoveCall {
    /// The package containing the module and function.
    pub package: ObjectID,
    /// The specific module in the package containing the function.
    pub module: Identifier,
    /// The function to be called.
    pub function: Identifier,
    /// The type arguments to the function.
    pub type_arguments: Vec<TypeTag>,
    /// The arguments to the function.
    pub arguments: Vec<Argument>,
}

impl ProgrammableMoveCall {
    fn input_objects(&self) -> Vec<InputObjectKind> {
        let ProgrammableMoveCall {
            package,
            type_arguments,
            ..
        } = self;
        let mut packages = BTreeSet::from([*package]);
        for type_argument in type_arguments {
            add_type_tag_packages(&mut packages, type_argument)
        }
        packages
            .into_iter()
            .map(InputObjectKind::MovePackage)
            .collect()
    }
}

impl Command {
    fn publish_command_input_objects(modules: &[Vec<u8>]) -> Vec<InputObjectKind> {
        // For module publishing, all the dependent packages are implicit input objects
        // because they must all be on-chain in order for the package to publish.
        // All authorities must have the same view of those dependencies in order
        // to achieve consistent publish results.
        let compiled_modules = modules
            .iter()
            .filter_map(|bytes| match CompiledModule::deserialize(bytes) {
                Ok(m) => Some(m),
                // We will ignore this error here and simply let latter execution
                // to discover this error again and fail the transaction.
                // It's preferable to let transaction fail and charge gas when
                // malformed package is provided.
                Err(_) => None,
            })
            .collect::<Vec<_>>();
        Transaction::input_objects_in_compiled_modules(&compiled_modules)
    }

    fn input_objects(&self) -> Vec<InputObjectKind> {
        match self {
            Command::Publish(modules) => Self::publish_command_input_objects(modules),
            Command::MoveCall(c) => c.input_objects(),
            Command::MakeMoveVec(Some(t), _) => {
                let mut packages = BTreeSet::new();
                add_type_tag_packages(&mut packages, t);
                packages
                    .into_iter()
                    .map(InputObjectKind::MovePackage)
                    .collect()
            }
            Command::MakeMoveVec(None, _)
            | Command::TransferObjects(_, _)
            | Command::SplitCoin(_, _)
            | Command::MergeCoins(_, _) => vec![],
        }
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        match self {
            Command::MoveCall(call) => {
                let is_blocked = BLOCKED_MOVE_FUNCTIONS.contains(&(
                    call.package,
                    call.module.as_str(),
                    call.function.as_str(),
                ));
                fp_ensure!(!is_blocked, UserInputError::BlockedMoveFunction);
                let mut type_arguments_count = 0;
                for tag in call.type_arguments.iter() {
                    type_arguments_count +=
                        type_tag_validity_check(tag, config, 1, type_arguments_count)?;
                    fp_ensure!(
                        type_arguments_count < config.max_type_arguments() as usize,
                        UserInputError::SizeLimitExceeded {
                            limit: "maximum type arguments in a call transaction".to_string(),
                            value: config.max_type_arguments().to_string()
                        }
                    );
                }
                fp_ensure!(
                    call.arguments.len() < config.max_arguments() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum arguments in a move call".to_string(),
                        value: config.max_arguments().to_string()
                    }
                );
            }
            Command::TransferObjects(args, _) | Command::MergeCoins(_, args) => {
                fp_ensure!(!args.is_empty(), UserInputError::EmptyCommandInput);
                fp_ensure!(
                    args.len() < config.max_arguments() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum arguments in a programmable transaction command"
                            .to_string(),
                        value: config.max_arguments().to_string()
                    }
                );
            }
            Command::MakeMoveVec(ty_opt, args) => {
                // ty_opt.is_none() ==> !args.is_empty()
                fp_ensure!(
                    ty_opt.is_some() || !args.is_empty(),
                    UserInputError::EmptyCommandInput
                );
                if let Some(ty) = ty_opt {
                    let type_arguments_count = type_tag_validity_check(ty, config, 1, 0)?;
                    fp_ensure!(
                        type_arguments_count < config.max_type_arguments() as usize,
                        UserInputError::SizeLimitExceeded {
                            limit: "maximum type arguments in a call transaction".to_string(),
                            value: config.max_type_arguments().to_string()
                        }
                    );
                }
                fp_ensure!(!args.is_empty(), UserInputError::EmptyCommandInput);
                fp_ensure!(
                    args.len() < config.max_arguments() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum arguments in a programmable transaction command"
                            .to_string(),
                        value: config.max_arguments().to_string()
                    }
                );
            }
            Command::Publish(modules) => {
                fp_ensure!(!modules.is_empty(), UserInputError::EmptyCommandInput);
                fp_ensure!(
                    modules.len() < config.max_modules_in_publish() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum modules in a programmable transaction publish command"
                            .to_string(),
                        value: config.max_modules_in_publish().to_string()
                    }
                );
            }
            Command::SplitCoin(_, _) => (),
        };
        Ok(())
    }
}

fn write_sep<T: Display>(
    f: &mut Formatter<'_>,
    items: impl IntoIterator<Item = T>,
    sep: &str,
) -> std::fmt::Result {
    let mut xs = items.into_iter().peekable();
    while let Some(x) = xs.next() {
        if xs.peek().is_some() {
            write!(f, "{sep}")?;
        }
        write!(f, "{x}")?;
    }
    Ok(())
}

impl ProgrammableTransaction {
    pub fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let ProgrammableTransaction { inputs, commands } = self;
        let input_arg_objects = inputs
            .iter()
            .flat_map(|arg| arg.input_objects())
            .collect::<Vec<_>>();
        // all objects, not just mutable, must be unique
        let mut used = HashSet::new();
        if !input_arg_objects.iter().all(|o| used.insert(o.object_id())) {
            return Err(UserInputError::DuplicateObjectRefInput);
        }
        // do not duplicate packages referred to in commands
        let command_input_objects: BTreeSet<InputObjectKind> = commands
            .iter()
            .flat_map(|command| command.input_objects())
            .collect();
        Ok(input_arg_objects
            .into_iter()
            .chain(command_input_objects)
            .collect())
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        fp_ensure!(
            self.commands.len() < config.max_programmable_tx_commands() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum commands in a programmable transaction".to_string(),
                value: config.max_programmable_tx_commands().to_string()
            }
        );
        for c in &self.commands {
            c.validity_check(config)?
        }
        Ok(())
    }

    fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.inputs
            .iter()
            .filter_map(|arg| match arg {
                CallArg::Pure(_) | CallArg::Object(ObjectArg::ImmOrOwnedObject(_)) => None,
                CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutable,
                }) => Some(vec![SharedInputObject {
                    id: *id,
                    initial_shared_version: *initial_shared_version,
                    mutable: *mutable,
                }]),
                CallArg::ObjVec(_) => {
                    panic!(
                        "not supported in programmable transactions, \
                        should be unreachable if the input checker was run"
                    )
                }
            })
            .flatten()
    }
}

impl Display for Argument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Argument::GasCoin => write!(f, "GasCoin"),
            Argument::Input(i) => write!(f, "Input({i})"),
            Argument::Result(i) => write!(f, "Result({i})"),
            Argument::NestedResult(i, j) => write!(f, "NestedResult({i},{j})"),
        }
    }
}

impl Display for ProgrammableMoveCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        } = self;
        write!(f, "{package}::{module}::{function}")?;
        if !type_arguments.is_empty() {
            write!(f, "<")?;
            write_sep(f, type_arguments, ",")?;
            write!(f, ">")?;
        }
        write!(f, "(")?;
        write_sep(f, arguments, ",")?;
        write!(f, ")")
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::MoveCall(p) => {
                write!(f, "MoveCall({p})")
            }
            Command::MakeMoveVec(ty_opt, elems) => {
                write!(f, "MakeMoveVec(")?;
                if let Some(ty) = ty_opt {
                    write!(f, "Some{ty}")?;
                } else {
                    write!(f, "None")?;
                }
                write!(f, ",[")?;
                write_sep(f, elems, ",")?;
                write!(f, "])")
            }
            Command::TransferObjects(objs, addr) => {
                write!(f, "TransferObjects([")?;
                write_sep(f, objs, ",")?;
                write!(f, "],{addr})")
            }
            Command::SplitCoin(coin, amount) => write!(f, "SplitCoin({coin},{amount})"),
            Command::MergeCoins(target, coins) => {
                write!(f, "MergeCoins({target},")?;
                write_sep(f, coins, ",")?;
                write!(f, ")")
            }
            Command::Publish(_bytes) => write!(f, "Publish(_)"),
        }
    }
}

impl Display for ProgrammableTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let ProgrammableTransaction { inputs, commands } = self;
        writeln!(f, "Inputs: {inputs:?}")?;
        writeln!(f, "Commands: [")?;
        for c in commands {
            writeln!(f, "  {c},")?;
        }
        writeln!(f, "]")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SharedInputObject {
    pub id: ObjectID,
    pub initial_shared_version: SequenceNumber,
    pub mutable: bool,
}

impl SharedInputObject {
    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn into_id_and_version(self) -> (ObjectID, SequenceNumber) {
        (self.id, self.initial_shared_version)
    }
}

impl SingleTransactionKind {
    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match &self {
            Self::Call(_) | Self::ChangeEpoch(_) | Self::ConsensusCommitPrologue(_) => {
                Either::Left(self.all_move_call_shared_input_objects())
            }
            Self::ProgrammableTransaction(pt) => {
                Either::Right(Either::Left(pt.shared_input_objects()))
            }
            _ => Either::Right(Either::Right(iter::empty())),
        }
    }

    /// Returns an iterator of all shared input objects used by this transaction.
    /// It covers both Call and ChangeEpoch transaction kind, because both makes Move calls.
    /// This function is split out of shared_input_objects because Either type only supports
    /// two variants, while we need to be able to return three variants (Flatten, Once, Empty).
    fn all_move_call_shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match &self {
            Self::Call(MoveCall { arguments, .. }) => Either::Left(
                arguments
                    .iter()
                    .filter_map(|arg| match arg {
                        CallArg::Pure(_) | CallArg::Object(ObjectArg::ImmOrOwnedObject(_)) => None,
                        CallArg::Object(ObjectArg::SharedObject {
                            id,
                            initial_shared_version,
                            mutable,
                        }) => Some(vec![SharedInputObject {
                            id: *id,
                            initial_shared_version: *initial_shared_version,
                            mutable: *mutable,
                        }]),
                        CallArg::ObjVec(vec) => Some(
                            vec.iter()
                                .filter_map(|obj_arg| {
                                    if let ObjectArg::SharedObject {
                                        id,
                                        initial_shared_version,
                                        mutable,
                                    } = obj_arg
                                    {
                                        Some(SharedInputObject {
                                            id: *id,
                                            initial_shared_version: *initial_shared_version,
                                            mutable: *mutable,
                                        })
                                    } else {
                                        None
                                    }
                                })
                                .collect(),
                        ),
                    })
                    .flatten(),
            ),
            Self::ChangeEpoch(_) => Either::Right(iter::once(SharedInputObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            })),
            Self::ConsensusCommitPrologue(_) => Either::Right(iter::once(SharedInputObject {
                id: SUI_CLOCK_OBJECT_ID,
                initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                mutable: true,
            })),
            _ => unreachable!(),
        }
    }

    /// Actively being replaced by programmable transactions
    pub fn legacy_move_call(&self) -> Option<&MoveCall> {
        match &self {
            Self::Call(call @ MoveCall { .. }) => Some(call),
            _ => None,
        }
    }

    /// Return the metadata of each of the input objects for the transaction.
    /// For a Move object, we attach the object reference;
    /// for a Move package, we provide the object id only since they never change on chain.
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let input_objects = match &self {
            Self::TransferObject(TransferObject { object_ref, .. }) => {
                vec![InputObjectKind::ImmOrOwnedMoveObject(*object_ref)]
            }
            Self::Call(move_call) => move_call.input_objects(),
            Self::Publish(MoveModulePublish { modules }) => {
                Command::publish_command_input_objects(modules)
            }
            Self::TransferSui(_) => {
                vec![]
            }
            Self::Pay(Pay { coins, .. }) => coins
                .iter()
                .map(|o| InputObjectKind::ImmOrOwnedMoveObject(*o))
                .collect(),
            Self::PaySui(PaySui { coins, .. }) => coins
                .iter()
                .map(|o| InputObjectKind::ImmOrOwnedMoveObject(*o))
                .collect(),
            Self::PayAllSui(PayAllSui { coins, .. }) => coins
                .iter()
                .map(|o| InputObjectKind::ImmOrOwnedMoveObject(*o))
                .collect(),
            Self::ChangeEpoch(_) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                }]
            }
            Self::Genesis(_) => {
                vec![]
            }
            Self::ConsensusCommitPrologue(_) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutable: true,
                }]
            }
            Self::ProgrammableTransaction(p) => return p.input_objects(),
        };
        // Ensure that there are no duplicate inputs. This cannot be removed because:
        // In [`AuthorityState::check_locks`], we check that there are no duplicate mutable
        // input objects, which would have made this check here unnecessary. However we
        // do plan to allow shared objects show up more than once in multiple single
        // transactions down the line. Once we have that, we need check here to make sure
        // the same shared object doesn't show up more than once in the same single
        // transaction.
        let mut used = HashSet::new();
        if !input_objects.iter().all(|o| used.insert(o.object_id())) {
            return Err(UserInputError::DuplicateObjectRefInput);
        }
        Ok(input_objects)
    }

    pub fn validity_check(&self, config: &ProtocolConfig, gas: &[ObjectRef]) -> UserInputResult {
        match self {
            SingleTransactionKind::Publish(publish) => publish.validity_check(config)?,
            SingleTransactionKind::Call(call) => call.validity_check(config)?,
            SingleTransactionKind::Pay(p) => p.validity_check(config)?,
            SingleTransactionKind::PaySui(p) => p.validity_check(config, gas)?,
            SingleTransactionKind::PayAllSui(pa) => pa.validity_check(config, gas)?,
            SingleTransactionKind::ProgrammableTransaction(p) => p.validity_check(config)?,
            SingleTransactionKind::TransferObject(_)
            | SingleTransactionKind::TransferSui(_)
            | SingleTransactionKind::ChangeEpoch(_)
            | SingleTransactionKind::Genesis(_)
            | SingleTransactionKind::ConsensusCommitPrologue(_) => (),
        };
        Ok(())
    }
}

impl Display for SingleTransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match &self {
            Self::TransferObject(t) => {
                writeln!(writer, "Transaction Kind : Transfer Object")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                let (object_id, seq, digest) = t.object_ref;
                writeln!(writer, "Object ID : {}", &object_id)?;
                writeln!(writer, "Sequence Number : {:?}", seq)?;
                writeln!(writer, "Object Digest : {}", digest)?;
            }
            Self::TransferSui(t) => {
                writeln!(writer, "Transaction Kind : Transfer SUI")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                if let Some(amount) = t.amount {
                    writeln!(writer, "Amount: {}", amount)?;
                } else {
                    writeln!(writer, "Amount: Full Balance")?;
                }
            }
            Self::Pay(p) => {
                writeln!(writer, "Transaction Kind : Pay")?;
                writeln!(writer, "Coins:")?;
                for (object_id, seq, digest) in &p.coins {
                    writeln!(writer, "Object ID : {}", &object_id)?;
                    writeln!(writer, "Sequence Number : {:?}", seq)?;
                    writeln!(writer, "Object Digest : {}", digest)?;
                }
                writeln!(writer, "Recipients:")?;
                for recipient in &p.recipients {
                    writeln!(writer, "{}", recipient)?;
                }
                writeln!(writer, "Amounts:")?;
                for amount in &p.amounts {
                    writeln!(writer, "{}", amount)?
                }
            }
            Self::PaySui(p) => {
                writeln!(writer, "Transaction Kind : Pay SUI")?;
                writeln!(writer, "Coins:")?;
                for (object_id, seq, digest) in &p.coins {
                    writeln!(writer, "Object ID : {}", &object_id)?;
                    writeln!(writer, "Sequence Number : {:?}", seq)?;
                    writeln!(writer, "Object Digest : {}", digest)?;
                }
                writeln!(writer, "Recipients:")?;
                for recipient in &p.recipients {
                    writeln!(writer, "{}", recipient)?;
                }
                writeln!(writer, "Amounts:")?;
                for amount in &p.amounts {
                    writeln!(writer, "{}", amount)?
                }
            }
            Self::PayAllSui(p) => {
                writeln!(writer, "Transaction Kind : Pay all SUI")?;
                writeln!(writer, "Coins:")?;
                for (object_id, seq, digest) in &p.coins {
                    writeln!(writer, "Object ID : {}", &object_id)?;
                    writeln!(writer, "Sequence Number : {:?}", seq)?;
                    writeln!(writer, "Object Digest : {}", digest)?;
                }
                writeln!(writer, "Recipient:")?;
                writeln!(writer, "{}", &p.recipient)?;
            }
            Self::Publish(_p) => {
                writeln!(writer, "Transaction Kind : Publish")?;
            }
            Self::Call(c) => {
                writeln!(writer, "Transaction Kind : Call")?;
                writeln!(writer, "Package ID : {}", c.package.to_hex_literal())?;
                writeln!(writer, "Module : {}", c.module)?;
                writeln!(writer, "Function : {}", c.function)?;
                writeln!(writer, "Arguments : {:?}", c.arguments)?;
                writeln!(writer, "Type Arguments : {:?}", c.type_arguments)?;
            }
            Self::ChangeEpoch(e) => {
                writeln!(writer, "Transaction Kind : Epoch Change")?;
                writeln!(writer, "New epoch ID : {}", e.epoch)?;
                writeln!(writer, "Storage gas reward : {}", e.storage_charge)?;
                writeln!(writer, "Computation gas reward : {}", e.computation_charge)?;
                writeln!(writer, "Storage rebate : {}", e.storage_rebate)?;
                writeln!(writer, "Timestamp : {}", e.epoch_start_timestamp_ms)?;
            }
            Self::Genesis(_) => {
                writeln!(writer, "Transaction Kind : Genesis")?;
            }
            Self::ConsensusCommitPrologue(p) => {
                writeln!(writer, "Transaction Kind : Consensus Commit Prologue")?;
                writeln!(writer, "Timestamp : {}", p.commit_timestamp_ms)?;
            }
            Self::ProgrammableTransaction(p) => {
                writeln!(writer, "Transaction Kind : Programmable")?;
                write!(writer, "{p}")?;
            }
        }
        write!(f, "{}", writer)
    }
}

// TODO: Make SingleTransactionKind a Box
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, IntoStaticStr)]
pub enum TransactionKind {
    /// A single transaction.
    Single(SingleTransactionKind),
    /// A batch of single transactions.
    Batch(Vec<SingleTransactionKind>),
    // .. more transaction types go here
}

impl VersionedProtocolMessage for TransactionKind {
    fn check_version_supported(&self, current_protocol_version: ProtocolVersion) -> SuiResult {
        // If we add new cases here, check that current_protocol_version does not pre-date the
        // addition of that enumerant.
        match &self {
            TransactionKind::Single(s) => s.check_version_supported(current_protocol_version),
            TransactionKind::Batch(v) => {
                for s in v {
                    s.check_version_supported(current_protocol_version)?
                }
                Ok(())
            }
        }
    }
}

impl TransactionKind {
    pub fn single_transactions(&self) -> impl Iterator<Item = &SingleTransactionKind> {
        match self {
            TransactionKind::Single(s) => Either::Left(std::iter::once(s)),
            TransactionKind::Batch(b) => Either::Right(b.iter()),
        }
    }

    pub fn into_single_transactions(self) -> impl Iterator<Item = SingleTransactionKind> {
        match self {
            TransactionKind::Single(s) => Either::Left(std::iter::once(s)),
            TransactionKind::Batch(b) => Either::Right(b.into_iter()),
        }
    }

    pub fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let mut seen = BTreeSet::new();
        let inputs: Vec<_> = self
            .single_transactions()
            .map(|s| s.input_objects())
            .collect::<UserInputResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|kind| seen.insert(*kind))
            .collect();
        Ok(inputs)
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match &self {
            TransactionKind::Single(s) => Either::Left(s.shared_input_objects()),
            TransactionKind::Batch(b) => {
                Either::Right(b.iter().flat_map(|kind| kind.shared_input_objects()))
            }
        }
    }

    pub fn batch_size(&self) -> usize {
        match self {
            TransactionKind::Single(_) => 1,
            TransactionKind::Batch(batch) => batch.len(),
        }
    }

    pub fn is_pay_sui_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::Single(SingleTransactionKind::PaySui(_))
                | TransactionKind::Single(SingleTransactionKind::PayAllSui(_))
        )
    }

    pub fn is_system_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::Single(
                SingleTransactionKind::ChangeEpoch(_)
                    | SingleTransactionKind::Genesis(_)
                    | SingleTransactionKind::ConsensusCommitPrologue(_)
            )
        )
    }

    pub fn is_change_epoch_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::Single(SingleTransactionKind::ChangeEpoch(_))
        )
    }

    pub fn is_genesis_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::Single(SingleTransactionKind::Genesis(_))
        )
    }

    pub fn validity_check(&self, config: &ProtocolConfig, gas: &[ObjectRef]) -> UserInputResult {
        match self {
            TransactionKind::Batch(b) => {
                fp_ensure!(
                    !b.is_empty(),
                    UserInputError::InvalidBatchTransaction {
                        error: "Batch Transaction cannot be empty".to_string(),
                    }
                );
                fp_ensure!(
                    b.len() <= config.max_tx_in_batch() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum transactions in a batch".to_string(),
                        value: config.max_tx_in_batch().to_string()
                    }
                );
                // Check that all transaction kinds can be in a batch.
                let valid = b.iter().all(|s| match s {
                    SingleTransactionKind::Call(_)
                    | SingleTransactionKind::TransferObject(_)
                    | SingleTransactionKind::Pay(_) => true,
                    SingleTransactionKind::TransferSui(_)
                    | SingleTransactionKind::PaySui(_)
                    | SingleTransactionKind::PayAllSui(_)
                    | SingleTransactionKind::ChangeEpoch(_)
                    | SingleTransactionKind::Genesis(_)
                    | SingleTransactionKind::Publish(_)
                    | SingleTransactionKind::ConsensusCommitPrologue(_)
                    | SingleTransactionKind::ProgrammableTransaction(_) => false,
                });
                fp_ensure!(
                    valid,
                    UserInputError::InvalidBatchTransaction {
                        error: "Batch transaction contains non-batchable transactions. Only Call,
                        Pay and TransferObject are allowed"
                            .to_string()
                    }
                );
                for s in b {
                    s.validity_check(config, gas)?
                }
            }
            TransactionKind::Single(s) => s.validity_check(config, gas)?,
        }
        Ok(())
    }
}

impl Display for TransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match &self {
            Self::Single(s) => {
                write!(writer, "{}", s)?;
            }
            Self::Batch(b) => {
                writeln!(writer, "Transaction Kind : Batch")?;
                writeln!(writer, "List of transactions in the batch:")?;
                for kind in b {
                    writeln!(writer, "{}", kind)?;
                }
            }
        }
        write!(f, "{}", writer)
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct GasData {
    pub payment: Vec<ObjectRef>,
    pub owner: SuiAddress,
    pub price: u64,
    pub budget: u64,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum TransactionExpiration {
    /// The transaction has no expiration
    None,
    /// Validators wont sign a transaction unless the expiration Epoch
    /// is greater than or equal to the current epoch
    Epoch(EpochId),
}

#[enum_dispatch(TransactionDataAPI)]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum TransactionData {
    V1(TransactionDataV1),
}

impl VersionedProtocolMessage for TransactionData {
    fn message_version(&self) -> Option<u64> {
        Some(match self {
            Self::V1(_) => 1,
        })
    }

    fn check_version_supported(&self, current_protocol_version: ProtocolVersion) -> SuiResult {
        let (message_version, supported) = match self {
            Self::V1(_) => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Suppose we add V2 at protocol version 7, then we must change this to:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Self::V2 => (2, SupportedProtocolVersions::new_for_message(7, u64::MAX)),
            //
            // Suppose we remove support for V1 after protocol version 12: we can do it like so:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, 12)),
        };

        if supported.is_version_supported(current_protocol_version) {
            Ok(())
        } else {
            Err(SuiError::WrongMessageVersion {
                message_version,
                supported,
                current_protocol_version,
            })
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransactionDataV1 {
    pub kind: TransactionKind,
    pub sender: SuiAddress,
    pub gas_data: GasData,
    pub expiration: TransactionExpiration,
}

impl TransactionData {
    pub fn new_with_dummy_gas_price(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: DUMMY_GAS_PRICE,
                owner: sender,
                payment: vec![gas_payment],
                budget: gas_budget,
            },
            expiration: TransactionExpiration::None,
        })
    }

    pub fn new_system_transaction(kind: TransactionKind) -> Self {
        // assert transaction kind if a system transaction?
        // assert!(kind.is_system_tx());
        let sender = SuiAddress::default();
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: DUMMY_GAS_PRICE,
                owner: sender,
                payment: vec![(ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN)],
                budget: 0,
            },
            expiration: TransactionExpiration::None,
        })
    }

    pub fn new(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: gas_price,
                owner: sender,
                payment: vec![gas_payment],
                budget: gas_budget,
            },
            expiration: TransactionExpiration::None,
        })
    }

    pub fn new_with_gas_coins(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: Vec<ObjectRef>,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: gas_price,
                owner: sender,
                payment: gas_payment,
                budget: gas_budget,
            },
            expiration: TransactionExpiration::None,
        })
    }

    pub fn new_with_gas_data(kind: TransactionKind, sender: SuiAddress, gas_data: GasData) -> Self {
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data,
            expiration: TransactionExpiration::None,
        })
    }

    pub fn new_move_call_with_dummy_gas_price(
        sender: SuiAddress,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_payment: ObjectRef,
        arguments: Vec<CallArg>,
        gas_budget: u64,
    ) -> Self {
        Self::new_move_call(
            sender,
            package,
            module,
            function,
            type_arguments,
            gas_payment,
            arguments,
            gas_budget,
            DUMMY_GAS_PRICE,
        )
    }

    pub fn new_move_call(
        sender: SuiAddress,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_payment: ObjectRef,
        arguments: Vec<CallArg>,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::Call(MoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_move_call_with_gas_coins(
        sender: SuiAddress,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_payment: Vec<ObjectRef>,
        arguments: Vec<CallArg>,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::Call(MoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        }));
        Self::new_with_gas_coins(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_transfer_with_dummy_gas_price(
        recipient: SuiAddress,
        object_ref: ObjectRef,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        Self::new_transfer(
            recipient,
            object_ref,
            sender,
            gas_payment,
            gas_budget,
            DUMMY_GAS_PRICE,
        )
    }

    pub fn new_transfer(
        recipient: SuiAddress,
        object_ref: ObjectRef,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::TransferObject(TransferObject {
            recipient,
            object_ref,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_transfer_sui_with_dummy_gas_price(
        recipient: SuiAddress,
        sender: SuiAddress,
        amount: Option<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        Self::new_transfer_sui(
            recipient,
            sender,
            amount,
            gas_payment,
            gas_budget,
            DUMMY_GAS_PRICE,
        )
    }

    pub fn new_transfer_sui(
        recipient: SuiAddress,
        sender: SuiAddress,
        amount: Option<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::TransferSui(TransferSui {
            recipient,
            amount,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_pay_with_dummy_gas_price(
        sender: SuiAddress,
        coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        Self::new_pay(
            sender,
            coins,
            recipients,
            amounts,
            gas_payment,
            gas_budget,
            DUMMY_GAS_PRICE,
        )
    }

    pub fn new_pay(
        sender: SuiAddress,
        coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::Pay(Pay {
            coins,
            recipients,
            amounts,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_pay_sui_with_dummy_gas_price(
        sender: SuiAddress,
        coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        Self::new_pay_sui(
            sender,
            coins,
            recipients,
            amounts,
            gas_payment,
            gas_budget,
            DUMMY_GAS_PRICE,
        )
    }

    pub fn new_pay_sui(
        sender: SuiAddress,
        coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::PaySui(PaySui {
            coins,
            recipients,
            amounts,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_pay_all_sui(
        sender: SuiAddress,
        coins: Vec<ObjectRef>,
        recipient: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::PayAllSui(PayAllSui {
            coins,
            recipient,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_module_with_dummy_gas_price(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        modules: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Self {
        Self::new_module(sender, gas_payment, modules, gas_budget, DUMMY_GAS_PRICE)
    }

    pub fn new_module(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        modules: Vec<Vec<u8>>,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::Publish(MoveModulePublish {
            modules,
        }));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn new_programmable_with_dummy_gas_price(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        pt: ProgrammableTransaction,
        gas_budget: u64,
    ) -> Self {
        Self::new_programmable(sender, gas_payment, pt, gas_budget, DUMMY_GAS_PRICE)
    }

    pub fn new_programmable(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        pt: ProgrammableTransaction,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let kind = TransactionKind::Single(SingleTransactionKind::ProgrammableTransaction(pt));
        Self::new(kind, sender, gas_payment, gas_budget, gas_price)
    }

    pub fn execution_parts(&self) -> (TransactionKind, SuiAddress, Vec<ObjectRef>) {
        (
            self.kind().clone(),
            self.sender(),
            self.gas_data().payment.clone(),
        )
    }
}

#[enum_dispatch]
pub trait TransactionDataAPI {
    fn sender(&self) -> SuiAddress;

    // Note: this implies that TransactionKind itself must be versioned, so that it can be
    // shared across versions. This will be easy to do since it is already an enum.
    fn kind(&self) -> &TransactionKind;

    // Used by programmable_transaction_builder
    fn kind_mut(&mut self) -> &mut TransactionKind;

    // kind is moved out of often enough that this is worth it to special case.
    fn into_kind(self) -> TransactionKind;

    /// Transaction signer and Gas owner
    fn signers(&self) -> Vec<SuiAddress>;

    fn gas_data(&self) -> &GasData;

    fn gas_owner(&self) -> SuiAddress;

    fn gas(&self) -> &[ObjectRef];

    fn gas_price(&self) -> u64;

    fn gas_budget(&self) -> u64;

    fn expiration(&self) -> &TransactionExpiration;

    fn contains_shared_object(&self) -> bool;

    fn shared_input_objects(&self) -> Vec<SharedInputObject>;

    /// Actively being replaced by programmable transactions
    fn legacy_move_calls(&self) -> Vec<&MoveCall>;

    fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>>;

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult;

    /// Check if the transaction is compliant with sponsorship.
    fn check_sponsorship(&self) -> UserInputResult;

    fn is_system_tx(&self) -> bool;
    fn is_change_epoch_tx(&self) -> bool;
    fn is_genesis_tx(&self) -> bool;

    #[cfg(test)]
    fn sender_mut(&mut self) -> &mut SuiAddress;

    #[cfg(test)]
    fn gas_data_mut(&mut self) -> &mut GasData;

    // TODO: this should be #[cfg(test)], but for some reason it is not visible in
    // authority_tests.rs even though that entire module is #[cfg(test)]
    fn expiration_mut(&mut self) -> &mut TransactionExpiration;
}

impl TransactionDataAPI for TransactionDataV1 {
    fn sender(&self) -> SuiAddress {
        self.sender
    }

    fn kind(&self) -> &TransactionKind {
        &self.kind
    }

    fn kind_mut(&mut self) -> &mut TransactionKind {
        &mut self.kind
    }

    fn into_kind(self) -> TransactionKind {
        self.kind
    }

    /// Transaction signer and Gas owner
    fn signers(&self) -> Vec<SuiAddress> {
        let mut signers = vec![self.sender];
        if self.gas_owner() != self.sender {
            signers.push(self.gas_owner());
        }
        signers
    }

    fn gas_data(&self) -> &GasData {
        &self.gas_data
    }

    fn gas_owner(&self) -> SuiAddress {
        self.gas_data.owner
    }

    fn gas(&self) -> &[ObjectRef] {
        &self.gas_data.payment
    }

    fn gas_price(&self) -> u64 {
        self.gas_data.price
    }

    fn gas_budget(&self) -> u64 {
        self.gas_data.budget
    }

    fn expiration(&self) -> &TransactionExpiration {
        &self.expiration
    }

    fn contains_shared_object(&self) -> bool {
        self.kind.shared_input_objects().next().is_some()
    }

    fn shared_input_objects(&self) -> Vec<SharedInputObject> {
        self.kind.shared_input_objects().collect()
    }

    fn legacy_move_calls(&self) -> Vec<&MoveCall> {
        self.kind
            .single_transactions()
            .flat_map(|s| s.legacy_move_call())
            .collect()
    }

    fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let mut inputs = self.kind.input_objects()?;

        if !self.kind.is_system_tx() && !self.kind.is_pay_sui_tx() {
            inputs.extend(
                self.gas()
                    .iter()
                    .map(|obj_ref| InputObjectKind::ImmOrOwnedMoveObject(*obj_ref)),
            );
        }
        Ok(inputs)
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        self.kind().validity_check(config, self.gas())?;
        self.check_sponsorship()
    }

    /// Check if the transaction is compliant with sponsorship.
    fn check_sponsorship(&self) -> UserInputResult {
        // Not a sponsored transaction, nothing to check
        if self.gas_owner() == self.sender() {
            return Ok(());
        }
        let allow_sponsored_tx = match &self.kind {
            // For the sake of simplicity, we do not allow batched transaction
            // to be sponsored.
            TransactionKind::Batch(_b) => false,
            TransactionKind::Single(s) => match s {
                SingleTransactionKind::Call(_)
                | SingleTransactionKind::TransferObject(_)
                | SingleTransactionKind::Pay(_)
                | SingleTransactionKind::Publish(_) => true,
                SingleTransactionKind::TransferSui(_)
                | SingleTransactionKind::PaySui(_)
                | SingleTransactionKind::PayAllSui(_)
                | SingleTransactionKind::ChangeEpoch(_)
                | SingleTransactionKind::ConsensusCommitPrologue(_)
                | SingleTransactionKind::ProgrammableTransaction(_)
                | SingleTransactionKind::Genesis(_) => false,
            },
        };
        if allow_sponsored_tx {
            return Ok(());
        }
        Err(UserInputError::UnsupportedSponsoredTransactionKind)
    }

    fn is_change_epoch_tx(&self) -> bool {
        self.kind.is_change_epoch_tx()
    }

    fn is_system_tx(&self) -> bool {
        self.kind.is_system_tx()
    }

    fn is_genesis_tx(&self) -> bool {
        self.kind.is_genesis_tx()
    }

    #[cfg(test)]
    fn sender_mut(&mut self) -> &mut SuiAddress {
        &mut self.sender
    }

    #[cfg(test)]
    fn gas_data_mut(&mut self) -> &mut GasData {
        &mut self.gas_data
    }

    fn expiration_mut(&mut self) -> &mut TransactionExpiration {
        &mut self.expiration
    }
}

impl TransactionDataV1 {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SenderSignedData {
    pub intent_message: IntentMessage<TransactionData>,
    /// A list of signatures signed by all transaction participants.
    /// 1. non participant signature must not be present.
    /// 2. signature order does not matter.
    pub tx_signatures: Vec<GenericSignature>,
}

impl SenderSignedData {
    pub fn new(
        tx_data: TransactionData,
        intent: Intent,
        tx_signatures: Vec<GenericSignature>,
    ) -> Self {
        Self {
            intent_message: IntentMessage::new(intent, tx_data),
            tx_signatures,
        }
    }

    pub fn new_from_sender_signature(
        tx_data: TransactionData,
        intent: Intent,
        tx_signature: Signature,
    ) -> Self {
        Self {
            intent_message: IntentMessage::new(intent, tx_data),
            tx_signatures: vec![tx_signature.into()],
        }
    }

    // This function does not check validity of the signature
    // or perform any de-dup checks.
    pub fn add_signature(&mut self, new_signature: Signature) {
        self.tx_signatures.push(new_signature.into());
    }

    fn get_signer_sig_mapping(&self) -> SuiResult<BTreeMap<SuiAddress, &GenericSignature>> {
        let mut mapping = BTreeMap::new();
        for sig in &self.tx_signatures {
            let address = sig.try_into()?;
            mapping.insert(address, sig);
        }
        Ok(mapping)
    }

    pub fn transaction_data(&self) -> &TransactionData {
        &self.intent_message.value
    }
}

impl VersionedProtocolMessage for SenderSignedData {
    fn message_version(&self) -> Option<u64> {
        self.transaction_data().message_version()
    }

    fn check_version_supported(&self, current_protocol_version: ProtocolVersion) -> SuiResult {
        self.transaction_data()
            .check_version_supported(current_protocol_version)?;

        // This code does nothing right now. Its purpose is to cause a compiler error when a
        // new signature type is added.
        //
        // When adding a new signature type, check if current_protocol_version
        // predates support for the new type. If it does, return
        // SuiError::WrongMessageVersion
        for sig in &self.tx_signatures {
            match sig {
                GenericSignature::MultiSig(_) | GenericSignature::Signature(_) => (),
            }
        }

        Ok(())
    }
}

impl Message for SenderSignedData {
    type DigestType = TransactionDigest;
    const SCOPE: IntentScope = IntentScope::SenderSignedTransaction;

    fn digest(&self) -> Self::DigestType {
        TransactionDigest::new(sha3_hash(&self.intent_message.value))
    }

    fn verify(&self) -> SuiResult {
        if self.intent_message.value.is_system_tx() {
            return Ok(());
        }

        // Verify signatures. Steps are ordered in asc complexity order to minimize abuse.
        let signers = self.intent_message.value.signers();
        // Signature number needs to match
        fp_ensure!(
            self.tx_signatures.len() == signers.len(),
            SuiError::SignerSignatureNumberMismatch {
                actual: self.tx_signatures.len(),
                expected: signers.len()
            }
        );
        // All required signers need to be sign.
        let present_sigs = self.get_signer_sig_mapping()?;
        for s in signers {
            if !present_sigs.contains_key(&s) {
                return Err(SuiError::SignerSignatureAbsent {
                    signer: s.to_string(),
                });
            }
        }

        // Verify all present signatures.
        for (signer, signature) in present_sigs {
            signature.verify_secure_generic(&self.intent_message, signer)?;
        }
        Ok(())
    }
}

impl<S> Envelope<SenderSignedData, S> {
    pub fn sender_address(&self) -> SuiAddress {
        self.data().intent_message.value.sender()
    }

    pub fn gas(&self) -> &[ObjectRef] {
        self.data().intent_message.value.gas()
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.data()
            .intent_message
            .value
            .shared_input_objects()
            .into_iter()
    }

    pub fn input_objects_in_compiled_modules(
        compiled_modules: &[CompiledModule],
    ) -> Vec<InputObjectKind> {
        let to_be_published: BTreeSet<_> = compiled_modules.iter().map(|m| m.self_id()).collect();
        let mut dependent_packages = BTreeSet::new();
        for module in compiled_modules {
            for handle in &module.module_handles {
                if !to_be_published.contains(&module.module_id_for_handle(handle)) {
                    let address = ObjectID::from(*module.address_identifier_at(handle.address));
                    dependent_packages.insert(address);
                }
            }
        }

        // We don't care about the digest of the dependent packages.
        // They are all read-only on-chain and their digest never changes.
        dependent_packages
            .into_iter()
            .map(InputObjectKind::MovePackage)
            .collect::<Vec<_>>()
    }

    pub fn is_system_tx(&self) -> bool {
        self.data().intent_message.value.is_system_tx()
    }
}

impl Transaction {
    pub fn from_data_and_signer(
        data: TransactionData,
        intent: Intent,
        signers: Vec<&dyn Signer<Signature>>,
    ) -> Self {
        let intent_msg = IntentMessage::new(intent.clone(), data.clone());
        let mut signatures = Vec::with_capacity(signers.len());
        for signer in signers {
            signatures.push(Signature::new_secure(&intent_msg, signer));
        }
        Self::from_data(data, intent, signatures)
    }

    pub fn from_data(data: TransactionData, intent: Intent, signatures: Vec<Signature>) -> Self {
        Self::from_generic_sig_data(
            data,
            intent,
            signatures.into_iter().map(|s| s.into()).collect(),
        )
    }

    pub fn signature_from_signer(
        data: TransactionData,
        intent: Intent,
        signer: &dyn Signer<Signature>,
    ) -> Signature {
        let intent_msg = IntentMessage::new(intent, data);
        Signature::new_secure(&intent_msg, signer)
    }

    pub fn from_generic_sig_data(
        data: TransactionData,
        intent: Intent,
        signatures: Vec<GenericSignature>,
    ) -> Self {
        Self::new(SenderSignedData::new(data, intent, signatures))
    }

    /// Returns the Base64 encoded tx_bytes
    /// and a list of Base64 encoded [enum GenericSignature].
    pub fn to_tx_bytes_and_signatures(&self) -> (Base64, Vec<Base64>) {
        (
            Base64::from_bytes(&bcs::to_bytes(&self.data().intent_message.value).unwrap()),
            self.data()
                .tx_signatures
                .iter()
                .map(|s| Base64::from_bytes(s.as_ref()))
                .collect(),
        )
    }
}

impl VerifiedTransaction {
    pub fn new_change_epoch(
        next_epoch: EpochId,
        protocol_version: ProtocolVersion,
        storage_charge: u64,
        computation_charge: u64,
        storage_rebate: u64,
        epoch_start_timestamp_ms: u64,
        system_packages: Vec<(SequenceNumber, Vec<Vec<u8>>)>,
    ) -> Self {
        ChangeEpoch {
            epoch: next_epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            epoch_start_timestamp_ms,
            system_packages,
        }
        .pipe(SingleTransactionKind::ChangeEpoch)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_genesis_transaction(objects: Vec<GenesisObject>) -> Self {
        GenesisTransaction { objects }
            .pipe(SingleTransactionKind::Genesis)
            .pipe(Self::new_system_transaction)
    }

    pub fn new_consensus_commit_prologue(
        epoch: u64,
        round: u64,
        commit_timestamp_ms: CheckpointTimestamp,
    ) -> Self {
        ConsensusCommitPrologue {
            epoch,
            round,
            commit_timestamp_ms,
        }
        .pipe(SingleTransactionKind::ConsensusCommitPrologue)
        .pipe(Self::new_system_transaction)
    }

    fn new_system_transaction(system_transaction: SingleTransactionKind) -> Self {
        system_transaction
            .pipe(TransactionKind::Single)
            .pipe(TransactionData::new_system_transaction)
            .pipe(|data| {
                SenderSignedData::new_from_sender_signature(
                    data,
                    Intent::default(),
                    Ed25519SuiSignature::from_bytes(&[0; Ed25519SuiSignature::LENGTH])
                        .unwrap()
                        .into(),
                )
            })
            .pipe(Transaction::new)
            .pipe(Self::new_from_verified)
    }
}

impl VerifiedSignedTransaction {
    /// Use signing key to create a signed object.
    pub fn new(
        epoch: EpochId,
        transaction: VerifiedTransaction,
        authority: AuthorityName,
        secret: &dyn Signer<AuthoritySignature>,
    ) -> Self {
        Self::new_from_verified(SignedTransaction::new(
            epoch,
            transaction.into_inner().into_data(),
            secret,
            authority,
        ))
    }
}

/// A transaction that is signed by a sender but not yet by an authority.
pub type Transaction = Envelope<SenderSignedData, EmptySignInfo>;
pub type VerifiedTransaction = VerifiedEnvelope<SenderSignedData, EmptySignInfo>;
pub type TrustedTransaction = TrustedEnvelope<SenderSignedData, EmptySignInfo>;

/// A transaction that is signed by a sender and also by an authority.
pub type SignedTransaction = Envelope<SenderSignedData, AuthoritySignInfo>;
pub type VerifiedSignedTransaction = VerifiedEnvelope<SenderSignedData, AuthoritySignInfo>;

pub type CertifiedTransaction = Envelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TxCertAndSignedEffects = (
    CertifiedTransaction,
    SignedTransactionEffects,
    TransactionEvents,
);

pub type VerifiedCertificate = VerifiedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TrustedCertificate = TrustedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;

/// An ExecutableTransaction is a wrapper of a transaction with a CertificateProof that indicates
/// there existed a valid certificate for this transaction, and hence it can be executed locally.
/// This is an abstraction data structure to cover both the case where the transaction is
/// certified or checkpointed when we schedule it for execution.
pub type ExecutableTransaction = Envelope<SenderSignedData, CertificateProof>;
pub type VerifiedExecutableTransaction = VerifiedEnvelope<SenderSignedData, CertificateProof>;
pub type TrustedExecutableTransaction = TrustedEnvelope<SenderSignedData, CertificateProof>;

impl VerifiedExecutableTransaction {
    pub fn certificate_sig(&self) -> Option<&AuthorityStrongQuorumSignInfo> {
        match self.auth_sig() {
            CertificateProof::Certified(sig) => Some(sig),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum ObjectInfoRequestKind {
    /// Request the latest object state.
    LatestObjectInfo,
    /// Request a specific version of the object.
    /// This is used only for debugging purpose and will not work as a generic solution
    /// since we don't keep around all historic object versions.
    /// No production code should depend on this kind.
    PastObjectInfoDebug(SequenceNumber),
}

/// A request for information about an object and optionally its
/// parent certificate at a specific version.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ObjectInfoRequest {
    /// The id of the object to retrieve, at the latest version.
    pub object_id: ObjectID,
    /// if a format option is provided, return the layout of the object in the given format.
    pub object_format_options: Option<ObjectFormatOptions>,
    /// The type of request, either latest object info or the past.
    pub request_kind: ObjectInfoRequestKind,
}

impl ObjectInfoRequest {
    pub fn past_object_info_debug_request(
        object_id: ObjectID,
        version: SequenceNumber,
        layout: Option<ObjectFormatOptions>,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            object_format_options: layout,
            request_kind: ObjectInfoRequestKind::PastObjectInfoDebug(version),
        }
    }

    pub fn latest_object_info_request(
        object_id: ObjectID,
        layout: Option<ObjectFormatOptions>,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            object_format_options: layout,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo,
        }
    }
}

/// This message provides information about the latest object and its lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfoResponse {
    /// Value of the requested object in this authority
    pub object: Object,
    /// Schema of the Move value inside this object.
    /// None if the object is a Move package, or the request did not ask for the layout
    pub layout: Option<MoveStructLayout>,
    /// Transaction the object is locked on in this authority.
    /// None if the object is not currently locked by this authority.
    /// This should be only used for debugging purpose, such as from sui-tool. No prod clients should
    /// rely on it.
    pub lock_for_debugging: Option<SignedTransaction>,
}

/// Verified version of `ObjectInfoResponse`. `layout` and `lock_for_debugging` are skipped because they
/// are not needed and we don't want to verify them.
#[derive(Debug, Clone)]
pub struct VerifiedObjectInfoResponse {
    /// Value of the requested object in this authority
    pub object: Object,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInfoRequest {
    pub transaction_digest: TransactionDigest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Signature over the transaction.
    Signed(AuthoritySignInfo),
    /// For executed transaction, we could return an optional certificate signature on the transaction
    /// (i.e. the signature part of the CertifiedTransaction), as well as the signed effects.
    /// The certificate signature is optional because for transactions executed in previous
    /// epochs, we won't keep around the certificate signatures.
    Executed(
        Option<AuthorityStrongQuorumSignInfo>,
        SignedTransactionEffects,
        TransactionEvents,
    ),
}

impl TransactionStatus {
    pub fn into_signed_for_testing(self) -> AuthoritySignInfo {
        match self {
            Self::Signed(s) => s,
            _ => unreachable!("Incorrect response type"),
        }
    }

    pub fn into_effects_for_testing(self) -> SignedTransactionEffects {
        match self {
            Self::Executed(_, e, _) => e,
            _ => unreachable!("Incorrect response type"),
        }
    }
}

impl PartialEq for TransactionStatus {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Signed(s1) => match other {
                Self::Signed(s2) => s1.epoch == s2.epoch,
                _ => false,
            },
            Self::Executed(c1, e1, ev1) => match other {
                Self::Executed(c2, e2, ev2) => {
                    c1.as_ref().map(|a| a.epoch) == c2.as_ref().map(|a| a.epoch)
                        && e1.epoch() == e2.epoch()
                        && e1.digest() == e2.digest()
                        && ev1.digest() == ev2.digest()
                }
                _ => false,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleTransactionResponse {
    pub status: TransactionStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionInfoResponse {
    pub transaction: SenderSignedData,
    pub status: TransactionStatus,
}

#[derive(Clone, Debug)]
pub enum VerifiedTransactionInfoResponse {
    Signed(VerifiedSignedTransaction),
    ExecutedWithCert(
        VerifiedCertificate,
        VerifiedSignedTransactionEffects,
        TransactionEvents,
    ),
    ExecutedWithoutCert(
        VerifiedTransaction,
        VerifiedSignedTransactionEffects,
        TransactionEvents,
    ),
}

impl VerifiedTransactionInfoResponse {
    pub fn is_executed(&self) -> bool {
        match self {
            VerifiedTransactionInfoResponse::Signed(_) => false,
            VerifiedTransactionInfoResponse::ExecutedWithCert(_, _, _)
            | VerifiedTransactionInfoResponse::ExecutedWithoutCert(_, _, _) => true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleCertificateResponse {
    pub signed_effects: SignedTransactionEffects,
    // TODO: Add a case for finalized transaction.
    pub events: TransactionEvents,
}

#[derive(Clone, Debug)]
pub struct VerifiedHandleCertificateResponse {
    pub signed_effects: VerifiedSignedTransactionEffects,
    pub events: TransactionEvents,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum CallResult {
    Bool(bool),
    U8(u8),
    U64(u64),
    U128(u128),
    Address(AccountAddress),
    // these are not ideal but there is no other way to deserialize
    // vectors encoded in BCS (you need a full type before this can be
    // done)
    BoolVec(Vec<bool>),
    U8Vec(Vec<u8>),
    U64Vec(Vec<u64>),
    U128Vec(Vec<u128>),
    AddrVec(Vec<AccountAddress>),
    BoolVecVec(Vec<bool>),
    U8VecVec(Vec<Vec<u8>>),
    U64VecVec(Vec<Vec<u64>>),
    U128VecVec(Vec<Vec<u128>>),
    AddrVecVec(Vec<Vec<AccountAddress>>),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    /// Gas used in the failed case, and the error.
    Failure {
        /// The error
        error: ExecutionFailureStatus,
        /// Which command the error occurred
        command: Option<CommandIndex>,
    },
}

pub type CommandIndex = usize;

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionFailureStatus {
    //
    // General transaction errors
    //
    InsufficientGas,
    InvalidGasObject,
    InvalidTransactionUpdate,
    FunctionNotFound,
    InvariantViolation,
    MoveObjectTooBig {
        object_size: u64,
        max_object_size: u64,
    },
    MovePackageTooBig {
        object_size: u64,
        max_object_size: u64,
    },

    //
    // Transfer errors
    //
    InvalidTransferObject,
    InvalidTransferSui,
    InvalidTransferSuiInsufficientBalance,
    InvalidCoinObject,

    //
    // Pay errors
    //
    /// Supplied 0 input coins
    EmptyInputCoins,
    /// Supplied an empty list of recipient addresses for the payment
    EmptyRecipients,
    /// Supplied a different number of recipient addresses and recipient amounts
    RecipientsAmountsArityMismatch,
    /// Not enough funds to perform the requested payment
    InsufficientBalance,
    /// Coin type check failed in pay/pay_sui/pay_all_sui transaction.
    /// In pay transaction, it means the input coins' types are not the same;
    /// In PaySui/PayAllSui, it means some input coins are not SUI coins.
    CoinTypeMismatch,
    CoinTooLarge,

    //
    // MoveCall errors
    //
    NonEntryFunctionInvoked,
    EntryTypeArityMismatch,
    EntryArgumentError(EntryArgumentError),
    EntryTypeArgumentError(EntryTypeArgumentError),
    CircularObjectOwnership(CircularObjectOwnership),
    InvalidChildObjectArgument(InvalidChildObjectArgument),
    InvalidSharedByValue(InvalidSharedByValue),
    TooManyChildObjects {
        object: ObjectID,
    },
    InvalidParentDeletion {
        parent: ObjectID,
        kind: Option<DeleteKind>,
    },
    InvalidParentFreezing {
        parent: ObjectID,
    },

    //
    // MovePublish errors
    //
    PublishErrorEmptyPackage,
    PublishErrorNonZeroAddress,
    PublishErrorDuplicateModule,
    SuiMoveVerificationError,

    //
    // Errors from the Move VM
    //
    // Indicates an error from a non-abort instruction
    MovePrimitiveRuntimeError(Option<MoveLocation>),
    /// Indicates and `abort` from inside Move code. Contains the location of the abort and the
    /// abort code
    MoveAbort(MoveLocation, u64), // TODO func def + offset?
    VMVerificationOrDeserializationError,
    VMInvariantViolation,

    /// The total amount of coins to be paid is larger than the maximum value of u64.
    TotalPaymentAmountOverflow,
    /// The total balance of coins is larger than the maximum value of u64.
    TotalCoinBalanceOverflow,

    //
    // Programmable Transaction Errors
    //
    CommandArgumentError {
        arg_idx: u16,
        kind: CommandArgumentError,
    },
    UnusedValueWithoutDrop {
        result_idx: u16,
        secondary_idx: u16,
    },
    InvalidPublicFunctionReturnType {
        idx: u16,
    },
    ArityMismatch,
    // NOTE: if you want to add a new enum,
    // please add it at the end for Rust SDK backward compatibility.
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash)]
pub struct MoveLocation {
    pub module: ModuleId,
    pub function: u16,
    pub instruction: CodeOffset,
    pub function_name: Option<String>,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct EntryArgumentError {
    pub argument_idx: LocalIndex,
    pub kind: EntryArgumentErrorKind,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum EntryArgumentErrorKind {
    TypeMismatch,
    InvalidObjectByValue,
    InvalidObjectByMuteRef,
    ObjectKindMismatch,
    UnsupportedPureArg,
    ArityMismatch,
    ObjectMutabilityMismatch,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct EntryTypeArgumentError {
    pub argument_idx: TypeParameterIndex,
    pub kind: EntryTypeArgumentErrorKind,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum EntryTypeArgumentErrorKind {
    TypeNotFound,
    ArityMismatch,
    ConstraintNotSatisfied,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct CircularObjectOwnership {
    pub object: ObjectID,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct InvalidChildObjectArgument {
    pub child: ObjectID,
    pub parent: SuiAddress,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct InvalidSharedByValue {
    pub object: ObjectID,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash, Error)]
pub enum CommandArgumentError {
    #[error("The type of the value does not match the expected type")]
    TypeMismatch,
    #[error("The argument cannot be deserialized into a value of the specified type")]
    InvalidBCSBytes,
    #[error("The argument cannot be instantiated from raw bytes")]
    InvalidUsageOfPureArg,
    #[error(
        "Invalid argument to private entry function. \
        These functions cannot take arguments from other Move functions"
    )]
    InvalidArgumentToPrivateEntryFunction,
    #[error("Out of bounds access to input or result vector {idx}")]
    IndexOutOfBounds { idx: u16 },
    #[error(
        "Out of bounds secondary access to result vector \
        {result_idx} at secondary index {secondary_idx}"
    )]
    SecondaryIndexOutOfBounds { result_idx: u16, secondary_idx: u16 },
    #[error(
        "Invalid usage of result {result_idx}, \
        expected a single result but found multiple return values"
    )]
    InvalidResultArity { result_idx: u16 },
    #[error(
        "Invalid taking of the Gas coin. \
        It can only be used by-value with TransferObjects"
    )]
    InvalidGasCoinUsage,
    #[error(
        "Invalid usage of borrowed value. \
        Mutably borrowed values require unique usage. \
        Immutably borrowed values cannot be taken or borrowed mutably"
    )]
    InvalidUsageOfBorrowedValue,
    #[error(
        "Invalid usage of already taken value. \
        There is now no value available at this location"
    )]
    InvalidUsageOfTakenValue,
    #[error("Immutable and shared objects cannot be passed by-value.")]
    InvalidObjectByValue,
    #[error("Immutable objects cannot be passed by mutable reference, &mut.")]
    InvalidObjectByMutRef,
}

impl ExecutionFailureStatus {
    pub fn entry_argument_error(argument_idx: LocalIndex, kind: EntryArgumentErrorKind) -> Self {
        EntryArgumentError { argument_idx, kind }.into()
    }

    pub fn entry_type_argument_error(
        argument_idx: TypeParameterIndex,
        kind: EntryTypeArgumentErrorKind,
    ) -> Self {
        EntryTypeArgumentError { argument_idx, kind }.into()
    }

    pub fn circular_object_ownership(object: ObjectID) -> Self {
        CircularObjectOwnership { object }.into()
    }

    pub fn invalid_child_object_argument(child: ObjectID, parent: SuiAddress) -> Self {
        InvalidChildObjectArgument { child, parent }.into()
    }

    pub fn invalid_shared_by_value(object: ObjectID) -> Self {
        InvalidSharedByValue { object }.into()
    }

    pub fn command_argument_error(kind: CommandArgumentError, arg_idx: u16) -> Self {
        Self::CommandArgumentError { arg_idx, kind }
    }
}

impl Display for ExecutionFailureStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionFailureStatus::CoinTypeMismatch => {
                write!(
                    f,
                    "Coin type check failed in pay/pay_sui/pay_all_sui transaction"
                )
            }
            ExecutionFailureStatus::CoinTooLarge => {
                write!(f, "Coin exceeds maximum value for a single coin")
            }
            ExecutionFailureStatus::EmptyInputCoins => {
                write!(f, "Expected a non-empty list of input Coin objects")
            }
            ExecutionFailureStatus::EmptyRecipients => {
                write!(f, "Expected a non-empty list of recipient addresses")
            }
            ExecutionFailureStatus::InsufficientBalance => write!(
                f,
                "Value of input coins is insufficient to cover outgoing amounts"
            ),
            ExecutionFailureStatus::InsufficientGas => write!(f, "Insufficient Gas."),
            ExecutionFailureStatus::InvalidGasObject => {
                write!(
                    f,
                    "Invalid Gas Object. Possibly not address-owned or possibly not a SUI coin."
                )
            }
            ExecutionFailureStatus::InvalidTransactionUpdate => {
                write!(f, "Invalid Transaction Update.")
            }
            ExecutionFailureStatus::MoveObjectTooBig {
                object_size,
                max_object_size,
            } => write!(
                f,
                "Move object with size {object_size} is larger \
                than the maximum object size {max_object_size}"
            ),
            ExecutionFailureStatus::MovePackageTooBig {
                object_size,
                max_object_size,
            } => write!(
                f,
                "Move package with size {object_size} is larger than the \
                maximum object size {max_object_size}"
            ),
            ExecutionFailureStatus::FunctionNotFound => write!(f, "Function Not Found."),
            ExecutionFailureStatus::InvariantViolation => write!(f, "INVARIANT VIOLATION."),
            ExecutionFailureStatus::InvalidTransferObject => write!(
                f,
                "Invalid Transfer Object Transaction. \
                Possibly not address-owned or possibly does not have public transfer."
            ),
            ExecutionFailureStatus::InvalidCoinObject => {
                write!(f, "Invalid coin::Coin object bytes.")
            }
            ExecutionFailureStatus::InvalidTransferSui => write!(
                f,
                "Invalid Transfer SUI. \
                Possibly not address-owned or possibly not a SUI coin."
            ),
            ExecutionFailureStatus::InvalidTransferSuiInsufficientBalance => {
                write!(f, "Invalid Transfer SUI, Insufficient Balance.")
            }
            ExecutionFailureStatus::NonEntryFunctionInvoked => write!(
                f,
                "Non Entry Function Invoked. Move Call must start with an entry function"
            ),
            ExecutionFailureStatus::EntryTypeArityMismatch => write!(
                f,
                "Number of type arguments does not match the expected value",
            ),
            ExecutionFailureStatus::EntryArgumentError(data) => {
                write!(f, "Entry Argument Error. {data}")
            }
            ExecutionFailureStatus::EntryTypeArgumentError(data) => {
                write!(f, "Entry Type Argument Error. {data}")
            }
            ExecutionFailureStatus::CircularObjectOwnership(data) => {
                write!(f, "Circular  Object Ownership. {data}")
            }
            ExecutionFailureStatus::InvalidChildObjectArgument(data) => {
                write!(f, "Invalid Object Owned Argument. {data}")
            }
            ExecutionFailureStatus::InvalidSharedByValue(data) => {
                write!(f, "Invalid Shared Object By-Value Usage. {data}.")
            }
            ExecutionFailureStatus::RecipientsAmountsArityMismatch => write!(
                f,
                "Expected recipient and amounts lists to be the same length"
            ),
            ExecutionFailureStatus::TooManyChildObjects { object } => {
                write!(
                    f,
                    "Object {object} has too many child objects. \
                    The number of child objects cannot exceed 2^32 - 1."
                )
            }
            ExecutionFailureStatus::InvalidParentDeletion { parent, kind } => {
                let method = match kind {
                    Some(DeleteKind::Normal) => "deleted",
                    Some(DeleteKind::UnwrapThenDelete) => "unwrapped then deleted",
                    Some(DeleteKind::Wrap) => "wrapped in another object",
                    None => "created and destroyed",
                };
                write!(
                    f,
                    "Invalid Deletion of Parent Object with Children. Parent object {parent} was \
                    {method} before its children were deleted or transferred."
                )
            }
            ExecutionFailureStatus::InvalidParentFreezing { parent } => {
                write!(
                    f,
                    "Invalid Freezing of Parent Object with Children. Parent object {parent} was \
                    made immutable before its children were deleted or transferred."
                )
            }
            ExecutionFailureStatus::PublishErrorEmptyPackage => write!(
                f,
                "Publish Error, Empty Package. A package must have at least one module."
            ),
            ExecutionFailureStatus::PublishErrorNonZeroAddress => write!(
                f,
                "Publish Error, Non-zero Address. \
                The modules in the package must have their address set to zero."
            ),
            ExecutionFailureStatus::PublishErrorDuplicateModule => write!(
                f,
                "Publish Error, Duplicate Module. More than one module with a given name."
            ),
            ExecutionFailureStatus::SuiMoveVerificationError => write!(
                f,
                "Sui Move Bytecode Verification Error. \
                Please run the Sui Move Verifier for more information."
            ),
            ExecutionFailureStatus::MovePrimitiveRuntimeError(location) => {
                write!(f, "Move Primitive Runtime Error. Location: ")?;
                match location {
                    None => write!(f, "UNKNOWN")?,
                    Some(l) => write!(f, "{l}")?,
                }
                write!(
                    f,
                    ". Arithmetic error, stack overflow, max value depth, etc."
                )
            }
            ExecutionFailureStatus::MoveAbort(location, c) => {
                write!(
                    f,
                    "Move Runtime Abort. Location: {}, Abort Code: {}",
                    location, c
                )
            }
            ExecutionFailureStatus::VMVerificationOrDeserializationError => write!(
                f,
                "Move Bytecode Verification Error. \
                Please run the Bytecode Verifier for more information."
            ),
            ExecutionFailureStatus::VMInvariantViolation => {
                write!(f, "MOVE VM INVARIANT VIOLATION.")
            }
            ExecutionFailureStatus::TotalPaymentAmountOverflow => {
                write!(f, "The total amount of coins to be paid overflows of u64")
            }
            ExecutionFailureStatus::TotalCoinBalanceOverflow => {
                write!(f, "The total balance of coins overflows u64")
            }
            ExecutionFailureStatus::CommandArgumentError { arg_idx, kind } => {
                write!(f, "Invalid command argument at {arg_idx}. {kind}")
            }
            ExecutionFailureStatus::UnusedValueWithoutDrop {
                result_idx,
                secondary_idx,
            } => {
                write!(
                    f,
                    "Unused result without the drop ability. \
                    Command result {result_idx}, return value {secondary_idx}"
                )
            }
            ExecutionFailureStatus::InvalidPublicFunctionReturnType { idx } => {
                write!(
                    f,
                    "Invalid public Move function signature. \
                    Unsupported return type for return value {idx}"
                )
            }
            ExecutionFailureStatus::ArityMismatch => {
                write!(
                    f,
                    "Arity mismatch for Move function. \
                    The number of arguments does not match the number of parameters"
                )
            }
        }
    }
}

impl Display for MoveLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            module,
            function,
            instruction,
            function_name,
        } = self;
        if let Some(fname) = function_name {
            write!(
                f,
                "{module}::{fname} (function index {function}) at offset {instruction}"
            )
        } else {
            write!(
                f,
                "{module} in function definition {function} at offset {instruction}"
            )
        }
    }
}

impl Display for EntryArgumentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let EntryArgumentError { argument_idx, kind } = self;
        write!(f, "Error for argument at index {argument_idx}: {kind}",)
    }
}

impl Display for EntryArgumentErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryArgumentErrorKind::TypeMismatch => write!(f, "Type mismatch."),
            EntryArgumentErrorKind::InvalidObjectByValue => {
                write!(f, "Immutable and shared objects cannot be passed by-value.")
            }
            EntryArgumentErrorKind::InvalidObjectByMuteRef => {
                write!(
                    f,
                    "Immutable objects cannot be passed by mutable reference, &mut."
                )
            }
            EntryArgumentErrorKind::ObjectKindMismatch => {
                write!(f, "Mismatch with object argument kind and its actual kind.")
            }
            EntryArgumentErrorKind::UnsupportedPureArg => write!(
                f,
                "Unsupported non-object argument; if it is an object, it must be \
                populated by an object ID."
            ),
            EntryArgumentErrorKind::ArityMismatch => {
                write!(
                    f,
                    "Mismatch between the number of actual versus expected arguments."
                )
            }
            EntryArgumentErrorKind::ObjectMutabilityMismatch => {
                write!(
                    f,
                    "Mismatch between the mutability of actual versus expected arguments."
                )
            }
        }
    }
}

impl Display for EntryTypeArgumentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let EntryTypeArgumentError { argument_idx, kind } = self;
        write!(f, "Error for type argument at index {argument_idx}: {kind}",)
    }
}

impl Display for EntryTypeArgumentErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryTypeArgumentErrorKind::TypeNotFound => {
                write!(f, "A type was not found in the module specified",)
            }
            EntryTypeArgumentErrorKind::ArityMismatch => write!(
                f,
                "Mismatch between the number of actual versus expected type arguments."
            ),
            EntryTypeArgumentErrorKind::ConstraintNotSatisfied => write!(
                f,
                "A type provided did not match the specified constraints."
            ),
        }
    }
}

impl Display for CircularObjectOwnership {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let CircularObjectOwnership { object } = self;
        write!(f, "Circular object ownership, including object {object}.")
    }
}

impl Display for InvalidChildObjectArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let InvalidChildObjectArgument { child, parent } = self;
        write!(
            f,
            "Object {child} is owned by object {parent}. \
            Objects owned by other objects cannot be used as input arguments."
        )
    }
}

impl Display for InvalidSharedByValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let InvalidSharedByValue { object } = self;
        write!(
            f,
        "When a shared object is passed as an owned Move value in an entry function, either the \
        the shared object's type must be defined in the same module as the called function. The \
        shared object {object} is not defined in this module",
        )
    }
}

impl std::error::Error for ExecutionFailureStatus {}

impl ExecutionStatus {
    pub fn new_failure(
        error: ExecutionFailureStatus,
        command: Option<CommandIndex>,
    ) -> ExecutionStatus {
        ExecutionStatus::Failure { error, command }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ExecutionStatus::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, ExecutionStatus::Failure { .. })
    }

    pub fn unwrap(&self) {
        match self {
            ExecutionStatus::Success => {}
            ExecutionStatus::Failure { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
        }
    }

    pub fn unwrap_err(self) -> (ExecutionFailureStatus, Option<CommandIndex>) {
        match self {
            ExecutionStatus::Success { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
            ExecutionStatus::Failure { error, command } => (error, command),
        }
    }
}

impl From<EntryArgumentError> for ExecutionFailureStatus {
    fn from(error: EntryArgumentError) -> Self {
        Self::EntryArgumentError(error)
    }
}

impl From<EntryTypeArgumentError> for ExecutionFailureStatus {
    fn from(error: EntryTypeArgumentError) -> Self {
        Self::EntryTypeArgumentError(error)
    }
}

impl From<CircularObjectOwnership> for ExecutionFailureStatus {
    fn from(error: CircularObjectOwnership) -> Self {
        Self::CircularObjectOwnership(error)
    }
}

impl From<InvalidChildObjectArgument> for ExecutionFailureStatus {
    fn from(error: InvalidChildObjectArgument) -> Self {
        Self::InvalidChildObjectArgument(error)
    }
}

impl From<InvalidSharedByValue> for ExecutionFailureStatus {
    fn from(error: InvalidSharedByValue) -> Self {
        Self::InvalidSharedByValue(error)
    }
}

pub trait VersionedProtocolMessage {
    /// Return version of message. Some messages depend on their enclosing messages to know the
    /// version number, so not every implementor implements this.
    fn message_version(&self) -> Option<u64> {
        None
    }

    /// Check that the version of the message is the correct one to use at this protocol version.
    fn check_version_supported(&self, current_protocol_version: ProtocolVersion) -> SuiResult;
}

/// The response from processing a transaction or a certified transaction
#[enum_dispatch(TransactionEffectsAPI)]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum TransactionEffects {
    V1(TransactionEffectsV1),
}

impl VersionedProtocolMessage for TransactionEffects {
    fn message_version(&self) -> Option<u64> {
        Some(match self {
            Self::V1(_) => 1,
        })
    }

    fn check_version_supported(&self, current_protocol_version: ProtocolVersion) -> SuiResult {
        let (message_version, supported) = match self {
            Self::V1(_) => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Suppose we add V2 at protocol version 7, then we must change this to:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Self::V2 => (2, SupportedProtocolVersions::new_for_message(7, u64::MAX)),
        };

        if supported.is_version_supported(current_protocol_version) {
            Ok(())
        } else {
            Err(SuiError::WrongMessageVersion {
                message_version,
                supported,
                current_protocol_version,
            })
        }
    }
}

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffectsV1 {
    /// The status of the execution
    pub status: ExecutionStatus,
    /// The epoch when this transaction was executed.
    pub executed_epoch: EpochId,
    pub gas_used: GasCostSummary,
    /// The version that every modified (mutated or deleted) object had before it was modified by
    /// this transaction.
    pub modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    /// The object references of the shared objects used in this transaction. Empty if no shared objects were used.
    pub shared_objects: Vec<ObjectRef>,
    /// The transaction digest
    pub transaction_digest: TransactionDigest,

    // TODO: All the SequenceNumbers in the ObjectRefs below equal the same value (the lamport
    // timestamp of the transaction).  Consider factoring this out into one place in the effects.
    /// ObjectRef and owner of new objects created.
    pub created: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of mutated objects, including gas object.
    pub mutated: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of objects that are unwrapped in this transaction.
    /// Unwrapped objects are objects that were wrapped into other objects in the past,
    /// and just got extracted out.
    pub unwrapped: Vec<(ObjectRef, Owner)>,
    /// Object Refs of objects now deleted (the old refs).
    pub deleted: Vec<ObjectRef>,
    /// Object refs of objects previously wrapped in other objects but now deleted.
    pub unwrapped_then_deleted: Vec<ObjectRef>,
    /// Object refs of objects now wrapped in other objects.
    pub wrapped: Vec<ObjectRef>,
    /// The updated gas object reference. Have a dedicated field for convenient access.
    /// It's also included in mutated.
    pub gas_object: (ObjectRef, Owner),
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    pub events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    pub dependencies: Vec<TransactionDigest>,
}

impl TransactionEffects {
    /// Creates a TransactionEffects message from the results of execution, choosing the correct
    /// format for the current protocol version.
    pub fn new_from_execution(
        _protocol_version: ProtocolVersion,
        status: ExecutionStatus,
        executed_epoch: EpochId,
        gas_used: GasCostSummary,
        modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
        shared_objects: Vec<ObjectRef>,
        transaction_digest: TransactionDigest,
        created: Vec<(ObjectRef, Owner)>,
        mutated: Vec<(ObjectRef, Owner)>,
        unwrapped: Vec<(ObjectRef, Owner)>,
        deleted: Vec<ObjectRef>,
        unwrapped_then_deleted: Vec<ObjectRef>,
        wrapped: Vec<ObjectRef>,
        gas_object: (ObjectRef, Owner),
        events_digest: Option<TransactionEventsDigest>,
        dependencies: Vec<TransactionDigest>,
    ) -> Self {
        // TODO: when there are multiple versions, use protocol_version to construct the
        // appropriate one.

        Self::V1(TransactionEffectsV1 {
            status,
            executed_epoch,
            gas_used,
            modified_at_versions,
            shared_objects,
            transaction_digest,
            created,
            mutated,
            unwrapped,
            deleted,
            unwrapped_then_deleted,
            wrapped,
            gas_object,
            events_digest,
            dependencies,
        })
    }

    pub fn execution_digests(&self) -> ExecutionDigests {
        ExecutionDigests {
            transaction: *self.transaction_digest(),
            effects: self.digest(),
        }
    }
}

// testing helpers.
impl TransactionEffects {
    pub fn new_with_tx(tx: &Transaction) -> TransactionEffects {
        Self::new_with_tx_and_gas(
            tx,
            (
                random_object_ref(),
                Owner::AddressOwner(tx.data().intent_message.value.sender()),
            ),
        )
    }

    pub fn new_with_tx_and_gas(tx: &Transaction, gas_object: (ObjectRef, Owner)) -> Self {
        TransactionEffects::V1(TransactionEffectsV1 {
            transaction_digest: *tx.digest(),
            gas_object,
            ..Default::default()
        })
    }
}

#[enum_dispatch]
pub trait TransactionEffectsAPI {
    fn status(&self) -> &ExecutionStatus;
    fn into_status(self) -> ExecutionStatus;
    fn executed_epoch(&self) -> EpochId;
    fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)];
    fn shared_objects(&self) -> &[ObjectRef];
    fn created(&self) -> &[(ObjectRef, Owner)];
    fn mutated(&self) -> &[(ObjectRef, Owner)];
    fn unwrapped(&self) -> &[(ObjectRef, Owner)];
    fn deleted(&self) -> &[ObjectRef];
    fn unwrapped_then_deleted(&self) -> &[ObjectRef];
    fn wrapped(&self) -> &[ObjectRef];
    fn gas_object(&self) -> &(ObjectRef, Owner);
    fn events_digest(&self) -> Option<&TransactionEventsDigest>;
    fn dependencies(&self) -> &[TransactionDigest];

    fn all_mutated(&self) -> Vec<(&ObjectRef, &Owner, WriteKind)>;

    fn all_deleted(&self) -> Vec<(&ObjectRef, DeleteKind)>;

    fn transaction_digest(&self) -> &TransactionDigest;

    fn mutated_excluding_gas(&self) -> Vec<&(ObjectRef, Owner)>;

    fn gas_cost_summary(&self) -> &GasCostSummary;

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary;

    // All of these should be #[cfg(test)], but they are used by tests in other crates, and
    // dependencies don't get built with cfg(test) set as far as I can tell.
    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus;
    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary;
    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest;
    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest>;
    fn shared_objects_mut_for_testing(&mut self) -> &mut Vec<ObjectRef>;
    fn modified_at_versions_mut_for_testing(&mut self) -> &mut Vec<(ObjectID, SequenceNumber)>;
}

impl TransactionEffectsAPI for TransactionEffectsV1 {
    fn status(&self) -> &ExecutionStatus {
        &self.status
    }
    fn into_status(self) -> ExecutionStatus {
        self.status
    }
    fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)] {
        &self.modified_at_versions
    }
    fn shared_objects(&self) -> &[ObjectRef] {
        &self.shared_objects
    }
    fn created(&self) -> &[(ObjectRef, Owner)] {
        &self.created
    }
    fn mutated(&self) -> &[(ObjectRef, Owner)] {
        &self.mutated
    }
    fn unwrapped(&self) -> &[(ObjectRef, Owner)] {
        &self.unwrapped
    }
    fn deleted(&self) -> &[ObjectRef] {
        &self.deleted
    }
    fn unwrapped_then_deleted(&self) -> &[ObjectRef] {
        &self.unwrapped_then_deleted
    }
    fn wrapped(&self) -> &[ObjectRef] {
        &self.wrapped
    }
    fn gas_object(&self) -> &(ObjectRef, Owner) {
        &self.gas_object
    }
    fn events_digest(&self) -> Option<&TransactionEventsDigest> {
        self.events_digest.as_ref()
    }
    fn dependencies(&self) -> &[TransactionDigest] {
        &self.dependencies
    }

    fn executed_epoch(&self) -> EpochId {
        self.executed_epoch
    }

    /// Return an iterator that iterates through all mutated objects, including mutated,
    /// created and unwrapped objects. In other words, all objects that still exist
    /// in the object state after this transaction.
    /// It doesn't include deleted/wrapped objects.
    fn all_mutated(&self) -> Vec<(&ObjectRef, &Owner, WriteKind)> {
        self.mutated
            .iter()
            .map(|(r, o)| (r, o, WriteKind::Mutate))
            .chain(self.created.iter().map(|(r, o)| (r, o, WriteKind::Create)))
            .chain(
                self.unwrapped
                    .iter()
                    .map(|(r, o)| (r, o, WriteKind::Unwrap)),
            )
            .collect()
    }

    /// Return an iterator that iterates through all deleted objects, including deleted,
    /// unwrapped_then_deleted, and wrapped objects. In other words, all objects that
    /// do not exist in the object state after this transaction.
    fn all_deleted(&self) -> Vec<(&ObjectRef, DeleteKind)> {
        self.deleted
            .iter()
            .map(|r| (r, DeleteKind::Normal))
            .chain(
                self.unwrapped_then_deleted
                    .iter()
                    .map(|r| (r, DeleteKind::UnwrapThenDelete)),
            )
            .chain(self.wrapped.iter().map(|r| (r, DeleteKind::Wrap)))
            .collect()
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    fn mutated_excluding_gas(&self) -> Vec<&(ObjectRef, Owner)> {
        self.mutated
            .iter()
            .filter(|o| *o != &self.gas_object)
            .collect()
    }

    fn transaction_digest(&self) -> &TransactionDigest {
        &self.transaction_digest
    }

    fn gas_cost_summary(&self) -> &GasCostSummary {
        &self.gas_used
    }

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary {
        TransactionEffectsDebugSummary {
            bcs_size: bcs::serialized_size(self).unwrap(),
            status: self.status.clone(),
            gas_used: self.gas_used.clone(),
            transaction_digest: self.transaction_digest,
            created_object_count: self.created.len(),
            mutated_object_count: self.mutated.len(),
            unwrapped_object_count: self.unwrapped.len(),
            deleted_object_count: self.deleted.len(),
            wrapped_object_count: self.wrapped.len(),
            dependency_count: self.dependencies.len(),
        }
    }

    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus {
        &mut self.status
    }
    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary {
        &mut self.gas_used
    }
    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest {
        &mut self.transaction_digest
    }
    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest> {
        &mut self.dependencies
    }
    fn shared_objects_mut_for_testing(&mut self) -> &mut Vec<ObjectRef> {
        &mut self.shared_objects
    }
    fn modified_at_versions_mut_for_testing(&mut self) -> &mut Vec<(ObjectID, SequenceNumber)> {
        &mut self.modified_at_versions
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub struct TransactionEvents {
    pub data: Vec<Event>,
}

impl TransactionEvents {
    pub fn digest(&self) -> TransactionEventsDigest {
        TransactionEventsDigest::new(sha3_hash(self))
    }
}

impl Message for TransactionEffects {
    type DigestType = TransactionEffectsDigest;
    const SCOPE: IntentScope = IntentScope::TransactionEffects;

    fn digest(&self) -> Self::DigestType {
        TransactionEffectsDigest::new(sha3_hash(self))
    }

    fn verify(&self) -> SuiResult {
        Ok(())
    }
}

impl Display for TransactionEffectsV1 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Status : {:?}", self.status)?;
        if !self.created.is_empty() {
            writeln!(writer, "Created Objects:")?;
            for ((id, _, _), owner) in &self.created {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        if !self.mutated.is_empty() {
            writeln!(writer, "Mutated Objects:")?;
            for ((id, _, _), owner) in &self.mutated {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        if !self.deleted.is_empty() {
            writeln!(writer, "Deleted Objects:")?;
            for (id, _, _) in &self.deleted {
                writeln!(writer, "  - ID: {}", id)?;
            }
        }
        if !self.wrapped.is_empty() {
            writeln!(writer, "Wrapped Objects:")?;
            for (id, _, _) in &self.wrapped {
                writeln!(writer, "  - ID: {}", id)?;
            }
        }
        if !self.unwrapped.is_empty() {
            writeln!(writer, "Unwrapped Objects:")?;
            for ((id, _, _), owner) in &self.unwrapped {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl Default for TransactionEffects {
    fn default() -> Self {
        TransactionEffects::V1(Default::default())
    }
}

impl Default for TransactionEffectsV1 {
    fn default() -> Self {
        TransactionEffectsV1 {
            status: ExecutionStatus::Success,
            executed_epoch: 0,
            gas_used: GasCostSummary {
                computation_cost: 0,
                storage_cost: 0,
                storage_rebate: 0,
            },
            modified_at_versions: Vec::new(),
            shared_objects: Vec::new(),
            transaction_digest: TransactionDigest::random(),
            created: Vec::new(),
            mutated: Vec::new(),
            unwrapped: Vec::new(),
            deleted: Vec::new(),
            unwrapped_then_deleted: Vec::new(),
            wrapped: Vec::new(),
            gas_object: (
                random_object_ref(),
                Owner::AddressOwner(SuiAddress::default()),
            ),
            events_digest: None,
            dependencies: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct TransactionEffectsDebugSummary {
    /// Size of bcs serialized byets of the effects.
    pub bcs_size: usize,
    pub status: ExecutionStatus,
    pub gas_used: GasCostSummary,
    pub transaction_digest: TransactionDigest,
    pub created_object_count: usize,
    pub mutated_object_count: usize,
    pub unwrapped_object_count: usize,
    pub deleted_object_count: usize,
    pub wrapped_object_count: usize,
    pub dependency_count: usize,
    // TODO: Add deleted_and_unwrapped_object_count and event digest.
}

pub type TransactionEffectsEnvelope<S> = Envelope<TransactionEffects, S>;
pub type UnsignedTransactionEffects = TransactionEffectsEnvelope<EmptySignInfo>;
pub type SignedTransactionEffects = TransactionEffectsEnvelope<AuthoritySignInfo>;
pub type CertifiedTransactionEffects = TransactionEffectsEnvelope<AuthorityStrongQuorumSignInfo>;

pub type TrustedSignedTransactionEffects = TrustedEnvelope<TransactionEffects, AuthoritySignInfo>;
pub type VerifiedTransactionEffectsEnvelope<S> = VerifiedEnvelope<TransactionEffects, S>;
pub type VerifiedSignedTransactionEffects = VerifiedTransactionEffectsEnvelope<AuthoritySignInfo>;
pub type VerifiedCertifiedTransactionEffects =
    VerifiedTransactionEffectsEnvelope<AuthorityStrongQuorumSignInfo>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum InputObjectKind {
    // A Move package, must be immutable.
    MovePackage(ObjectID),
    // A Move object, either immutable, or owned mutable.
    ImmOrOwnedMoveObject(ObjectRef),
    // A Move object that's shared and mutable.
    SharedMoveObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        mutable: bool,
    },
}

impl InputObjectKind {
    pub fn object_id(&self) -> ObjectID {
        match self {
            Self::MovePackage(id) => *id,
            Self::ImmOrOwnedMoveObject((id, _, _)) => *id,
            Self::SharedMoveObject { id, .. } => *id,
        }
    }

    pub fn version(&self) -> Option<SequenceNumber> {
        match self {
            Self::MovePackage(..) => None,
            Self::ImmOrOwnedMoveObject((_, version, _)) => Some(*version),
            Self::SharedMoveObject { .. } => None,
        }
    }

    pub fn object_not_found_error(&self) -> UserInputError {
        match *self {
            Self::MovePackage(package_id) => {
                UserInputError::DependentPackageNotFound { package_id }
            }
            Self::ImmOrOwnedMoveObject((object_id, version, _)) => UserInputError::ObjectNotFound {
                object_id,
                version: Some(version),
            },
            Self::SharedMoveObject { id, .. } => UserInputError::ObjectNotFound {
                object_id: id,
                version: None,
            },
        }
    }
}

pub struct InputObjects {
    objects: Vec<(InputObjectKind, Object)>,
}

impl InputObjects {
    pub fn new(objects: Vec<(InputObjectKind, Object)>) -> Self {
        Self { objects }
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn filter_owned_objects(&self) -> Vec<ObjectRef> {
        let owned_objects: Vec<_> = self
            .objects
            .iter()
            .filter_map(|(object_kind, object)| match object_kind {
                InputObjectKind::MovePackage(_) => None,
                InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                    if object.is_immutable() {
                        None
                    } else {
                        Some(*object_ref)
                    }
                }
                InputObjectKind::SharedMoveObject { .. } => None,
            })
            .collect();

        debug!(
            num_mutable_objects = owned_objects.len(),
            "Checked locks and found mutable objects"
        );

        owned_objects
    }

    pub fn filter_shared_objects(&self) -> Vec<ObjectRef> {
        self.objects
            .iter()
            .filter(|(kind, _)| matches!(kind, InputObjectKind::SharedMoveObject { .. }))
            .map(|(_, obj)| obj.compute_object_reference())
            .collect()
    }

    pub fn transaction_dependencies(&self) -> BTreeSet<TransactionDigest> {
        self.objects
            .iter()
            .map(|(_, obj)| obj.previous_transaction)
            .collect()
    }

    pub fn mutable_inputs(&self) -> Vec<ObjectRef> {
        self.objects
            .iter()
            .filter_map(|(kind, object)| match kind {
                InputObjectKind::MovePackage(_) => None,
                InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                    if object.is_immutable() {
                        None
                    } else {
                        Some(*object_ref)
                    }
                }
                InputObjectKind::SharedMoveObject { mutable, .. } => {
                    if *mutable {
                        Some(object.compute_object_reference())
                    } else {
                        None
                    }
                }
            })
            .collect()
    }

    /// The version to set on objects created by the computation that `self` is input to.
    /// Guaranteed to be strictly greater than the versions of all input objects.
    pub fn lamport_timestamp(&self) -> SequenceNumber {
        let input_versions = self
            .objects
            .iter()
            .filter_map(|(_, object)| object.data.try_as_move().map(MoveObject::version));

        SequenceNumber::lamport_increment(input_versions)
    }

    pub fn into_object_map(self) -> BTreeMap<ObjectID, Object> {
        self.objects
            .into_iter()
            .map(|(_, object)| (object.id(), object))
            .collect()
    }
}

impl Display for CertifiedTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Transaction Hash: {:?}", self.digest())?;
        writeln!(
            writer,
            "Signed Authorities Bitmap : {:?}",
            self.auth_sig().signers_map
        )?;
        write!(writer, "{}", &self.data().intent_message.value.kind())?;
        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConsensusSync {
    pub sequence_number: SequenceNumber,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsensusTransaction {
    /// Encodes an u64 unique tracking id to allow us trace a message between Sui and Narwhal.
    /// Use an byte array instead of u64 to ensure stable serialization.
    pub tracking_id: [u8; 8],
    pub kind: ConsensusTransactionKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ConsensusTransactionKey {
    Certificate(TransactionDigest),
    CheckpointSignature(AuthorityName, CheckpointSequenceNumber),
    EndOfPublish(AuthorityName),
    CapabilityNotification(AuthorityName, u64 /* generation */),
}

impl Debug for ConsensusTransactionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(digest) => write!(f, "Certificate({:?})", digest),
            Self::CheckpointSignature(name, seq) => {
                write!(f, "CheckpointSignature({:?}, {:?})", name.concise(), seq)
            }
            Self::EndOfPublish(name) => write!(f, "EndOfPublish({:?})", name.concise()),
            Self::CapabilityNotification(name, generation) => write!(
                f,
                "CapabilityNotification({:?}, {:?})",
                name.concise(),
                generation
            ),
        }
    }
}

/// Used to advertise capabilities of each authority via narwhal. This allows validators to
/// negotiate the creation of the ChangeEpoch transaction.
#[derive(Serialize, Deserialize, Clone, Hash)]
pub struct AuthorityCapabilities {
    /// Originating authority - must match narwhal transaction source.
    pub authority: AuthorityName,
    /// Generation number set by sending authority. Used to determine which of multiple
    /// AuthorityCapabilities messages from the same authority is the most recent.
    ///
    /// (Currently, we just set this to the current time in milliseconds since the epoch, but this
    /// should not be interpreted as a timestamp.)
    pub generation: u64,

    /// ProtocolVersions that the authority supports.
    pub supported_protocol_versions: SupportedProtocolVersions,

    /// The ObjectRefs of all versions of system packages that the validator possesses.
    /// Used to determine whether to do a framework/movestdlib upgrade.
    pub available_system_packages: Vec<ObjectRef>,
}

impl Debug for AuthorityCapabilities {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorityCapabilities")
            .field("authority", &self.authority.concise())
            .field("generation", &self.generation)
            .field(
                "supported_protocol_versions",
                &self.supported_protocol_versions,
            )
            .field("available_system_packages", &self.available_system_packages)
            .finish()
    }
}

impl AuthorityCapabilities {
    pub fn new(
        authority: AuthorityName,
        supported_protocol_versions: SupportedProtocolVersions,
        available_system_packages: Vec<ObjectRef>,
    ) -> Self {
        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Sui did not exist prior to 1970")
            .as_millis()
            .try_into()
            .expect("This build of sui is not supported in the year 500,000,000");
        Self {
            authority,
            generation,
            supported_protocol_versions,
            available_system_packages,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ConsensusTransactionKind {
    UserTransaction(Box<CertifiedTransaction>),
    CheckpointSignature(Box<CheckpointSignatureMessage>),
    EndOfPublish(AuthorityName),
    CapabilityNotification(AuthorityCapabilities),
}

impl ConsensusTransaction {
    pub fn new_certificate_message(
        authority: &AuthorityName,
        certificate: CertifiedTransaction,
    ) -> Self {
        let mut hasher = DefaultHasher::new();
        let tx_digest = certificate.digest();
        tx_digest.hash(&mut hasher);
        authority.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::UserTransaction(Box::new(certificate)),
        }
    }

    pub fn new_checkpoint_signature_message(data: CheckpointSignatureMessage) -> Self {
        let mut hasher = DefaultHasher::new();
        data.summary.auth_signature.signature.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CheckpointSignature(Box::new(data)),
        }
    }

    pub fn new_end_of_publish(authority: AuthorityName) -> Self {
        let mut hasher = DefaultHasher::new();
        authority.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::EndOfPublish(authority),
        }
    }

    pub fn new_capability_notification(capabilities: AuthorityCapabilities) -> Self {
        let mut hasher = DefaultHasher::new();
        capabilities.hash(&mut hasher);
        let tracking_id = hasher.finish().to_le_bytes();
        Self {
            tracking_id,
            kind: ConsensusTransactionKind::CapabilityNotification(capabilities),
        }
    }

    pub fn get_tracking_id(&self) -> u64 {
        (&self.tracking_id[..])
            .read_u64::<BigEndian>()
            .unwrap_or_default()
    }

    pub fn verify(&self, committee: &Committee) -> SuiResult<()> {
        match &self.kind {
            ConsensusTransactionKind::UserTransaction(certificate) => {
                certificate.verify_signature(committee)
            }
            ConsensusTransactionKind::CheckpointSignature(data) => data.verify(committee),
            // EndOfPublish and CapabilityNotification are authenticated in
            // AuthorityPerEpochStore::verify_consensus_transaction
            ConsensusTransactionKind::EndOfPublish(_)
            | ConsensusTransactionKind::CapabilityNotification(_) => Ok(()),
        }
    }

    pub fn key(&self) -> ConsensusTransactionKey {
        match &self.kind {
            ConsensusTransactionKind::UserTransaction(cert) => {
                ConsensusTransactionKey::Certificate(*cert.digest())
            }
            ConsensusTransactionKind::CheckpointSignature(data) => {
                ConsensusTransactionKey::CheckpointSignature(
                    data.summary.auth_signature.authority,
                    data.summary.summary.sequence_number,
                )
            }
            ConsensusTransactionKind::EndOfPublish(authority) => {
                ConsensusTransactionKey::EndOfPublish(*authority)
            }
            ConsensusTransactionKind::CapabilityNotification(cap) => {
                ConsensusTransactionKey::CapabilityNotification(cap.authority, cap.generation)
            }
        }
    }

    pub fn is_user_certificate(&self) -> bool {
        matches!(self.kind, ConsensusTransactionKind::UserTransaction(_))
    }

    pub fn is_end_of_publish(&self) -> bool {
        matches!(self.kind, ConsensusTransactionKind::EndOfPublish(_))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, schemars::JsonSchema)]
pub enum ExecuteTransactionRequestType {
    WaitForEffectsCert,
    WaitForLocalExecution,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecuteTransactionRequest {
    pub transaction: Transaction,
    pub request_type: ExecuteTransactionRequestType,
}

#[derive(Debug)]
pub enum TransactionType {
    SingleWriter, // Txes that only use owned objects and/or immutable objects
    SharedObject, // Txes that use at least one shared object
}

impl ExecuteTransactionRequest {
    pub fn transaction_type(&self) -> TransactionType {
        if self.transaction.contains_shared_object() {
            TransactionType::SharedObject
        } else {
            TransactionType::SingleWriter
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum EffectsFinalityInfo {
    Certified(AuthorityStrongQuorumSignInfo),
    Checkpointed(EpochId, CheckpointSequenceNumber),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FinalizedEffects {
    pub effects: TransactionEffects,
    pub finality_info: EffectsFinalityInfo,
}

impl FinalizedEffects {
    pub fn new_from_effects_cert(effects_cert: CertifiedTransactionEffects) -> Self {
        let (data, sig) = effects_cert.into_data_and_sig();
        Self {
            effects: data,
            finality_info: EffectsFinalityInfo::Certified(sig),
        }
    }

    pub fn epoch(&self) -> EpochId {
        match &self.finality_info {
            EffectsFinalityInfo::Certified(cert) => cert.epoch,
            EffectsFinalityInfo::Checkpointed(epoch, _) => *epoch,
        }
    }
}

/// When requested to execute a transaction with WaitForLocalExecution,
/// TransactionOrchestrator attempts to execute this transaction locally
/// after it is finalized. This value represents whether the transaction
/// is confirmed to be executed on this node before the response returns.
pub type IsTransactionExecutedLocally = bool;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ExecuteTransactionResponse {
    EffectsCert(
        Box<(
            FinalizedEffects,
            TransactionEvents,
            IsTransactionExecutedLocally,
        )>,
    ),
}

#[derive(Clone, Debug)]
pub struct QuorumDriverRequest {
    pub transaction: VerifiedTransaction,
}

#[derive(Debug, Clone)]
pub struct QuorumDriverResponse {
    pub effects_cert: VerifiedCertifiedTransactionEffects,
    pub events: TransactionEvents,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SystemStateRequest {
    // This is needed to make gRPC happy.
    pub _unused: bool,
}

// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{base_types::*, error::*};
use crate::committee::{EpochId, ProtocolVersion};
use crate::crypto::{
    default_hash, AuthoritySignInfo, AuthoritySignature, AuthorityStrongQuorumSignInfo,
    DefaultHash, Ed25519SuiSignature, EmptySignInfo, Signature, Signer, SuiSignatureInner,
    ToFromBytes,
};
use crate::digests::{CertificateDigest, SenderSignedDataDigest};
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages_checkpoint::CheckpointTimestamp;
use crate::messages_consensus::ConsensusCommitPrologue;
use crate::object::{MoveObject, Object, Owner};
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::signature::{AuthenticatorTrait, GenericSignature};
use crate::{
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_FRAMEWORK_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use enum_dispatch::enum_dispatch;
use fastcrypto::{encoding::Base64, hash::HashFunction};
use itertools::Either;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use serde::{Deserialize, Serialize};
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::fmt::Write;
use std::fmt::{Debug, Display, Formatter};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    hash::Hash,
    iter,
};
use strum::IntoStaticStr;
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use tap::Pipe;
use tracing::trace;

pub const TEST_ONLY_GAS_UNIT_FOR_TRANSFER: u64 = 2_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS: u64 = 10_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_PUBLISH: u64 = 25_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_STAKING: u64 = 10_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_GENERIC: u64 = 5_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_VALIDATOR: u64 = 25_000_000;
pub const TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN: u64 = 1_000_000;

pub const GAS_PRICE_FOR_SYSTEM_TX: u64 = 1;

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
pub struct ChangeEpoch {
    /// The next (to become) epoch ID.
    pub epoch: EpochId,
    /// The protocol version in effect in the new epoch.
    pub protocol_version: ProtocolVersion,
    /// The total amount of gas charged for storage during the epoch.
    pub storage_charge: u64,
    /// The total amount of gas charged for computation during the epoch.
    pub computation_charge: u64,
    /// The amount of storage rebate refunded to the txn senders.
    pub storage_rebate: u64,
    /// The non-refundable storage fee.
    pub non_refundable_storage_fee: u64,
    /// Unix timestamp when epoch started
    pub epoch_start_timestamp_ms: u64,
    /// System packages (specifically framework and move stdlib) that are written before the new
    /// epoch starts. This tracks framework upgrades on chain. When executing the ChangeEpoch txn,
    /// the validator must write out the modules below.  Modules are provided with the version they
    /// will be upgraded to, their modules in serialized form (which include their package ID), and
    /// a list of their transitive dependencies.
    pub system_packages: Vec<(SequenceNumber, Vec<Vec<u8>>, Vec<ObjectID>)>,
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

impl GenesisObject {
    pub fn id(&self) -> ObjectID {
        match self {
            GenesisObject::RawObject { data, .. } => data.id(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, IntoStaticStr)]
pub enum TransactionKind {
    /// A transaction that allows the interleaving of native commands and Move calls
    ProgrammableTransaction(ProgrammableTransaction),
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
    // .. more transaction types go here
}

impl VersionedProtocolMessage for TransactionKind {
    fn check_version_supported(&self, _protocol_config: &ProtocolConfig) -> SuiResult {
        // This code does nothing right now - it exists to cause a compiler error when new
        // enumerants are added to TransactionKind.
        //
        // When we add new cases here, check that current_protocol_version does not pre-date the
        // addition of that enumerant.
        match &self {
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_)
            | TransactionKind::ProgrammableTransaction(_) => Ok(()),
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
        }
        Ok(())
    }
}

impl From<bool> for CallArg {
    fn from(b: bool) -> Self {
        // unwrap safe because every u8 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&b).unwrap())
    }
}

impl From<u8> for CallArg {
    fn from(n: u8) -> Self {
        // unwrap safe because every u8 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u16> for CallArg {
    fn from(n: u16) -> Self {
        // unwrap safe because every u16 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u32> for CallArg {
    fn from(n: u32) -> Self {
        // unwrap safe because every u32 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u64> for CallArg {
    fn from(n: u64) -> Self {
        // unwrap safe because every u64 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<u128> for CallArg {
    fn from(n: u128) -> Self {
        // unwrap safe because every u128 value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(&n).unwrap())
    }
}

impl From<&Vec<u8>> for CallArg {
    fn from(v: &Vec<u8>) -> Self {
        // unwrap safe because every vec<u8> value is BCS-serializable
        CallArg::Pure(bcs::to_bytes(v).unwrap())
    }
}

impl From<ObjectRef> for CallArg {
    fn from(obj: ObjectRef) -> Self {
        CallArg::Object(ObjectArg::ImmOrOwnedObject(obj))
    }
}

impl ObjectArg {
    pub fn id(&self) -> ObjectID {
        match self {
            ObjectArg::ImmOrOwnedObject((id, _, _)) | ObjectArg::SharedObject { id, .. } => *id,
        }
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
    /// `(&mut Coin<T>, Vec<u64>)` -> `Vec<Coin<T>>`
    /// It splits off some amounts into a new coins with those amounts
    SplitCoins(Argument, Vec<Argument>),
    /// `(&mut Coin<T>, Vec<Coin<T>>)`
    /// It merges n-coins into the first coin
    MergeCoins(Argument, Vec<Argument>),
    /// Publishes a Move package. It takes the package bytes and a list of the package's transitive
    /// dependencies to link against on-chain.
    Publish(Vec<Vec<u8>>, Vec<ObjectID>),
    /// `forall T: Vec<T> -> vector<T>`
    /// Given n-values of the same type, it constructs a vector. For non objects or an empty vector,
    /// the type tag must be specified.
    MakeMoveVec(Option<TypeTag>, Vec<Argument>),
    /// Upgrades a Move package
    /// Takes (in order):
    /// 1. A vector of serialized modules for the package.
    /// 2. A vector of object ids for the transitive dependencies of the new package.
    /// 3. The object ID of the package being upgraded.
    /// 4. An argument holding the `UpgradeTicket` that must have been produced from an earlier command in the same
    ///    programmable transaction.
    Upgrade(Vec<Vec<u8>>, Vec<ObjectID>, ObjectID, Argument),
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
        Ok(())
    }
}

impl Command {
    pub fn move_call(
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<Argument>,
    ) -> Self {
        Command::MoveCall(Box::new(ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        }))
    }

    fn input_objects(&self) -> Vec<InputObjectKind> {
        match self {
            Command::Upgrade(_, deps, package_id, _) => deps
                .iter()
                .map(|id| InputObjectKind::MovePackage(*id))
                .chain(Some(InputObjectKind::MovePackage(*package_id)))
                .collect(),
            Command::Publish(_, deps) => deps
                .iter()
                .map(|id| InputObjectKind::MovePackage(*id))
                .collect(),
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
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _) => vec![],
        }
    }

    fn non_system_packages_to_be_published(&self) -> Option<&Vec<Vec<u8>>> {
        match self {
            Command::Upgrade(v, _, _, _) => Some(v),
            Command::Publish(v, _) => Some(v),
            Command::MoveCall(_)
            | Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _)
            | Command::MakeMoveVec(_, _) => None,
        }
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        match self {
            Command::MoveCall(call) => call.validity_check(config)?,
            Command::TransferObjects(args, _)
            | Command::MergeCoins(_, args)
            | Command::SplitCoins(_, args) => {
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
                fp_ensure!(
                    args.len() < config.max_arguments() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum arguments in a programmable transaction command"
                            .to_string(),
                        value: config.max_arguments().to_string()
                    }
                );
            }
            Command::Publish(modules, _dep_ids) => {
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
            Command::Upgrade(modules, _, _, _) => {
                fp_ensure!(!modules.is_empty(), UserInputError::EmptyCommandInput);
                fp_ensure!(
                    modules.len() < config.max_modules_in_publish() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum modules in a programmable transaction upgrade command"
                            .to_string(),
                        value: config.max_modules_in_publish().to_string()
                    }
                );
            }
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
        let ProgrammableTransaction { inputs, commands } = self;
        fp_ensure!(
            commands.len() < config.max_programmable_tx_commands() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum commands in a programmable transaction".to_string(),
                value: config.max_programmable_tx_commands().to_string()
            }
        );
        for input in inputs {
            input.validity_check(config)?
        }
        for command in commands {
            command.validity_check(config)?
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
            })
            .flatten()
    }

    fn move_calls(&self) -> Vec<(&ObjectID, &IdentStr, &IdentStr)> {
        self.commands
            .iter()
            .filter_map(|command| match command {
                Command::MoveCall(m) => Some((
                    &m.package,
                    m.module.as_ident_str(),
                    m.function.as_ident_str(),
                )),
                _ => None,
            })
            .collect()
    }

    pub fn non_system_packages_to_be_published(&self) -> impl Iterator<Item = &Vec<Vec<u8>>> + '_ {
        self.commands
            .iter()
            .filter_map(|q| q.non_system_packages_to_be_published())
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
            Command::SplitCoins(coin, amounts) => {
                write!(f, "SplitCoins({coin}")?;
                write_sep(f, amounts, ",")?;
                write!(f, ")")
            }
            Command::MergeCoins(target, coins) => {
                write!(f, "MergeCoins({target},")?;
                write_sep(f, coins, ",")?;
                write!(f, ")")
            }
            Command::Publish(_bytes, deps) => {
                write!(f, "Publish(_,")?;
                write_sep(f, deps, ",")?;
                write!(f, ")")
            }
            Command::Upgrade(_bytes, deps, current_package_id, ticket) => {
                write!(f, "Upgrade(_,")?;
                write_sep(f, deps, ",")?;
                write!(f, ", {current_package_id}")?;
                write!(f, ", {ticket}")?;
                write!(f, ")")
            }
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

impl TransactionKind {
    /// present to make migrations to programmable transactions eaier.
    /// Will be removed
    pub fn programmable(pt: ProgrammableTransaction) -> Self {
        TransactionKind::ProgrammableTransaction(pt)
    }

    pub fn is_system_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::ChangeEpoch(_)
                | TransactionKind::Genesis(_)
                | TransactionKind::ConsensusCommitPrologue(_)
        )
    }

    /// If this is advance epoch transaction, returns (total gas charged, total gas rebated).
    /// TODO: We should use GasCostSummary directly in ChangeEpoch struct, and return that
    /// directly.
    pub fn get_advance_epoch_tx_gas_summary(&self) -> Option<(u64, u64)> {
        match self {
            Self::ChangeEpoch(e) => {
                Some((e.computation_charge + e.storage_charge, e.storage_rebate))
            }
            _ => None,
        }
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    /// Returns an iterator of all shared input objects used by this transaction.
    /// It covers both Call and ChangeEpoch transaction kind, because both makes Move calls.
    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match &self {
            Self::ChangeEpoch(_) => Either::Left(Either::Left(iter::once(SharedInputObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            }))),

            Self::ConsensusCommitPrologue(_) => {
                Either::Left(Either::Right(iter::once(SharedInputObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutable: true,
                })))
            }
            Self::ProgrammableTransaction(pt) => {
                Either::Right(Either::Left(pt.shared_input_objects()))
            }
            _ => Either::Right(Either::Right(iter::empty())),
        }
    }

    fn move_calls(&self) -> Vec<(&ObjectID, &IdentStr, &IdentStr)> {
        match &self {
            Self::ProgrammableTransaction(pt) => pt.move_calls(),
            _ => vec![],
        }
    }

    /// Return the metadata of each of the input objects for the transaction.
    /// For a Move object, we attach the object reference;
    /// for a Move package, we provide the object id only since they never change on chain.
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let input_objects = match &self {
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

    pub fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        match self {
            TransactionKind::ProgrammableTransaction(p) => p.validity_check(config)?,
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_) => (),
        };
        Ok(())
    }

    /// number of commands, or 0 if it is a system transaction
    pub fn num_commands(&self) -> usize {
        match self {
            TransactionKind::ProgrammableTransaction(pt) => pt.commands.len(),
            _ => 0,
        }
    }

    pub fn iter_commands(&self) -> impl Iterator<Item = &Command> {
        match self {
            TransactionKind::ProgrammableTransaction(pt) => pt.commands.iter(),
            _ => [].iter(),
        }
    }
}

impl Display for TransactionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match &self {
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

    fn check_version_supported(&self, protocol_config: &ProtocolConfig) -> SuiResult {
        // First check the gross version
        let (message_version, supported) = match self {
            Self::V1(_) => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Suppose we add V2 at protocol version 7, then we must change this to:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Self::V2 => (2, SupportedProtocolVersions::new_for_message(7, u64::MAX)),
            //
            // Suppose we remove support for V1 after protocol version 12: we can do it like so:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, 12)),
        };

        if !supported.is_version_supported(protocol_config.version) {
            return Err(SuiError::WrongMessageVersion {
                error: format!(
                    "TransactionDataV{} is not supported at {:?}. (Supported range is {:?}",
                    message_version, protocol_config.version, supported
                ),
            });
        }

        // Now check interior versioned data
        self.kind().check_version_supported(protocol_config)?;

        Ok(())
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
    fn new_system_transaction(kind: TransactionKind) -> Self {
        // assert transaction kind if a system transaction
        assert!(kind.is_system_tx());
        let sender = SuiAddress::default();
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: GAS_PRICE_FOR_SYSTEM_TX,
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
        Self::new_with_gas_coins_allow_sponsor(
            kind,
            sender,
            gas_payment,
            gas_budget,
            gas_price,
            sender,
        )
    }

    pub fn new_with_gas_coins_allow_sponsor(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: Vec<ObjectRef>,
        gas_budget: u64,
        gas_price: u64,
        gas_sponsor: SuiAddress,
    ) -> Self {
        TransactionData::V1(TransactionDataV1 {
            kind,
            sender,
            gas_data: GasData {
                price: gas_price,
                owner: gas_sponsor,
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
    ) -> anyhow::Result<Self> {
        Self::new_move_call_with_gas_coins(
            sender,
            package,
            module,
            function,
            type_arguments,
            vec![gas_payment],
            arguments,
            gas_budget,
            gas_price,
        )
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
    ) -> anyhow::Result<Self> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.move_call(package, module, function, type_arguments, arguments)?;
            builder.finish()
        };
        Ok(Self::new_programmable(
            sender,
            gas_payment,
            pt,
            gas_budget,
            gas_price,
        ))
    }

    pub fn new_transfer(
        recipient: SuiAddress,
        object_ref: ObjectRef,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_object(recipient, object_ref).unwrap();
            builder.finish()
        };
        Self::new_programmable(sender, vec![gas_payment], pt, gas_budget, gas_price)
    }

    pub fn new_transfer_sui(
        recipient: SuiAddress,
        sender: SuiAddress,
        amount: Option<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        Self::new_transfer_sui_allow_sponsor(
            recipient,
            sender,
            amount,
            gas_payment,
            gas_budget,
            gas_price,
            sender,
        )
    }

    pub fn new_transfer_sui_allow_sponsor(
        recipient: SuiAddress,
        sender: SuiAddress,
        amount: Option<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
        gas_sponsor: SuiAddress,
    ) -> Self {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient, amount);
            builder.finish()
        };
        Self::new_programmable_allow_sponsor(
            sender,
            vec![gas_payment],
            pt,
            gas_budget,
            gas_price,
            gas_sponsor,
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
    ) -> anyhow::Result<Self> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.pay(coins, recipients, amounts)?;
            builder.finish()
        };
        Ok(Self::new_programmable(
            sender,
            vec![gas_payment],
            pt,
            gas_budget,
            gas_price,
        ))
    }

    pub fn new_pay_sui(
        sender: SuiAddress,
        mut coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> anyhow::Result<Self> {
        coins.insert(0, gas_payment);
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.pay_sui(recipients, amounts)?;
            builder.finish()
        };
        Ok(Self::new_programmable(
            sender, coins, pt, gas_budget, gas_price,
        ))
    }

    pub fn new_pay_all_sui(
        sender: SuiAddress,
        mut coins: Vec<ObjectRef>,
        recipient: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        coins.insert(0, gas_payment);
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.pay_all_sui(recipient);
            builder.finish()
        };
        Self::new_programmable(sender, coins, pt, gas_budget, gas_price)
    }

    pub fn new_module(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let upgrade_cap = builder.publish_upgradeable(modules, dep_ids);
            builder.transfer_arg(sender, upgrade_cap);
            builder.finish()
        };
        Self::new_programmable(sender, vec![gas_payment], pt, gas_budget, gas_price)
    }

    pub fn new_upgrade(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        package_id: ObjectID,
        modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
        (upgrade_capability, capability_owner): (ObjectRef, Owner),
        upgrade_policy: u8,
        digest: Vec<u8>,
        gas_budget: u64,
        gas_price: u64,
    ) -> anyhow::Result<Self> {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let capability_arg = match capability_owner {
                Owner::AddressOwner(_) => ObjectArg::ImmOrOwnedObject(upgrade_capability),
                Owner::Shared {
                    initial_shared_version,
                } => ObjectArg::SharedObject {
                    id: upgrade_capability.0,
                    initial_shared_version,
                    mutable: true,
                },
                Owner::Immutable => {
                    return Err(anyhow::anyhow!(
                        "Upgrade capability is stored immutably and cannot be used for upgrades"
                    ))
                }
                // If the capability is owned by an object, then the module defining the owning
                // object gets to decide how the upgrade capability should be used.
                Owner::ObjectOwner(_) => {
                    return Err(anyhow::anyhow!("Upgrade capability controlled by object"))
                }
            };
            builder.obj(capability_arg).unwrap();
            let upgrade_arg = builder.pure(upgrade_policy).unwrap();
            let digest_arg = builder.pure(digest).unwrap();
            let upgrade_ticket = builder.programmable_move_call(
                SUI_FRAMEWORK_OBJECT_ID,
                ident_str!("package").to_owned(),
                ident_str!("authorize_upgrade").to_owned(),
                vec![],
                vec![Argument::Input(0), upgrade_arg, digest_arg],
            );
            let upgrade_receipt = builder.upgrade(package_id, upgrade_ticket, dep_ids, modules);

            builder.programmable_move_call(
                SUI_FRAMEWORK_OBJECT_ID,
                ident_str!("package").to_owned(),
                ident_str!("commit_upgrade").to_owned(),
                vec![],
                vec![Argument::Input(0), upgrade_receipt],
            );

            builder.finish()
        };
        Ok(Self::new_programmable(
            sender,
            vec![gas_payment],
            pt,
            gas_budget,
            gas_price,
        ))
    }

    pub fn new_programmable(
        sender: SuiAddress,
        gas_payment: Vec<ObjectRef>,
        pt: ProgrammableTransaction,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        Self::new_programmable_allow_sponsor(sender, gas_payment, pt, gas_budget, gas_price, sender)
    }

    pub fn new_programmable_allow_sponsor(
        sender: SuiAddress,
        gas_payment: Vec<ObjectRef>,
        pt: ProgrammableTransaction,
        gas_budget: u64,
        gas_price: u64,
        sponsor: SuiAddress,
    ) -> Self {
        let kind = TransactionKind::ProgrammableTransaction(pt);
        Self::new_with_gas_coins_allow_sponsor(
            kind,
            sender,
            gas_payment,
            gas_budget,
            gas_price,
            sponsor,
        )
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

    // Note: this implies that SingleTransactionKind itself must be versioned, so that it can be
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

    fn move_calls(&self) -> Vec<(&ObjectID, &IdentStr, &IdentStr)>;

    fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>>;

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult;

    fn validity_check_no_gas_check(&self, config: &ProtocolConfig) -> UserInputResult;

    /// Check if the transaction is compliant with sponsorship.
    fn check_sponsorship(&self) -> UserInputResult;

    fn is_system_tx(&self) -> bool;
    fn is_change_epoch_tx(&self) -> bool;
    fn is_genesis_tx(&self) -> bool;

    /// Check if the transaction is sponsored (namely gas owner != sender)
    fn is_sponsored_tx(&self) -> bool;

    #[cfg(test)]
    fn sender_mut(&mut self) -> &mut SuiAddress;

    #[cfg(test)]
    fn gas_data_mut(&mut self) -> &mut GasData;

    // This should be used in testing only.
    fn expiration_mut_for_testing(&mut self) -> &mut TransactionExpiration;
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

    fn move_calls(&self) -> Vec<(&ObjectID, &IdentStr, &IdentStr)> {
        self.kind.move_calls()
    }

    fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>> {
        let mut inputs = self.kind.input_objects()?;

        if !self.kind.is_system_tx() {
            inputs.extend(
                self.gas()
                    .iter()
                    .map(|obj_ref| InputObjectKind::ImmOrOwnedMoveObject(*obj_ref)),
            );
        }
        Ok(inputs)
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        fp_ensure!(!self.gas().is_empty(), UserInputError::MissingGasPayment);
        fp_ensure!(
            self.gas().len() < config.max_gas_payment_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum number of gas payment objects".to_string(),
                value: config.max_gas_payment_objects().to_string()
            }
        );
        self.validity_check_no_gas_check(config)
    }

    // Keep all the logic for validity here, we need this for dry run where the gas
    // may not be provided and created "on the fly"
    fn validity_check_no_gas_check(&self, config: &ProtocolConfig) -> UserInputResult {
        self.kind().validity_check(config)?;
        self.check_sponsorship()
    }

    /// Check if the transaction is sponsored (namely gas owner != sender)
    fn is_sponsored_tx(&self) -> bool {
        self.gas_owner() != self.sender
    }

    /// Check if the transaction is compliant with sponsorship.
    fn check_sponsorship(&self) -> UserInputResult {
        // Not a sponsored transaction, nothing to check
        if self.gas_owner() == self.sender() {
            return Ok(());
        }
        let allow_sponsored_tx = match &self.kind {
            TransactionKind::ProgrammableTransaction(_) => true,
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::ConsensusCommitPrologue(_)
            | TransactionKind::Genesis(_) => false,
        };
        if allow_sponsored_tx {
            return Ok(());
        }
        Err(UserInputError::UnsupportedSponsoredTransactionKind)
    }

    fn is_change_epoch_tx(&self) -> bool {
        matches!(self.kind, TransactionKind::ChangeEpoch(_))
    }

    fn is_system_tx(&self) -> bool {
        self.kind.is_system_tx()
    }

    fn is_genesis_tx(&self) -> bool {
        matches!(self.kind, TransactionKind::Genesis(_))
    }

    #[cfg(test)]
    fn sender_mut(&mut self) -> &mut SuiAddress {
        &mut self.sender
    }

    #[cfg(test)]
    fn gas_data_mut(&mut self) -> &mut GasData {
        &mut self.gas_data
    }

    fn expiration_mut_for_testing(&mut self) -> &mut TransactionExpiration {
        &mut self.expiration
    }
}

impl TransactionDataV1 {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SenderSignedData(Vec<SenderSignedTransaction>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SenderSignedTransaction {
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
        Self(vec![SenderSignedTransaction {
            intent_message: IntentMessage::new(intent, tx_data),
            tx_signatures,
        }])
    }

    pub fn new_from_sender_signature(
        tx_data: TransactionData,
        intent: Intent,
        tx_signature: Signature,
    ) -> Self {
        Self(vec![SenderSignedTransaction {
            intent_message: IntentMessage::new(intent, tx_data),
            tx_signatures: vec![tx_signature.into()],
        }])
    }

    pub fn inner(&self) -> &SenderSignedTransaction {
        // assert is safe - SenderSignedTransaction::verify ensures length is 1.
        assert_eq!(self.0.len(), 1);
        self.0
            .get(0)
            .expect("SenderSignedData must contain exactly one transaction")
    }

    pub fn inner_mut(&mut self) -> &mut SenderSignedTransaction {
        // assert is safe - SenderSignedTransaction::verify ensures length is 1.
        assert_eq!(self.0.len(), 1);
        self.0
            .get_mut(0)
            .expect("SenderSignedData must contain exactly one transaction")
    }

    // This function does not check validity of the signature
    // or perform any de-dup checks.
    pub fn add_signature(&mut self, new_signature: Signature) {
        self.inner_mut().tx_signatures.push(new_signature.into());
    }

    fn get_signer_sig_mapping(&self) -> SuiResult<BTreeMap<SuiAddress, &GenericSignature>> {
        let mut mapping = BTreeMap::new();
        for sig in &self.inner().tx_signatures {
            let address = sig.try_into()?;
            mapping.insert(address, sig);
        }
        Ok(mapping)
    }

    pub fn transaction_data(&self) -> &TransactionData {
        &self.intent_message().value
    }

    pub fn intent_message(&self) -> &IntentMessage<TransactionData> {
        &self.inner().intent_message
    }

    pub fn tx_signatures(&self) -> &[GenericSignature] {
        &self.inner().tx_signatures
    }

    #[cfg(test)]
    pub fn intent_message_mut_for_testing(&mut self) -> &mut IntentMessage<TransactionData> {
        &mut self.inner_mut().intent_message
    }

    // used cross-crate, so cannot be #[cfg(test)]
    pub fn tx_signatures_mut_for_testing(&mut self) -> &mut Vec<GenericSignature> {
        &mut self.inner_mut().tx_signatures
    }

    pub fn full_message_digest(&self) -> SenderSignedDataDigest {
        let mut digest = DefaultHash::default();
        bcs::serialize_into(&mut digest, self).expect("serialization should not fail");
        let hash = digest.finalize();
        SenderSignedDataDigest::new(hash.into())
    }
}

impl VersionedProtocolMessage for SenderSignedData {
    fn message_version(&self) -> Option<u64> {
        self.transaction_data().message_version()
    }

    fn check_version_supported(&self, protocol_config: &ProtocolConfig) -> SuiResult {
        self.transaction_data()
            .check_version_supported(protocol_config)?;

        // This code does nothing right now. Its purpose is to cause a compiler error when a
        // new signature type is added.
        //
        // When adding a new signature type, check if current_protocol_version
        // predates support for the new type. If it does, return
        // SuiError::WrongMessageVersion
        for sig in &self.inner().tx_signatures {
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
        TransactionDigest::new(default_hash(&self.intent_message().value))
    }

    fn verify(&self, _sig_epoch: Option<EpochId>) -> SuiResult {
        fp_ensure!(
            self.0.len() == 1,
            SuiError::UserInputError {
                error: UserInputError::Unsupported(
                    "SenderSignedData must contain exactly one transaction".to_string()
                )
            }
        );
        if self.intent_message().value.is_system_tx() {
            return Ok(());
        }

        // Verify signatures. Steps are ordered in asc complexity order to minimize abuse.
        let signers = self.intent_message().value.signers();
        // Signature number needs to match
        fp_ensure!(
            self.inner().tx_signatures.len() == signers.len(),
            SuiError::SignerSignatureNumberMismatch {
                actual: self.inner().tx_signatures.len(),
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
            signature.verify_secure_generic(self.intent_message(), signer)?;
        }
        Ok(())
    }
}

impl<S> Envelope<SenderSignedData, S> {
    pub fn sender_address(&self) -> SuiAddress {
        self.data().intent_message().value.sender()
    }

    pub fn gas(&self) -> &[ObjectRef] {
        self.data().intent_message().value.gas()
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.data()
            .inner()
            .intent_message
            .value
            .shared_input_objects()
            .into_iter()
    }

    pub fn is_system_tx(&self) -> bool {
        self.data().intent_message().value.is_system_tx()
    }

    pub fn is_sponsored_tx(&self) -> bool {
        self.data().intent_message().value.is_sponsored_tx()
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

    // TODO: Rename this function and above to make it clearer.
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
            Base64::from_bytes(&bcs::to_bytes(&self.data().intent_message().value).unwrap()),
            self.data()
                .inner()
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
        non_refundable_storage_fee: u64,
        epoch_start_timestamp_ms: u64,
        system_packages: Vec<(SequenceNumber, Vec<Vec<u8>>, Vec<ObjectID>)>,
    ) -> Self {
        ChangeEpoch {
            epoch: next_epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            system_packages,
        }
        .pipe(TransactionKind::ChangeEpoch)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_genesis_transaction(objects: Vec<GenesisObject>) -> Self {
        GenesisTransaction { objects }
            .pipe(TransactionKind::Genesis)
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
        .pipe(TransactionKind::ConsensusCommitPrologue)
        .pipe(Self::new_system_transaction)
    }

    fn new_system_transaction(system_transaction: TransactionKind) -> Self {
        system_transaction
            .pipe(TransactionData::new_system_transaction)
            .pipe(|data| {
                SenderSignedData::new_from_sender_signature(
                    data,
                    Intent::sui_transaction(),
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

impl CertifiedTransaction {
    pub fn certificate_digest(&self) -> CertificateDigest {
        let mut digest = DefaultHash::default();
        bcs::serialize_into(&mut digest, self).expect("serialization should not fail");
        let hash = digest.finalize();
        CertificateDigest::new(hash.into())
    }
}

pub type VerifiedCertificate = VerifiedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TrustedCertificate = TrustedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;

pub trait VersionedProtocolMessage {
    /// Return version of message. Some messages depend on their enclosing messages to know the
    /// version number, so not every implementor implements this.
    fn message_version(&self) -> Option<u64> {
        None
    }

    /// Check that the version of the message is the correct one to use at this protocol version.
    fn check_version_supported(&self, protocol_config: &ProtocolConfig) -> SuiResult;
}

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

    pub fn is_shared_object(&self) -> bool {
        matches!(self, Self::SharedMoveObject { .. })
    }
}

#[derive(Clone)]
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

        trace!(
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

    pub fn into_objects(self) -> Vec<(InputObjectKind, Object)> {
        self.objects
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
        write!(writer, "{}", &self.data().intent_message().value.kind())?;
        write!(f, "{}", writer)
    }
}

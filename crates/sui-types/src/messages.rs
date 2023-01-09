// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{base_types::*, committee::Committee, error::*, event::Event};
use crate::certificate_proof::CertificateProof;
use crate::committee::{EpochId, StakeUnit};
use crate::crypto::{
    sha3_hash, AuthoritySignInfo, AuthoritySignature, AuthorityStrongQuorumSignInfo,
    Ed25519SuiSignature, EmptySignInfo, Signature, SuiSignatureInner, ToFromBytes,
};
use crate::gas::GasCostSummary;
use crate::intent::{Intent, IntentMessage};
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages_checkpoint::{CheckpointSequenceNumber, CheckpointSignatureMessage};
use crate::multisig::AuthenticatorTrait;
use crate::multisig::GenericSignature;
use crate::object::{MoveObject, Object, ObjectFormatOptions, Owner, PACKAGE_VERSION};
use crate::storage::{DeleteKind, WriteKind};
use crate::{
    SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use byteorder::{BigEndian, ReadBytesExt};
use fastcrypto::encoding::{Base64, Encoding, Hex};
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
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    hash::{Hash, Hasher},
    iter,
};
use strum::IntoStaticStr;
use tap::Pipe;
use tracing::debug;

pub const DUMMY_GAS_PRICE: u64 = 1;

const BLOCKED_MOVE_FUNCTIONS: [(ObjectID, &str, &str); 3] = [
    (
        SUI_FRAMEWORK_OBJECT_ID,
        "sui_system",
        "request_add_validator",
    ),
    (
        SUI_FRAMEWORK_OBJECT_ID,
        "sui_system",
        "request_remove_validator",
    ),
    (
        SUI_FRAMEWORK_OBJECT_ID,
        "sui_system",
        "request_set_commission_rate",
    ),
];

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
        // Temporary fix until SDK will be aware of mutable flag
        #[serde(skip, default = "bool_true")]
        mutable: bool,
    },
}

fn bool_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransferObject {
    pub recipient: SuiAddress,
    pub object_ref: ObjectRef,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveCall {
    pub package: ObjectID,
    pub module: Identifier,
    pub function: Identifier,
    pub type_arguments: Vec<TypeTag>,
    pub arguments: Vec<CallArg>,
}

#[serde_as]
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveModulePublish {
    #[serde_as(as = "Vec<Bytes>")]
    pub modules: Vec<Vec<u8>>,
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

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ChangeEpoch {
    /// The next (to become) epoch ID.
    pub epoch: EpochId,
    /// The total amount of gas charged for storage during the epoch.
    pub storage_charge: u64,
    /// The total amount of gas charged for computation during the epoch.
    pub computation_charge: u64,
    /// The total amount of storage rebate refunded during the epoch.
    pub storage_rebate: u64,
    /// Unix timestamp when epoch started
    pub epoch_start_timestamp_ms: u64,
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
    // .. more transaction types go here
}

impl MoveCall {
    pub fn input_objects(&self) -> Vec<InputObjectKind> {
        let MoveCall {
            arguments, package, ..
        } = self;
        arguments
            .iter()
            .filter_map(|arg| match arg {
                CallArg::Pure(_) => None,
                CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) => {
                    Some(vec![InputObjectKind::ImmOrOwnedMoveObject(*object_ref)])
                }
                CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutable,
                }) => {
                    let id = *id;
                    let initial_shared_version = *initial_shared_version;
                    let mutable = *mutable;
                    Some(vec![InputObjectKind::SharedMoveObject {
                        id,
                        initial_shared_version,
                        mutable,
                    }])
                }
                CallArg::ObjVec(vec) => Some(
                    vec.iter()
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
                ),
            })
            .flatten()
            .chain([InputObjectKind::MovePackage(*package)])
            .collect()
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
            Self::Call(_) | Self::ChangeEpoch(_) => {
                Either::Left(self.all_move_call_shared_input_objects())
            }
            _ => Either::Right(iter::empty()),
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
            _ => unreachable!(),
        }
    }

    pub fn move_call(&self) -> Option<&MoveCall> {
        match &self {
            Self::Call(call @ MoveCall { .. }) => Some(call),
            _ => None,
        }
    }

    /// Return the metadata of each of the input objects for the transaction.
    /// For a Move object, we attach the object reference;
    /// for a Move package, we provide the object id only since they never change on chain.
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> SuiResult<Vec<InputObjectKind>> {
        let input_objects = match &self {
            Self::TransferObject(TransferObject { object_ref, .. }) => {
                vec![InputObjectKind::ImmOrOwnedMoveObject(*object_ref)]
            }
            Self::Call(move_call) => move_call.input_objects(),
            Self::Publish(MoveModulePublish { modules }) => {
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
            return Err(SuiError::DuplicateObjectRefInput);
        }
        Ok(input_objects)
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
                writeln!(writer, "Object Digest : {}", Hex::encode(digest.0))?;
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
                    writeln!(writer, "Object Digest : {}", Hex::encode(digest.0))?;
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
                    writeln!(writer, "Object Digest : {}", Hex::encode(digest.0))?;
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
                    writeln!(writer, "Object Digest : {}", Hex::encode(digest.0))?;
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
                writeln!(writer, "Transaction Kind: Epoch Change")?;
                writeln!(writer, "New epoch ID: {}", e.epoch)?;
                writeln!(writer, "Storage gas reward: {}", e.storage_charge)?;
                writeln!(writer, "Computation gas reward: {}", e.computation_charge)?;
                writeln!(writer, "Storage rebate: {}", e.storage_rebate)?;
            }
            Self::Genesis(_) => {
                writeln!(writer, "Transaction Kind: Genesis")?;
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

    pub fn input_objects(&self) -> SuiResult<Vec<InputObjectKind>> {
        let inputs: Vec<_> = self
            .single_transactions()
            .map(|s| s.input_objects())
            .collect::<SuiResult<Vec<_>>>()?
            .into_iter()
            .flatten()
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
            TransactionKind::Single(SingleTransactionKind::ChangeEpoch(_))
                | TransactionKind::Single(SingleTransactionKind::Genesis(_))
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

    fn is_blocked_move_function(&self) -> bool {
        self.single_transactions().any(|tx| match tx {
            SingleTransactionKind::Call(call) => BLOCKED_MOVE_FUNCTIONS.contains(&(
                call.package,
                call.module.as_str(),
                call.function.as_str(),
            )),
            _ => false,
        })
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
pub struct TransactionData {
    pub kind: TransactionKind,
    sender: SuiAddress,
    gas_payment: ObjectRef,
    pub gas_price: u64,
    pub gas_budget: u64,
}

impl TransactionData {
    pub fn new_with_dummy_gas_price(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
    ) -> Self {
        TransactionData {
            kind,
            sender,
            gas_price: DUMMY_GAS_PRICE,
            gas_payment,
            gas_budget,
        }
    }

    pub fn new(
        kind: TransactionKind,
        sender: SuiAddress,
        gas_payment: ObjectRef,
        gas_budget: u64,
        gas_price: u64,
    ) -> Self {
        TransactionData {
            kind,
            sender,
            gas_price,
            gas_payment,
            gas_budget,
        }
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

    pub fn gas(&self) -> ObjectRef {
        self.gas_payment
    }

    pub fn signer(&self) -> SuiAddress {
        self.sender
    }

    pub fn gas_payment_object_ref(&self) -> &ObjectRef {
        &self.gas_payment
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.kind.shared_input_objects()
    }

    pub fn move_calls(&self) -> Vec<&MoveCall> {
        self.kind
            .single_transactions()
            .flat_map(|s| s.move_call())
            .collect()
    }

    pub fn input_objects(&self) -> SuiResult<Vec<InputObjectKind>> {
        let mut inputs = self
            .kind
            .input_objects()
            .map_err(SuiError::into_transaction_input_error)?;

        if !self.kind.is_system_tx() && !self.kind.is_pay_sui_tx() {
            inputs.push(InputObjectKind::ImmOrOwnedMoveObject(
                *self.gas_payment_object_ref(),
            ));
        }
        Ok(inputs)
    }

    pub fn validity_check(&self) -> SuiResult {
        Self::validity_check_impl(&self.kind, &self.gas_payment)
    }

    pub fn validity_check_impl(kind: &TransactionKind, gas_payment: &ObjectRef) -> SuiResult {
        fp_ensure!(
            !kind.is_blocked_move_function(),
            SuiError::BlockedMoveFunction
        );
        match kind {
            TransactionKind::Batch(b) => {
                fp_ensure!(
                    !b.is_empty(),
                    SuiError::InvalidBatchTransaction {
                        error: "Batch Transaction cannot be empty".to_string(),
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
                    | SingleTransactionKind::Publish(_) => false,
                });
                fp_ensure!(
                    valid,
                    SuiError::InvalidBatchTransaction {
                        error: "Batch transaction contains non-batchable transactions. Only Call \
                        and TransferObject are allowed"
                            .to_string()
                    }
                );
            }
            TransactionKind::Single(s) => match s {
                SingleTransactionKind::Pay(_)
                | SingleTransactionKind::Call(_)
                | SingleTransactionKind::Publish(_)
                | SingleTransactionKind::TransferObject(_)
                | SingleTransactionKind::TransferSui(_)
                | SingleTransactionKind::ChangeEpoch(_)
                | SingleTransactionKind::Genesis(_) => (),
                SingleTransactionKind::PaySui(p) => {
                    fp_ensure!(!p.coins.is_empty(), SuiError::EmptyInputCoins);
                    fp_ensure!(
                        // unwrap() is safe because coins are not empty.
                        p.coins.first().unwrap() == gas_payment,
                        SuiError::UnexpectedGasPaymentObject
                    );
                }
                SingleTransactionKind::PayAllSui(pa) => {
                    fp_ensure!(!pa.coins.is_empty(), SuiError::EmptyInputCoins);
                    fp_ensure!(
                        // unwrap() is safe because coins are not empty.
                        pa.coins.first().unwrap() == gas_payment,
                        SuiError::UnexpectedGasPaymentObject
                    );
                }
            },
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SenderSignedData {
    pub intent_message: IntentMessage<TransactionData>,
    pub tx_signature: GenericSignature,
}

impl SenderSignedData {
    pub fn new(tx_data: TransactionData, intent: Intent, tx_signature: GenericSignature) -> Self {
        Self {
            intent_message: IntentMessage::new(intent, tx_data),
            tx_signature,
        }
    }
}

impl Message for SenderSignedData {
    type DigestType = TransactionDigest;

    fn digest(&self) -> Self::DigestType {
        TransactionDigest::new(sha3_hash(&self.intent_message.value))
    }

    fn verify(&self) -> SuiResult {
        if self.intent_message.value.kind.is_system_tx() {
            return Ok(());
        }
        self.tx_signature
            .verify_secure_generic(&self.intent_message, self.intent_message.value.sender)
    }
}

impl<S> Envelope<SenderSignedData, S> {
    pub fn sender_address(&self) -> SuiAddress {
        self.data().intent_message.value.sender
    }

    pub fn gas_payment_object_ref(&self) -> &ObjectRef {
        self.data().intent_message.value.gas_payment_object_ref()
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.data().intent_message.value.kind.shared_input_objects()
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
        self.data().intent_message.value.kind.is_system_tx()
    }
}

impl Transaction {
    pub fn from_data_and_signer(
        data: TransactionData,
        intent: Intent,
        signer: &dyn signature::Signer<Signature>,
    ) -> Self {
        let data1 = data.clone();
        let intent1 = intent.clone();
        let intent_msg = IntentMessage::new(intent, data);
        let signature = Signature::new_secure(&intent_msg, signer);
        Self::new(SenderSignedData::new(data1, intent1, signature.into()))
    }

    pub fn from_data(data: TransactionData, intent: Intent, signature: Signature) -> Self {
        Self::from_generic_sig_data(data, intent, signature.into())
    }

    pub fn from_generic_sig_data(
        data: TransactionData,
        intent: Intent,
        signature: GenericSignature,
    ) -> Self {
        Self::new(SenderSignedData::new(data, intent, signature))
    }

    /// Returns the Base64 encoded tx_bytes and the Base64 encoded [enum GenericSignature].
    pub fn to_tx_bytes_and_signature(&self) -> (Base64, Base64) {
        (
            Base64::from_bytes(&bcs::to_bytes(&self.data().intent_message.value).unwrap()),
            Base64::from_bytes(self.data().tx_signature.as_ref()),
        )
    }
}

impl VerifiedTransaction {
    pub fn new_change_epoch(
        next_epoch: EpochId,
        storage_charge: u64,
        computation_charge: u64,
        storage_rebate: u64,
        epoch_start_timestamp_ms: u64,
    ) -> Self {
        ChangeEpoch {
            epoch: next_epoch,
            storage_charge,
            computation_charge,
            storage_rebate,
            epoch_start_timestamp_ms,
        }
        .pipe(SingleTransactionKind::ChangeEpoch)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_genesis_transaction(objects: Vec<GenesisObject>) -> Self {
        GenesisTransaction { objects }
            .pipe(SingleTransactionKind::Genesis)
            .pipe(Self::new_system_transaction)
    }

    fn new_system_transaction(system_transaction: SingleTransactionKind) -> Self {
        system_transaction
            .pipe(TransactionKind::Single)
            .pipe(|kind| {
                TransactionData::new_with_dummy_gas_price(
                    kind,
                    SuiAddress::default(),
                    (ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN),
                    0,
                )
            })
            .pipe(|data| SenderSignedData {
                intent_message: IntentMessage::new(Intent::default(), data),
                tx_signature: GenericSignature::Signature(
                    Ed25519SuiSignature::from_bytes(&[0; Ed25519SuiSignature::LENGTH])
                        .unwrap()
                        .into(),
                ),
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
        secret: &dyn signature::Signer<AuthoritySignature>,
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

/// A transaction that is signed by a sender and also by an authority.
pub type SignedTransaction = Envelope<SenderSignedData, AuthoritySignInfo>;
pub type VerifiedSignedTransaction = VerifiedEnvelope<SenderSignedData, AuthoritySignInfo>;

pub type CertifiedTransaction = Envelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TxCertAndSignedEffects = (CertifiedTransaction, SignedTransactionEffects);

pub type VerifiedCertificate = VerifiedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TrustedCertificate = TrustedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum ObjectInfoRequestKind {
    /// Request the latest object state, if a format option is provided,
    /// return the layout of the object in the given format.
    LatestObjectInfo(Option<ObjectFormatOptions>),
    /// Request the object state at a specific version
    PastObjectInfo(SequenceNumber),
    /// Similar to PastObjectInfo, except that it will also return the object content.
    /// This is used only for debugging purpose and will not work in the long run when
    /// we stop storing all historic versions of every object.
    /// No production code should depend on this kind.
    PastObjectInfoDebug(SequenceNumber, Option<ObjectFormatOptions>),
}

/// A request for information about an object and optionally its
/// parent certificate at a specific version.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ObjectInfoRequest {
    /// The id of the object to retrieve, at the latest version.
    pub object_id: ObjectID,
    /// The type of request, either latest object info or the past.
    pub request_kind: ObjectInfoRequestKind,
}

impl ObjectInfoRequest {
    pub fn past_object_info_request(object_id: ObjectID, version: SequenceNumber) -> Self {
        ObjectInfoRequest {
            object_id,
            request_kind: ObjectInfoRequestKind::PastObjectInfo(version),
        }
    }

    pub fn latest_object_info_request(
        object_id: ObjectID,
        layout: Option<ObjectFormatOptions>,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo(layout),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectResponse<T = SignedTransaction> {
    /// Value of the requested object in this authority
    pub object: Object,
    /// Transaction the object is locked on in this authority.
    /// None if the object is not currently locked by this authority.
    pub lock: Option<T>,
    /// Schema of the Move value inside this object.
    /// None if the object is a Move package, or the request did not ask for the layout
    pub layout: Option<MoveStructLayout>,
}

impl From<ObjectResponse<VerifiedSignedTransaction>> for ObjectResponse {
    fn from(o: ObjectResponse<VerifiedSignedTransaction>) -> Self {
        let ObjectResponse {
            object,
            lock,
            layout,
        } = o;

        Self {
            object,
            lock: lock.map(|l| l.into()),
            layout,
        }
    }
}

/// This message provides information about the latest object and its lock
/// as well as the parent certificate of the object at a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfoResponse<TxnT = SignedTransaction, CertT = CertifiedTransaction> {
    /// The certificate that created or mutated the object at a given version.
    /// If no parent certificate was requested the latest certificate concerning
    /// this object is sent. If the parent was requested and not found a error
    /// (ParentNotfound or CertificateNotfound) will be returned.
    pub parent_certificate: Option<CertT>,
    /// The full reference created by the above certificate
    pub requested_object_reference: Option<ObjectRef>,

    /// The object and its current lock, returned only if we are requesting
    /// the latest state of an object.
    /// If the object does not exist this is also None.
    pub object_and_lock: Option<ObjectResponse<TxnT>>,
}

pub type VerifiedObjectInfoResponse =
    ObjectInfoResponse<VerifiedSignedTransaction, VerifiedCertificate>;

impl ObjectInfoResponse {
    pub fn object(&self) -> Option<&Object> {
        match &self.object_and_lock {
            Some(ObjectResponse { object, .. }) => Some(object),
            _ => None,
        }
    }
}

impl From<VerifiedObjectInfoResponse> for ObjectInfoResponse {
    fn from(o: VerifiedObjectInfoResponse) -> Self {
        let ObjectInfoResponse {
            parent_certificate,
            requested_object_reference,
            object_and_lock,
        } = o;
        Self {
            parent_certificate: parent_certificate.map(|p| p.into()),
            requested_object_reference,
            object_and_lock: object_and_lock.map(|o| o.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransactionInfoRequest {
    pub transaction_digest: TransactionDigest,
}

impl From<TransactionDigest> for TransactionInfoRequest {
    fn from(transaction_digest: TransactionDigest) -> Self {
        TransactionInfoRequest { transaction_digest }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleCertificateResponse {
    pub signed_effects: SignedTransactionEffects,
}

#[derive(Clone, Debug)]
pub struct VerifiedHandleCertificateResponse {
    pub signed_effects: VerifiedSignedTransactionEffects,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInfoResponse<
    TxnT = SignedTransaction,
    CertT = CertifiedTransaction,
    EffectsT = SignedTransactionEffects,
> {
    // The signed transaction response to handle_transaction
    pub signed_transaction: Option<TxnT>,
    // The certificate in case one is available
    pub certified_transaction: Option<CertT>,
    // The effects resulting from a successful execution should
    // contain ObjectRef created, mutated, deleted and events.
    pub signed_effects: Option<EffectsT>,
}

pub type VerifiedTransactionInfoResponse = TransactionInfoResponse<
    VerifiedSignedTransaction,
    VerifiedCertificate,
    VerifiedSignedTransactionEffects,
>;

impl From<VerifiedTransactionInfoResponse> for TransactionInfoResponse {
    fn from(v: VerifiedTransactionInfoResponse) -> Self {
        let VerifiedTransactionInfoResponse {
            signed_transaction,
            certified_transaction,
            signed_effects,
        } = v;

        let certified_transaction = certified_transaction.map(|c| c.into_inner());
        let signed_transaction = signed_transaction.map(|c| c.into_inner());
        let signed_effects = signed_effects.map(|s| s.into_inner());
        TransactionInfoResponse {
            signed_transaction,
            certified_transaction,
            signed_effects,
        }
    }
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
    // Gas used in the failed case, and the error.
    Failure { error: ExecutionFailureStatus },
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionFailureStatus {
    //
    // General transaction errors
    //
    InsufficientGas,
    InvalidGasObject,
    InvalidTransactionUpdate,
    ModuleNotFound,
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
    ModuleNotFound,
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
            ExecutionFailureStatus::ModuleNotFound => write!(f, "Module Not Found."),
            ExecutionFailureStatus::MoveObjectTooBig { object_size, max_object_size } => write!(f, "Move object with size {object_size} is larger than the maximum object size {max_object_size}"),
            ExecutionFailureStatus::MovePackageTooBig { object_size, max_object_size } => write!(f, "Move package with size {object_size} is larger than the maximum object size {max_object_size}"),
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
            EntryTypeArgumentErrorKind::ModuleNotFound => write!(
                f,
                "A package (or module) in the type argument was not found"
            ),
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
    pub fn new_failure(error: ExecutionFailureStatus) -> ExecutionStatus {
        ExecutionStatus::Failure { error }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ExecutionStatus::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, ExecutionStatus::Failure { .. })
    }

    pub fn unwrap(self) {
        match self {
            ExecutionStatus::Success => {}
            ExecutionStatus::Failure { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
        }
    }

    pub fn unwrap_err(self) -> ExecutionFailureStatus {
        match self {
            ExecutionStatus::Success { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
            ExecutionStatus::Failure { error } => error,
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

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffects {
    /// The status of the execution
    pub status: ExecutionStatus,
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
    /// Object refs of objects now wrapped in other objects.
    pub wrapped: Vec<ObjectRef>,
    /// The updated gas object reference. Have a dedicated field for convenient access.
    /// It's also included in mutated.
    pub gas_object: (ObjectRef, Owner),
    /// The events emitted during execution. Note that only successful transactions emit events
    pub events: Vec<Event>,
    /// The set of transaction digests this transaction depends on.
    pub dependencies: Vec<TransactionDigest>,
}

impl TransactionEffects {
    /// Return an iterator that iterates through all mutated objects, including mutated,
    /// created and unwrapped objects. In other words, all objects that still exist
    /// in the object state after this transaction.
    /// It doesn't include deleted/wrapped objects.
    pub fn all_mutated(&self) -> impl Iterator<Item = (&ObjectRef, &Owner, WriteKind)> + Clone {
        self.mutated
            .iter()
            .map(|(r, o)| (r, o, WriteKind::Mutate))
            .chain(self.created.iter().map(|(r, o)| (r, o, WriteKind::Create)))
            .chain(
                self.unwrapped
                    .iter()
                    .map(|(r, o)| (r, o, WriteKind::Unwrap)),
            )
    }

    pub fn execution_digests(&self) -> ExecutionDigests {
        ExecutionDigests {
            transaction: self.transaction_digest,
            effects: self.digest(),
        }
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    pub fn mutated_excluding_gas(&self) -> impl Iterator<Item = &(ObjectRef, Owner)> {
        self.mutated.iter().filter(|o| *o != &self.gas_object)
    }

    pub fn gas_cost_summary(&self) -> &GasCostSummary {
        &self.gas_used
    }
}

impl Message for TransactionEffectsDigest {
    type DigestType = TransactionEffectsDigest;

    fn digest(&self) -> Self::DigestType {
        *self
    }

    fn verify(&self) -> SuiResult {
        Ok(())
    }
}

impl Message for ExecutionDigests {
    type DigestType = TransactionDigest;

    fn digest(&self) -> Self::DigestType {
        self.transaction
    }

    fn verify(&self) -> SuiResult {
        Ok(())
    }
}

impl Message for TransactionEffects {
    type DigestType = TransactionEffectsDigest;

    fn digest(&self) -> Self::DigestType {
        TransactionEffectsDigest(sha3_hash(self))
    }

    fn verify(&self) -> SuiResult {
        Ok(())
    }
}

impl Display for TransactionEffects {
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
        TransactionEffects {
            status: ExecutionStatus::Success,
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
            wrapped: Vec::new(),
            gas_object: (
                random_object_ref(),
                Owner::AddressOwner(SuiAddress::default()),
            ),
            events: Vec::new(),
            dependencies: Vec::new(),
        }
    }
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

pub type ValidExecutionDigests = Envelope<ExecutionDigests, CertificateProof>;
pub type ValidTransactionEffectsDigest = Envelope<TransactionEffectsDigest, CertificateProof>;
pub type ValidTransactionEffects = TransactionEffectsEnvelope<CertificateProof>;

impl From<ValidExecutionDigests> for ValidTransactionEffectsDigest {
    fn from(ved: ValidExecutionDigests) -> ValidTransactionEffectsDigest {
        let (data, validity) = ved.into_data_and_sig();
        ValidTransactionEffectsDigest::new(data.effects, validity)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
            Self::MovePackage(..) => Some(PACKAGE_VERSION),
            Self::ImmOrOwnedMoveObject((_, version, _)) => Some(*version),
            Self::SharedMoveObject { .. } => None,
        }
    }

    pub fn object_not_found_error(&self) -> SuiError {
        match *self {
            Self::MovePackage(package_id) => SuiError::DependentPackageNotFound { package_id },
            Self::ImmOrOwnedMoveObject((object_id, version, _)) => SuiError::ObjectNotFound {
                object_id,
                version: Some(version),
            },
            Self::SharedMoveObject { id, .. } => SuiError::ObjectNotFound {
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
        write!(writer, "{}", &self.data().intent_message.value.kind)?;
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
}

impl Debug for ConsensusTransactionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(digest) => write!(f, "Certificate({:?})", digest),
            Self::CheckpointSignature(name, seq) => {
                write!(f, "CheckpointSignature({:?}, {:?})", name.concise(), seq)
            }
            Self::EndOfPublish(name) => write!(f, "EndOfPublish({:?})", name.concise()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ConsensusTransactionKind {
    UserTransaction(Box<CertifiedTransaction>),
    CheckpointSignature(Box<CheckpointSignatureMessage>),
    EndOfPublish(AuthorityName),
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
            ConsensusTransactionKind::EndOfPublish(_) => Ok(()),
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

/// When requested to execute a transaction with WaitForLocalExecution,
/// TransactionOrchestrator attempts to execute this transaction locally
/// after it is finalized. This value represents whether the transaction
/// is confirmed to be executed on this node before the response returns.
pub type IsTransactionExecutedLocally = bool;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ExecuteTransactionResponse {
    EffectsCert(
        Box<(
            CertifiedTransaction,
            CertifiedTransactionEffects,
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
    pub tx_cert: VerifiedCertificate,
    pub effects_cert: VerifiedCertifiedTransactionEffects,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommitteeInfoRequest {
    pub epoch: Option<EpochId>,
}

#[derive(Serialize, Deserialize, Clone, schemars::JsonSchema, Debug)]
pub struct CommitteeInfoResponse {
    pub epoch: EpochId,
    pub committee_info: Option<Vec<(AuthorityName, StakeUnit)>>,
    // TODO: We could also return the certified checkpoint that contains this committee.
    // This would allows a client to verify the authenticity of the committee.
}

pub type CommitteeInfoResponseDigest = [u8; 32];

impl CommitteeInfoResponse {
    pub fn digest(&self) -> CommitteeInfoResponseDigest {
        sha3_hash(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommitteeInfo {
    pub epoch: EpochId,
    pub committee_info: Vec<(AuthorityName, StakeUnit)>,
    // TODO: We could also return the certified checkpoint that contains this committee.
    // This would allows a client to verify the authenticity of the committee.
}

// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{base_types::*, error::*, SUI_BRIDGE_OBJECT_ID};
use crate::authenticator_state::ActiveJwk;
use crate::committee::{Committee, EpochId, ProtocolVersion};
use crate::crypto::{
    default_hash, AuthoritySignInfo, AuthoritySignInfoTrait, AuthoritySignature,
    AuthorityStrongQuorumSignInfo, DefaultHash, Ed25519SuiSignature, EmptySignInfo,
    RandomnessRound, Signature, Signer, SuiSignatureInner, ToFromBytes,
};
use crate::digests::{CertificateDigest, SenderSignedDataDigest};
use crate::digests::{ChainIdentifier, ConsensusCommitDigest, ZKLoginInputsDigest};
use crate::execution::SharedInput;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages_checkpoint::CheckpointTimestamp;
use crate::messages_consensus::{
    ConsensusCommitPrologue, ConsensusCommitPrologueV2, ConsensusCommitPrologueV3,
    ConsensusDeterminedVersionAssignments,
};
use crate::object::{MoveObject, Object, Owner};
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::signature::{GenericSignature, VerifyParams};
use crate::signature_verification::{
    verify_sender_signed_data_message_signatures, VerifiedDigestCache,
};
use crate::type_input::TypeInput;
use crate::{
    SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_AUTHENTICATOR_STATE_OBJECT_SHARED_VERSION,
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_RANDOMNESS_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use enum_dispatch::enum_dispatch;
use fastcrypto::{encoding::Base64, hash::HashFunction};
use itertools::Either;
use move_core_types::{ident_str, identifier};
use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use nonempty::{nonempty, NonEmpty};
use serde::{Deserialize, Serialize};
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::fmt::Write;
use std::fmt::{Debug, Display, Formatter};
use std::iter::once;
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    hash::Hash,
    iter,
};
use strum::IntoStaticStr;
use sui_protocol_config::ProtocolConfig;
use tap::Pipe;
use tracing::trace;

pub const TEST_ONLY_GAS_UNIT_FOR_TRANSFER: u64 = 10_000;
pub const TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS: u64 = 50_000;
pub const TEST_ONLY_GAS_UNIT_FOR_PUBLISH: u64 = 70_000;
pub const TEST_ONLY_GAS_UNIT_FOR_STAKING: u64 = 50_000;
pub const TEST_ONLY_GAS_UNIT_FOR_GENERIC: u64 = 50_000;
pub const TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN: u64 = 10_000;
// For some transactions we may either perform heavy operations or touch
// objects that are storage expensive. That may happen (and often is the case)
// because the object touched are set up in genesis and carry no storage cost
// (and thus rebate) on first usage.
pub const TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE: u64 = 5_000_000;

pub const GAS_PRICE_FOR_SYSTEM_TX: u64 = 1;

pub const DEFAULT_VALIDATOR_GAS_PRICE: u64 = 1000;

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

impl CallArg {
    pub const SUI_SYSTEM_MUT: Self = Self::Object(ObjectArg::SUI_SYSTEM_MUT);
    pub const CLOCK_IMM: Self = Self::Object(ObjectArg::SharedObject {
        id: SUI_CLOCK_OBJECT_ID,
        initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
        mutable: false,
    });
    pub const CLOCK_MUT: Self = Self::Object(ObjectArg::SharedObject {
        id: SUI_CLOCK_OBJECT_ID,
        initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
        mutable: true,
    });
    pub const AUTHENTICATOR_MUT: Self = Self::Object(ObjectArg::SharedObject {
        id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
        initial_shared_version: SUI_AUTHENTICATOR_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum ObjectArg {
    // A Move object from fastpath.
    ImmOrOwnedObject(ObjectRef),
    // A Move object from consensus (historically consensus objects were always shared).
    // SharedObject::mutable controls whether caller asks for a mutable reference to shared object.
    SharedObject {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        mutable: bool,
    },
    // A Move object that can be received in this transaction.
    Receiving(ObjectRef),
}

fn type_input_validity_check(
    tag: &TypeInput,
    config: &ProtocolConfig,
    starting_count: &mut usize,
) -> UserInputResult<()> {
    let mut stack = vec![(tag, 1)];
    while let Some((tag, depth)) = stack.pop() {
        *starting_count += 1;
        fp_ensure!(
            *starting_count < config.max_type_arguments() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum type arguments in a call transaction".to_string(),
                value: config.max_type_arguments().to_string()
            }
        );
        fp_ensure!(
            depth < config.max_type_argument_depth(),
            UserInputError::SizeLimitExceeded {
                limit: "maximum type argument depth in a call transaction".to_string(),
                value: config.max_type_argument_depth().to_string()
            }
        );
        match tag {
            TypeInput::Bool
            | TypeInput::U8
            | TypeInput::U64
            | TypeInput::U128
            | TypeInput::Address
            | TypeInput::Signer
            | TypeInput::U16
            | TypeInput::U32
            | TypeInput::U256 => (),
            TypeInput::Vector(t) => {
                stack.push((t, depth + 1));
            }
            TypeInput::Struct(s) => {
                let next_depth = depth + 1;
                if config.validate_identifier_inputs() {
                    fp_ensure!(
                        identifier::is_valid(&s.module),
                        UserInputError::InvalidIdentifier {
                            error: s.module.clone()
                        }
                    );
                    fp_ensure!(
                        identifier::is_valid(&s.name),
                        UserInputError::InvalidIdentifier {
                            error: s.name.clone()
                        }
                    );
                }
                stack.extend(s.type_params.iter().map(|t| (t, next_depth)));
            }
        }
    }
    Ok(())
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct AuthenticatorStateExpire {
    /// expire JWKs that have a lower epoch than this
    pub min_epoch: u64,
    /// The initial version of the authenticator object that it was shared at.
    pub authenticator_obj_initial_shared_version: SequenceNumber,
}

impl AuthenticatorStateExpire {
    pub fn authenticator_obj_initial_shared_version(&self) -> SequenceNumber {
        self.authenticator_obj_initial_shared_version
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct AuthenticatorStateUpdate {
    /// Epoch of the authenticator state update transaction
    pub epoch: u64,
    /// Consensus round of the authenticator state update
    pub round: u64,
    /// newly active jwks
    pub new_active_jwks: Vec<ActiveJwk>,
    /// The initial version of the authenticator object that it was shared at.
    pub authenticator_obj_initial_shared_version: SequenceNumber,
    // to version this struct, do not add new fields. Instead, add a AuthenticatorStateUpdateV2 to
    // TransactionKind.
}

impl AuthenticatorStateUpdate {
    pub fn authenticator_obj_initial_shared_version(&self) -> SequenceNumber {
        self.authenticator_obj_initial_shared_version
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct RandomnessStateUpdate {
    /// Epoch of the randomness state update transaction
    pub epoch: u64,
    /// Randomness round of the update
    pub randomness_round: RandomnessRound,
    /// Updated random bytes
    pub random_bytes: Vec<u8>,
    /// The initial version of the randomness object that it was shared at.
    pub randomness_obj_initial_shared_version: SequenceNumber,
    // to version this struct, do not add new fields. Instead, add a RandomnessStateUpdateV2 to
    // TransactionKind.
}

impl RandomnessStateUpdate {
    pub fn randomness_obj_initial_shared_version(&self) -> SequenceNumber {
        self.randomness_obj_initial_shared_version
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
    ///
    /// The ChangeEpoch enumerant is now deprecated (but the ChangeEpoch struct is still used by
    /// EndOfEpochTransaction below).
    ChangeEpoch(ChangeEpoch),
    Genesis(GenesisTransaction),
    ConsensusCommitPrologue(ConsensusCommitPrologue),
    AuthenticatorStateUpdate(AuthenticatorStateUpdate),

    /// EndOfEpochTransaction replaces ChangeEpoch with a list of transactions that are allowed to
    /// run at the end of the epoch.
    EndOfEpochTransaction(Vec<EndOfEpochTransactionKind>),

    RandomnessStateUpdate(RandomnessStateUpdate),
    // V2 ConsensusCommitPrologue also includes the digest of the current consensus output.
    ConsensusCommitPrologueV2(ConsensusCommitPrologueV2),

    ConsensusCommitPrologueV3(ConsensusCommitPrologueV3),
    // .. more transaction types go here
}

/// EndOfEpochTransactionKind
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, IntoStaticStr)]
pub enum EndOfEpochTransactionKind {
    ChangeEpoch(ChangeEpoch),
    AuthenticatorStateCreate,
    AuthenticatorStateExpire(AuthenticatorStateExpire),
    RandomnessStateCreate,
    DenyListStateCreate,
    BridgeStateCreate(ChainIdentifier),
    BridgeCommitteeInit(SequenceNumber),
}

impl EndOfEpochTransactionKind {
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
        Self::ChangeEpoch(ChangeEpoch {
            epoch: next_epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            system_packages,
        })
    }

    pub fn new_authenticator_state_expire(
        min_epoch: u64,
        authenticator_obj_initial_shared_version: SequenceNumber,
    ) -> Self {
        Self::AuthenticatorStateExpire(AuthenticatorStateExpire {
            min_epoch,
            authenticator_obj_initial_shared_version,
        })
    }

    pub fn new_authenticator_state_create() -> Self {
        Self::AuthenticatorStateCreate
    }

    pub fn new_randomness_state_create() -> Self {
        Self::RandomnessStateCreate
    }

    pub fn new_deny_list_state_create() -> Self {
        Self::DenyListStateCreate
    }

    pub fn new_bridge_create(chain_identifier: ChainIdentifier) -> Self {
        Self::BridgeStateCreate(chain_identifier)
    }

    pub fn init_bridge_committee(bridge_shared_version: SequenceNumber) -> Self {
        Self::BridgeCommitteeInit(bridge_shared_version)
    }

    fn input_objects(&self) -> Vec<InputObjectKind> {
        match self {
            Self::ChangeEpoch(_) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                }]
            }
            Self::AuthenticatorStateCreate => vec![],
            Self::AuthenticatorStateExpire(expire) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    initial_shared_version: expire.authenticator_obj_initial_shared_version(),
                    mutable: true,
                }]
            }
            Self::RandomnessStateCreate => vec![],
            Self::DenyListStateCreate => vec![],
            Self::BridgeStateCreate(_) => vec![],
            Self::BridgeCommitteeInit(bridge_version) => vec![
                InputObjectKind::SharedMoveObject {
                    id: SUI_BRIDGE_OBJECT_ID,
                    initial_shared_version: *bridge_version,
                    mutable: true,
                },
                InputObjectKind::SharedMoveObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                },
            ],
        }
    }

    fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match self {
            Self::ChangeEpoch(_) => {
                Either::Left(vec![SharedInputObject::SUI_SYSTEM_OBJ].into_iter())
            }
            Self::AuthenticatorStateExpire(expire) => Either::Left(
                vec![SharedInputObject {
                    id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    initial_shared_version: expire.authenticator_obj_initial_shared_version(),
                    mutable: true,
                }]
                .into_iter(),
            ),
            Self::AuthenticatorStateCreate => Either::Right(iter::empty()),
            Self::RandomnessStateCreate => Either::Right(iter::empty()),
            Self::DenyListStateCreate => Either::Right(iter::empty()),
            Self::BridgeStateCreate(_) => Either::Right(iter::empty()),
            Self::BridgeCommitteeInit(bridge_version) => Either::Left(
                vec![
                    SharedInputObject {
                        id: SUI_BRIDGE_OBJECT_ID,
                        initial_shared_version: *bridge_version,
                        mutable: true,
                    },
                    SharedInputObject::SUI_SYSTEM_OBJ,
                ]
                .into_iter(),
            ),
        }
    }

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult {
        match self {
            Self::ChangeEpoch(_) => (),
            Self::AuthenticatorStateCreate | Self::AuthenticatorStateExpire(_) => {
                if !config.enable_jwk_consensus_updates() {
                    return Err(UserInputError::Unsupported(
                        "authenticator state updates not enabled".to_string(),
                    ));
                }
            }
            Self::RandomnessStateCreate => {
                if !config.random_beacon() {
                    return Err(UserInputError::Unsupported(
                        "random beacon not enabled".to_string(),
                    ));
                }
            }
            Self::DenyListStateCreate => {
                if !config.enable_coin_deny_list_v1() {
                    return Err(UserInputError::Unsupported(
                        "coin deny list not enabled".to_string(),
                    ));
                }
            }
            Self::BridgeStateCreate(_) => {
                if !config.enable_bridge() {
                    return Err(UserInputError::Unsupported(
                        "bridge not enabled".to_string(),
                    ));
                }
            }
            Self::BridgeCommitteeInit(_) => {
                if !config.enable_bridge() {
                    return Err(UserInputError::Unsupported(
                        "bridge not enabled".to_string(),
                    ));
                }
                if !config.should_try_to_finalize_bridge_committee() {
                    return Err(UserInputError::Unsupported(
                        "should not try to finalize committee yet".to_string(),
                    ));
                }
            }
        }
        Ok(())
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
            // Receiving objects are not part of the input objects.
            CallArg::Object(ObjectArg::Receiving(_)) => vec![],
        }
    }

    fn receiving_objects(&self) -> Vec<ObjectRef> {
        match self {
            CallArg::Pure(_) => vec![],
            CallArg::Object(o) => match o {
                ObjectArg::ImmOrOwnedObject(_) => vec![],
                ObjectArg::SharedObject { .. } => vec![],
                ObjectArg::Receiving(obj_ref) => vec![*obj_ref],
            },
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
            CallArg::Object(o) => match o {
                ObjectArg::ImmOrOwnedObject(_) | ObjectArg::SharedObject { .. } => (),
                ObjectArg::Receiving(_) => {
                    if !config.receiving_objects_supported() {
                        return Err(UserInputError::Unsupported(format!(
                            "receiving objects is not supported at {:?}",
                            config.version
                        )));
                    }
                }
            },
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
    pub const SUI_SYSTEM_MUT: Self = Self::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    };

    pub fn id(&self) -> ObjectID {
        match self {
            ObjectArg::Receiving((id, _, _))
            | ObjectArg::ImmOrOwnedObject((id, _, _))
            | ObjectArg::SharedObject { id, .. } => *id,
        }
    }
}

// Add package IDs, `ObjectID`, for types defined in modules.
fn add_type_input_packages(packages: &mut BTreeSet<ObjectID>, type_argument: &TypeInput) {
    let mut stack = vec![type_argument];
    while let Some(cur) = stack.pop() {
        match cur {
            TypeInput::Bool
            | TypeInput::U8
            | TypeInput::U64
            | TypeInput::U128
            | TypeInput::Address
            | TypeInput::Signer
            | TypeInput::U16
            | TypeInput::U32
            | TypeInput::U256 => (),
            TypeInput::Vector(inner) => stack.push(inner),
            TypeInput::Struct(struct_tag) => {
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
    MakeMoveVec(Option<TypeInput>, Vec<Argument>),
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
    pub module: String,
    /// The function to be called.
    pub function: String,
    /// The type arguments to the function.
    pub type_arguments: Vec<TypeInput>,
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
            add_type_input_packages(&mut packages, type_argument)
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
        for tag in &self.type_arguments {
            type_input_validity_check(tag, config, &mut type_arguments_count)?;
        }
        fp_ensure!(
            self.arguments.len() < config.max_arguments() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum arguments in a move call".to_string(),
                value: config.max_arguments().to_string()
            }
        );
        if config.validate_identifier_inputs() {
            fp_ensure!(
                identifier::is_valid(&self.module),
                UserInputError::InvalidIdentifier {
                    error: self.module.clone()
                }
            );
            fp_ensure!(
                identifier::is_valid(&self.function),
                UserInputError::InvalidIdentifier {
                    error: self.module.clone()
                }
            );
        }
        Ok(())
    }

    fn is_input_arg_used(&self, arg: u16) -> bool {
        self.arguments
            .iter()
            .any(|a| matches!(a, Argument::Input(inp) if *inp == arg))
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
        let module = module.to_string();
        let function = function.to_string();
        let type_arguments = type_arguments.into_iter().map(TypeInput::from).collect();
        Command::MoveCall(Box::new(ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        }))
    }

    pub fn make_move_vec(ty: Option<TypeTag>, args: Vec<Argument>) -> Self {
        Command::MakeMoveVec(ty.map(TypeInput::from), args)
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
                add_type_input_packages(&mut packages, t);
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
                    let mut type_arguments_count = 0;
                    type_input_validity_check(ty, config, &mut type_arguments_count)?;
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
            Command::Publish(modules, deps) | Command::Upgrade(modules, deps, _, _) => {
                fp_ensure!(!modules.is_empty(), UserInputError::EmptyCommandInput);
                fp_ensure!(
                    modules.len() < config.max_modules_in_publish() as usize,
                    UserInputError::SizeLimitExceeded {
                        limit: "maximum modules in a programmable transaction upgrade command"
                            .to_string(),
                        value: config.max_modules_in_publish().to_string()
                    }
                );
                if let Some(max_package_dependencies) = config.max_package_dependencies_as_option()
                {
                    fp_ensure!(
                        deps.len() < max_package_dependencies as usize,
                        UserInputError::SizeLimitExceeded {
                            limit: "maximum package dependencies".to_string(),
                            value: max_package_dependencies.to_string()
                        }
                    );
                };
            }
        };
        Ok(())
    }

    fn is_input_arg_used(&self, input_arg: u16) -> bool {
        match self {
            Command::MoveCall(c) => c.is_input_arg_used(input_arg),
            Command::TransferObjects(args, arg)
            | Command::MergeCoins(arg, args)
            | Command::SplitCoins(arg, args) => args
                .iter()
                .chain(once(arg))
                .any(|a| matches!(a, Argument::Input(inp) if *inp == input_arg)),
            Command::MakeMoveVec(_, args) => args
                .iter()
                .any(|a| matches!(a, Argument::Input(inp) if *inp == input_arg)),
            Command::Upgrade(_, _, _, arg) => {
                matches!(arg, Argument::Input(inp) if *inp == input_arg)
            }
            Command::Publish(_, _) => false,
        }
    }
}

pub fn write_sep<T: Display>(
    f: &mut Formatter<'_>,
    items: impl IntoIterator<Item = T>,
    sep: &str,
) -> std::fmt::Result {
    let mut xs = items.into_iter();
    let Some(x) = xs.next() else {
        return Ok(());
    };
    write!(f, "{x}")?;
    for x in xs {
        write!(f, "{sep}{x}")?;
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

    fn receiving_objects(&self) -> Vec<ObjectRef> {
        let ProgrammableTransaction { inputs, .. } = self;
        inputs
            .iter()
            .flat_map(|arg| arg.receiving_objects())
            .collect()
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
        let total_inputs = self.input_objects()?.len() + self.receiving_objects().len();
        fp_ensure!(
            total_inputs <= config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input + receiving objects in a transaction".to_string(),
                value: config.max_input_objects().to_string()
            }
        );
        for input in inputs {
            input.validity_check(config)?
        }
        if let Some(max_publish_commands) = config.max_publish_or_upgrade_per_ptb_as_option() {
            let publish_count = commands
                .iter()
                .filter(|c| matches!(c, Command::Publish(_, _) | Command::Upgrade(_, _, _, _)))
                .count() as u64;
            fp_ensure!(
                publish_count <= max_publish_commands,
                UserInputError::MaxPublishCountExceeded {
                    max_publish_commands,
                    publish_count,
                }
            );
        }
        for command in commands {
            command.validity_check(config)?;
        }

        // If randomness is used, it must be enabled by protocol config.
        // A command that uses Random can only be followed by TransferObjects or MergeCoins.
        if let Some(random_index) = inputs.iter().position(|obj| {
            matches!(obj, CallArg::Object(ObjectArg::SharedObject { id, .. }) if *id == SUI_RANDOMNESS_STATE_OBJECT_ID)
        }) {
            fp_ensure!(
                config.random_beacon(),
                UserInputError::Unsupported(
                    "randomness is not enabled on this network".to_string(),
                )
            );
            let mut used_random_object = false;
            let random_index = random_index.try_into().unwrap();
            for command in commands {
                if !used_random_object {
                    used_random_object = command.is_input_arg_used(random_index);
                } else {
                    fp_ensure!(
                        matches!(
                            command,
                            Command::TransferObjects(_, _) | Command::MergeCoins(_, _)
                        ),
                        UserInputError::PostRandomCommandRestrictions
                    );
                }
            }
        }

        Ok(())
    }

    fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        self.inputs
            .iter()
            .filter_map(|arg| match arg {
                CallArg::Pure(_)
                | CallArg::Object(ObjectArg::Receiving(_))
                | CallArg::Object(ObjectArg::ImmOrOwnedObject(_)) => None,
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

    fn move_calls(&self) -> Vec<(&ObjectID, &str, &str)> {
        self.commands
            .iter()
            .filter_map(|command| match command {
                Command::MoveCall(m) => Some((&m.package, m.module.as_str(), m.function.as_str())),
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
    pub const SUI_SYSTEM_OBJ: Self = Self {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    };

    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn id_and_version(&self) -> (ObjectID, SequenceNumber) {
        (self.id, self.initial_shared_version)
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
        // Keep this as an exhaustive match so that we can't forget to update it.
        match self {
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_)
            | TransactionKind::ConsensusCommitPrologueV2(_)
            | TransactionKind::ConsensusCommitPrologueV3(_)
            | TransactionKind::AuthenticatorStateUpdate(_)
            | TransactionKind::RandomnessStateUpdate(_)
            | TransactionKind::EndOfEpochTransaction(_) => true,
            TransactionKind::ProgrammableTransaction(_) => false,
        }
    }

    pub fn is_end_of_epoch_tx(&self) -> bool {
        matches!(
            self,
            TransactionKind::EndOfEpochTransaction(_) | TransactionKind::ChangeEpoch(_)
        )
    }

    /// If this is advance epoch transaction, returns (total gas charged, total gas rebated).
    /// TODO: We should use GasCostSummary directly in ChangeEpoch struct, and return that
    /// directly.
    pub fn get_advance_epoch_tx_gas_summary(&self) -> Option<(u64, u64)> {
        let e = match self {
            Self::ChangeEpoch(e) => e,
            Self::EndOfEpochTransaction(txns) => {
                if let EndOfEpochTransactionKind::ChangeEpoch(e) =
                    txns.last().expect("at least one end-of-epoch txn required")
                {
                    e
                } else {
                    panic!("final end-of-epoch txn must be ChangeEpoch")
                }
            }
            _ => return None,
        };

        Some((e.computation_charge + e.storage_charge, e.storage_rebate))
    }

    pub fn contains_shared_object(&self) -> bool {
        self.shared_input_objects().next().is_some()
    }

    /// Returns an iterator of all shared input objects used by this transaction.
    /// It covers both Call and ChangeEpoch transaction kind, because both makes Move calls.
    pub fn shared_input_objects(&self) -> impl Iterator<Item = SharedInputObject> + '_ {
        match &self {
            Self::ChangeEpoch(_) => {
                Either::Left(Either::Left(iter::once(SharedInputObject::SUI_SYSTEM_OBJ)))
            }

            Self::ConsensusCommitPrologue(_)
            | Self::ConsensusCommitPrologueV2(_)
            | Self::ConsensusCommitPrologueV3(_) => {
                Either::Left(Either::Left(iter::once(SharedInputObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutable: true,
                })))
            }
            Self::AuthenticatorStateUpdate(update) => {
                Either::Left(Either::Left(iter::once(SharedInputObject {
                    id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    initial_shared_version: update.authenticator_obj_initial_shared_version,
                    mutable: true,
                })))
            }
            Self::RandomnessStateUpdate(update) => {
                Either::Left(Either::Left(iter::once(SharedInputObject {
                    id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    initial_shared_version: update.randomness_obj_initial_shared_version,
                    mutable: true,
                })))
            }
            Self::EndOfEpochTransaction(txns) => Either::Left(Either::Right(
                txns.iter().flat_map(|txn| txn.shared_input_objects()),
            )),
            Self::ProgrammableTransaction(pt) => {
                Either::Right(Either::Left(pt.shared_input_objects()))
            }
            _ => Either::Right(Either::Right(iter::empty())),
        }
    }

    fn move_calls(&self) -> Vec<(&ObjectID, &str, &str)> {
        match &self {
            Self::ProgrammableTransaction(pt) => pt.move_calls(),
            _ => vec![],
        }
    }

    pub fn receiving_objects(&self) -> Vec<ObjectRef> {
        match &self {
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_)
            | TransactionKind::ConsensusCommitPrologueV2(_)
            | TransactionKind::ConsensusCommitPrologueV3(_)
            | TransactionKind::AuthenticatorStateUpdate(_)
            | TransactionKind::RandomnessStateUpdate(_)
            | TransactionKind::EndOfEpochTransaction(_) => vec![],
            TransactionKind::ProgrammableTransaction(pt) => pt.receiving_objects(),
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
            Self::ConsensusCommitPrologue(_)
            | Self::ConsensusCommitPrologueV2(_)
            | Self::ConsensusCommitPrologueV3(_) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutable: true,
                }]
            }
            Self::AuthenticatorStateUpdate(update) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                    initial_shared_version: update.authenticator_obj_initial_shared_version(),
                    mutable: true,
                }]
            }
            Self::RandomnessStateUpdate(update) => {
                vec![InputObjectKind::SharedMoveObject {
                    id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    initial_shared_version: update.randomness_obj_initial_shared_version(),
                    mutable: true,
                }]
            }
            Self::EndOfEpochTransaction(txns) => {
                // Dedup since transactions may have a overlap in input objects.
                // Note: it's critical to ensure the order of inputs are deterministic.
                let before_dedup: Vec<_> =
                    txns.iter().flat_map(|txn| txn.input_objects()).collect();
                let mut has_seen = HashSet::new();
                let mut after_dedup = vec![];
                for obj in before_dedup {
                    if has_seen.insert(obj) {
                        after_dedup.push(obj);
                    }
                }
                after_dedup
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
            // All transactiond kinds below are assumed to be system,
            // and no validity or limit checks are performed.
            TransactionKind::ChangeEpoch(_)
            | TransactionKind::Genesis(_)
            | TransactionKind::ConsensusCommitPrologue(_) => (),
            TransactionKind::ConsensusCommitPrologueV2(_) => {
                if !config.include_consensus_digest_in_prologue() {
                    return Err(UserInputError::Unsupported(
                        "ConsensusCommitPrologueV2 is not supported".to_string(),
                    ));
                }
            }
            TransactionKind::ConsensusCommitPrologueV3(_) => {
                if !config.record_consensus_determined_version_assignments_in_prologue() {
                    return Err(UserInputError::Unsupported(
                        "ConsensusCommitPrologueV3 is not supported".to_string(),
                    ));
                }
            }
            TransactionKind::EndOfEpochTransaction(txns) => {
                if !config.end_of_epoch_transaction_supported() {
                    return Err(UserInputError::Unsupported(
                        "EndOfEpochTransaction is not supported".to_string(),
                    ));
                }

                for tx in txns {
                    tx.validity_check(config)?;
                }
            }

            TransactionKind::AuthenticatorStateUpdate(_) => {
                if !config.enable_jwk_consensus_updates() {
                    return Err(UserInputError::Unsupported(
                        "authenticator state updates not enabled".to_string(),
                    ));
                }
            }
            TransactionKind::RandomnessStateUpdate(_) => {
                if !config.random_beacon() {
                    return Err(UserInputError::Unsupported(
                        "randomness state updates not enabled".to_string(),
                    ));
                }
            }
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

    /// number of transactions, or 1 if it is a system transaction
    pub fn tx_count(&self) -> usize {
        match self {
            TransactionKind::ProgrammableTransaction(pt) => pt.commands.len(),
            _ => 1,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::ChangeEpoch(_) => "ChangeEpoch",
            Self::Genesis(_) => "Genesis",
            Self::ConsensusCommitPrologue(_) => "ConsensusCommitPrologue",
            Self::ConsensusCommitPrologueV2(_) => "ConsensusCommitPrologueV2",
            Self::ConsensusCommitPrologueV3(_) => "ConsensusCommitPrologueV3",
            Self::ProgrammableTransaction(_) => "ProgrammableTransaction",
            Self::AuthenticatorStateUpdate(_) => "AuthenticatorStateUpdate",
            Self::RandomnessStateUpdate(_) => "RandomnessStateUpdate",
            Self::EndOfEpochTransaction(_) => "EndOfEpochTransaction",
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
            Self::ConsensusCommitPrologueV2(p) => {
                writeln!(writer, "Transaction Kind : Consensus Commit Prologue V2")?;
                writeln!(writer, "Timestamp : {}", p.commit_timestamp_ms)?;
                writeln!(writer, "Consensus Digest: {}", p.consensus_commit_digest)?;
            }
            Self::ConsensusCommitPrologueV3(p) => {
                writeln!(writer, "Transaction Kind : Consensus Commit Prologue V3")?;
                writeln!(writer, "Timestamp : {}", p.commit_timestamp_ms)?;
                writeln!(writer, "Consensus Digest: {}", p.consensus_commit_digest)?;
                writeln!(
                    writer,
                    "Consensus determined version assignment: {:?}",
                    p.consensus_determined_version_assignments
                )?;
            }
            Self::ProgrammableTransaction(p) => {
                writeln!(writer, "Transaction Kind : Programmable")?;
                write!(writer, "{p}")?;
            }
            Self::AuthenticatorStateUpdate(_) => {
                writeln!(writer, "Transaction Kind : Authenticator State Update")?;
            }
            Self::RandomnessStateUpdate(_) => {
                writeln!(writer, "Transaction Kind : Randomness State Update")?;
            }
            Self::EndOfEpochTransaction(_) => {
                writeln!(writer, "Transaction Kind : End of Epoch Transaction")?;
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
    // When new variants are introduced, it is important that we check version support
    // in the validity_check function based on the protocol config.
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
                }
                | Owner::ConsensusV2 {
                    start_version: initial_shared_version,
                    authenticator: _,
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
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("package").to_owned(),
                ident_str!("authorize_upgrade").to_owned(),
                vec![],
                vec![Argument::Input(0), upgrade_arg, digest_arg],
            );
            let upgrade_receipt = builder.upgrade(package_id, upgrade_ticket, dep_ids, modules);

            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
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

    pub fn message_version(&self) -> u64 {
        match self {
            TransactionData::V1(_) => 1,
        }
    }

    pub fn execution_parts(&self) -> (TransactionKind, SuiAddress, Vec<ObjectRef>) {
        (
            self.kind().clone(),
            self.sender(),
            self.gas_data().payment.clone(),
        )
    }

    pub fn uses_randomness(&self) -> bool {
        self.shared_input_objects()
            .iter()
            .any(|obj| obj.id() == SUI_RANDOMNESS_STATE_OBJECT_ID)
    }

    pub fn digest(&self) -> TransactionDigest {
        TransactionDigest::new(default_hash(self))
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
    fn signers(&self) -> NonEmpty<SuiAddress>;

    fn gas_data(&self) -> &GasData;

    fn gas_owner(&self) -> SuiAddress;

    fn gas(&self) -> &[ObjectRef];

    fn gas_price(&self) -> u64;

    fn gas_budget(&self) -> u64;

    fn expiration(&self) -> &TransactionExpiration;

    fn contains_shared_object(&self) -> bool;

    fn shared_input_objects(&self) -> Vec<SharedInputObject>;

    fn move_calls(&self) -> Vec<(&ObjectID, &str, &str)>;

    fn input_objects(&self) -> UserInputResult<Vec<InputObjectKind>>;

    fn receiving_objects(&self) -> Vec<ObjectRef>;

    fn validity_check(&self, config: &ProtocolConfig) -> UserInputResult;

    fn validity_check_no_gas_check(&self, config: &ProtocolConfig) -> UserInputResult;

    /// Check if the transaction is compliant with sponsorship.
    fn check_sponsorship(&self) -> UserInputResult;

    fn is_system_tx(&self) -> bool;
    fn is_genesis_tx(&self) -> bool;

    /// returns true if the transaction is one that is specially sequenced to run at the very end
    /// of the epoch
    fn is_end_of_epoch_tx(&self) -> bool;

    /// Check if the transaction is sponsored (namely gas owner != sender)
    fn is_sponsored_tx(&self) -> bool;

    fn sender_mut_for_testing(&mut self) -> &mut SuiAddress;

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
    fn signers(&self) -> NonEmpty<SuiAddress> {
        let mut signers = nonempty![self.sender];
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

    fn move_calls(&self) -> Vec<(&ObjectID, &str, &str)> {
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

    fn receiving_objects(&self) -> Vec<ObjectRef> {
        self.kind.receiving_objects()
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
        if matches!(&self.kind, TransactionKind::ProgrammableTransaction(_)) {
            return Ok(());
        }
        Err(UserInputError::UnsupportedSponsoredTransactionKind)
    }

    fn is_end_of_epoch_tx(&self) -> bool {
        matches!(
            self.kind,
            TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
        )
    }

    fn is_system_tx(&self) -> bool {
        self.kind.is_system_tx()
    }

    fn is_genesis_tx(&self) -> bool {
        matches!(self.kind, TransactionKind::Genesis(_))
    }

    fn sender_mut_for_testing(&mut self) -> &mut SuiAddress {
        &mut self.sender
    }

    fn gas_data_mut(&mut self) -> &mut GasData {
        &mut self.gas_data
    }

    fn expiration_mut_for_testing(&mut self) -> &mut TransactionExpiration {
        &mut self.expiration
    }
}

impl TransactionDataV1 {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SenderSignedData(SizeOneVec<SenderSignedTransaction>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SenderSignedTransaction {
    pub intent_message: IntentMessage<TransactionData>,
    /// A list of signatures signed by all transaction participants.
    /// 1. non participant signature must not be present.
    /// 2. signature order does not matter.
    pub tx_signatures: Vec<GenericSignature>,
}

impl Serialize for SenderSignedTransaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename = "SenderSignedTransaction")]
        struct SignedTxn<'a> {
            intent_message: &'a IntentMessage<TransactionData>,
            tx_signatures: &'a Vec<GenericSignature>,
        }

        if self.intent_message().intent != Intent::sui_transaction() {
            return Err(serde::ser::Error::custom("invalid Intent for Transaction"));
        }

        let txn = SignedTxn {
            intent_message: self.intent_message(),
            tx_signatures: &self.tx_signatures,
        };
        txn.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SenderSignedTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename = "SenderSignedTransaction")]
        struct SignedTxn {
            intent_message: IntentMessage<TransactionData>,
            tx_signatures: Vec<GenericSignature>,
        }

        let SignedTxn {
            intent_message,
            tx_signatures,
        } = Deserialize::deserialize(deserializer)?;

        if intent_message.intent != Intent::sui_transaction() {
            return Err(serde::de::Error::custom("invalid Intent for Transaction"));
        }

        Ok(Self {
            intent_message,
            tx_signatures,
        })
    }
}

impl SenderSignedTransaction {
    pub(crate) fn get_signer_sig_mapping(
        &self,
        verify_legacy_zklogin_address: bool,
    ) -> SuiResult<BTreeMap<SuiAddress, &GenericSignature>> {
        let mut mapping = BTreeMap::new();
        for sig in &self.tx_signatures {
            if verify_legacy_zklogin_address {
                // Try deriving the address from the legacy padded way.
                if let GenericSignature::ZkLoginAuthenticator(z) = sig {
                    mapping.insert(SuiAddress::try_from_padded(&z.inputs)?, sig);
                };
            }
            let address = sig.try_into()?;
            mapping.insert(address, sig);
        }
        Ok(mapping)
    }

    pub fn intent_message(&self) -> &IntentMessage<TransactionData> {
        &self.intent_message
    }
}

impl SenderSignedData {
    pub fn new(tx_data: TransactionData, tx_signatures: Vec<GenericSignature>) -> Self {
        Self(SizeOneVec::new(SenderSignedTransaction {
            intent_message: IntentMessage::new(Intent::sui_transaction(), tx_data),
            tx_signatures,
        }))
    }

    pub fn new_from_sender_signature(tx_data: TransactionData, tx_signature: Signature) -> Self {
        Self(SizeOneVec::new(SenderSignedTransaction {
            intent_message: IntentMessage::new(Intent::sui_transaction(), tx_data),
            tx_signatures: vec![tx_signature.into()],
        }))
    }

    pub fn inner(&self) -> &SenderSignedTransaction {
        self.0.element()
    }

    pub fn into_inner(self) -> SenderSignedTransaction {
        self.0.into_inner()
    }

    pub fn inner_mut(&mut self) -> &mut SenderSignedTransaction {
        self.0.element_mut()
    }

    // This function does not check validity of the signature
    // or perform any de-dup checks.
    pub fn add_signature(&mut self, new_signature: Signature) {
        self.inner_mut().tx_signatures.push(new_signature.into());
    }

    pub(crate) fn get_signer_sig_mapping(
        &self,
        verify_legacy_zklogin_address: bool,
    ) -> SuiResult<BTreeMap<SuiAddress, &GenericSignature>> {
        self.inner()
            .get_signer_sig_mapping(verify_legacy_zklogin_address)
    }

    pub fn transaction_data(&self) -> &TransactionData {
        &self.intent_message().value
    }

    pub fn intent_message(&self) -> &IntentMessage<TransactionData> {
        self.inner().intent_message()
    }

    pub fn tx_signatures(&self) -> &[GenericSignature] {
        &self.inner().tx_signatures
    }

    pub fn has_zklogin_sig(&self) -> bool {
        self.tx_signatures().iter().any(|sig| sig.is_zklogin())
    }

    pub fn has_upgraded_multisig(&self) -> bool {
        self.tx_signatures()
            .iter()
            .any(|sig| sig.is_upgraded_multisig())
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

    pub fn serialized_size(&self) -> SuiResult<usize> {
        bcs::serialized_size(self).map_err(|e| SuiError::TransactionSerializationError {
            error: e.to_string(),
        })
    }

    fn check_user_signature_protocol_compatibility(&self, config: &ProtocolConfig) -> SuiResult {
        for sig in &self.inner().tx_signatures {
            match sig {
                GenericSignature::MultiSig(_) => {
                    if !config.supports_upgraded_multisig() {
                        return Err(SuiError::UserInputError {
                            error: UserInputError::Unsupported(
                                "upgraded multisig format not enabled on this network".to_string(),
                            ),
                        });
                    }
                }
                GenericSignature::ZkLoginAuthenticator(_) => {
                    if !config.zklogin_auth() {
                        return Err(SuiError::UserInputError {
                            error: UserInputError::Unsupported(
                                "zklogin is not enabled on this network".to_string(),
                            ),
                        });
                    }
                }
                GenericSignature::PasskeyAuthenticator(_) => {
                    if !config.passkey_auth() {
                        return Err(SuiError::UserInputError {
                            error: UserInputError::Unsupported(
                                "passkey is not enabled on this network".to_string(),
                            ),
                        });
                    }
                }
                GenericSignature::Signature(_) | GenericSignature::MultiSigLegacy(_) => (),
            }
        }

        Ok(())
    }

    /// Validate untrusted user transaction, including its size, input count, command count, etc.
    /// Returns the certificate serialised bytes size.
    pub fn validity_check(
        &self,
        config: &ProtocolConfig,
        epoch: EpochId,
    ) -> Result<usize, SuiError> {
        // Check that the features used by the user signatures are enabled on the network.
        self.check_user_signature_protocol_compatibility(config)?;

        // CRITICAL!!
        // Users cannot send system transactions.
        let tx_data = &self.transaction_data();
        fp_ensure!(
            !tx_data.is_system_tx(),
            SuiError::UserInputError {
                error: UserInputError::Unsupported(
                    "SenderSignedData must not contain system transaction".to_string()
                )
            }
        );

        // Checks to see if the transaction has expired
        if match &tx_data.expiration() {
            TransactionExpiration::None => false,
            TransactionExpiration::Epoch(exp_poch) => *exp_poch < epoch,
        } {
            return Err(SuiError::TransactionExpired);
        }

        // Enforce overall transaction size limit.
        let tx_size = self.serialized_size()?;
        let max_tx_size_bytes = config.max_tx_size_bytes();
        fp_ensure!(
            tx_size as u64 <= max_tx_size_bytes,
            SuiError::UserInputError {
                error: UserInputError::SizeLimitExceeded {
                    limit: format!(
                        "serialized transaction size exceeded maximum of {max_tx_size_bytes}"
                    ),
                    value: tx_size.to_string(),
                }
            }
        );

        tx_data
            .validity_check(config)
            .map_err(Into::<SuiError>::into)?;

        Ok(tx_size)
    }
}

impl Message for SenderSignedData {
    type DigestType = TransactionDigest;
    const SCOPE: IntentScope = IntentScope::SenderSignedTransaction;

    /// Computes the tx digest that encodes the Rust type prefix from Signable trait.
    fn digest(&self) -> Self::DigestType {
        self.intent_message().value.digest()
    }
}

impl<S> Envelope<SenderSignedData, S> {
    pub fn sender_address(&self) -> SuiAddress {
        self.data().intent_message().value.sender()
    }

    pub fn gas_owner(&self) -> SuiAddress {
        self.data().intent_message().value.gas_owner()
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

    // Returns the primary key for this transaction.
    pub fn key(&self) -> TransactionKey {
        match &self.data().intent_message().value.kind() {
            TransactionKind::RandomnessStateUpdate(rsu) => {
                TransactionKey::RandomnessRound(rsu.epoch, rsu.randomness_round)
            }
            _ => TransactionKey::Digest(*self.digest()),
        }
    }

    // Returns non-Digest keys that could be used to refer to this transaction.
    //
    // At the moment this returns a single Option for efficiency, but if more key types are added,
    // the return type could change to Vec<TransactionKey>.
    pub fn non_digest_key(&self) -> Option<TransactionKey> {
        match &self.data().intent_message().value.kind() {
            TransactionKind::RandomnessStateUpdate(rsu) => Some(TransactionKey::RandomnessRound(
                rsu.epoch,
                rsu.randomness_round,
            )),
            _ => None,
        }
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
        signers: Vec<&dyn Signer<Signature>>,
    ) -> Self {
        let signatures = {
            let intent_msg = IntentMessage::new(Intent::sui_transaction(), &data);
            signers
                .into_iter()
                .map(|s| Signature::new_secure(&intent_msg, s))
                .collect()
        };
        Self::from_data(data, signatures)
    }

    // TODO: Rename this function and above to make it clearer.
    pub fn from_data(data: TransactionData, signatures: Vec<Signature>) -> Self {
        Self::from_generic_sig_data(data, signatures.into_iter().map(|s| s.into()).collect())
    }

    pub fn signature_from_signer(
        data: TransactionData,
        intent: Intent,
        signer: &dyn Signer<Signature>,
    ) -> Signature {
        let intent_msg = IntentMessage::new(intent, data);
        Signature::new_secure(&intent_msg, signer)
    }

    pub fn from_generic_sig_data(data: TransactionData, signatures: Vec<GenericSignature>) -> Self {
        Self::new(SenderSignedData::new(data, signatures))
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

    pub fn new_consensus_commit_prologue_v2(
        epoch: u64,
        round: u64,
        commit_timestamp_ms: CheckpointTimestamp,
        consensus_commit_digest: ConsensusCommitDigest,
    ) -> Self {
        ConsensusCommitPrologueV2 {
            epoch,
            round,
            commit_timestamp_ms,
            consensus_commit_digest,
        }
        .pipe(TransactionKind::ConsensusCommitPrologueV2)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_consensus_commit_prologue_v3(
        epoch: u64,
        round: u64,
        commit_timestamp_ms: CheckpointTimestamp,
        consensus_commit_digest: ConsensusCommitDigest,
        consensus_determined_version_assignments: ConsensusDeterminedVersionAssignments,
    ) -> Self {
        ConsensusCommitPrologueV3 {
            epoch,
            round,
            // sub_dag_index is reserved for when we have multi commits per round.
            sub_dag_index: None,
            commit_timestamp_ms,
            consensus_commit_digest,
            consensus_determined_version_assignments,
        }
        .pipe(TransactionKind::ConsensusCommitPrologueV3)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_authenticator_state_update(
        epoch: u64,
        round: u64,
        new_active_jwks: Vec<ActiveJwk>,
        authenticator_obj_initial_shared_version: SequenceNumber,
    ) -> Self {
        AuthenticatorStateUpdate {
            epoch,
            round,
            new_active_jwks,
            authenticator_obj_initial_shared_version,
        }
        .pipe(TransactionKind::AuthenticatorStateUpdate)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_randomness_state_update(
        epoch: u64,
        randomness_round: RandomnessRound,
        random_bytes: Vec<u8>,
        randomness_obj_initial_shared_version: SequenceNumber,
    ) -> Self {
        RandomnessStateUpdate {
            epoch,
            randomness_round,
            random_bytes,
            randomness_obj_initial_shared_version,
        }
        .pipe(TransactionKind::RandomnessStateUpdate)
        .pipe(Self::new_system_transaction)
    }

    pub fn new_end_of_epoch_transaction(txns: Vec<EndOfEpochTransactionKind>) -> Self {
        TransactionKind::EndOfEpochTransaction(txns).pipe(Self::new_system_transaction)
    }

    fn new_system_transaction(system_transaction: TransactionKind) -> Self {
        system_transaction
            .pipe(TransactionData::new_system_transaction)
            .pipe(|data| {
                SenderSignedData::new_from_sender_signature(
                    data,
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

impl Transaction {
    pub fn verify_signature_for_testing(
        &self,
        current_epoch: EpochId,
        verify_params: &VerifyParams,
    ) -> SuiResult {
        verify_sender_signed_data_message_signatures(
            self.data(),
            current_epoch,
            verify_params,
            Arc::new(VerifiedDigestCache::new_empty()),
        )
    }

    pub fn try_into_verified_for_testing(
        self,
        current_epoch: EpochId,
        verify_params: &VerifyParams,
    ) -> SuiResult<VerifiedTransaction> {
        self.verify_signature_for_testing(current_epoch, verify_params)?;
        Ok(VerifiedTransaction::new_from_verified(self))
    }
}

impl SignedTransaction {
    pub fn verify_signatures_authenticated_for_testing(
        &self,
        committee: &Committee,
        verify_params: &VerifyParams,
    ) -> SuiResult {
        verify_sender_signed_data_message_signatures(
            self.data(),
            committee.epoch(),
            verify_params,
            Arc::new(VerifiedDigestCache::new_empty()),
        )?;

        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::SenderSignedTransaction),
            committee,
        )
    }

    pub fn try_into_verified_for_testing(
        self,
        committee: &Committee,
        verify_params: &VerifyParams,
    ) -> SuiResult<VerifiedSignedTransaction> {
        self.verify_signatures_authenticated_for_testing(committee, verify_params)?;
        Ok(VerifiedSignedTransaction::new_from_verified(self))
    }
}

pub type CertifiedTransaction = Envelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;

impl CertifiedTransaction {
    pub fn certificate_digest(&self) -> CertificateDigest {
        let mut digest = DefaultHash::default();
        bcs::serialize_into(&mut digest, self).expect("serialization should not fail");
        let hash = digest.finalize();
        CertificateDigest::new(hash.into())
    }

    pub fn gas_price(&self) -> u64 {
        self.data().transaction_data().gas_price()
    }

    // TODO: Eventually we should remove all calls to verify_signature
    // and make sure they all call verify to avoid repeated verifications.
    pub fn verify_signatures_authenticated(
        &self,
        committee: &Committee,
        verify_params: &VerifyParams,
        zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> SuiResult {
        verify_sender_signed_data_message_signatures(
            self.data(),
            committee.epoch(),
            verify_params,
            zklogin_inputs_cache,
        )?;
        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::SenderSignedTransaction),
            committee,
        )
    }

    pub fn try_into_verified_for_testing(
        self,
        committee: &Committee,
        verify_params: &VerifyParams,
    ) -> SuiResult<VerifiedCertificate> {
        self.verify_signatures_authenticated(
            committee,
            verify_params,
            Arc::new(VerifiedDigestCache::new_empty()),
        )?;
        Ok(VerifiedCertificate::new_from_verified(self))
    }

    pub fn verify_committee_sigs_only(&self, committee: &Committee) -> SuiResult {
        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::SenderSignedTransaction),
            committee,
        )
    }
}

pub type VerifiedCertificate = VerifiedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;
pub type TrustedCertificate = TrustedEnvelope<SenderSignedData, AuthorityStrongQuorumSignInfo>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
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

    pub fn is_mutable(&self) -> bool {
        match self {
            Self::MovePackage(..) => false,
            Self::ImmOrOwnedMoveObject((_, _, _)) => true,
            Self::SharedMoveObject { mutable, .. } => *mutable,
        }
    }
}

/// The result of reading an object for execution. Because shared objects may be deleted, one
/// possible result of reading a shared object is that ObjectReadResultKind::Deleted is returned.
#[derive(Clone, Debug)]
pub struct ObjectReadResult {
    pub input_object_kind: InputObjectKind,
    pub object: ObjectReadResultKind,
}

#[derive(Clone)]
pub enum ObjectReadResultKind {
    Object(Object),
    // The version of the object that the transaction intended to read, and the digest of the tx
    // that deleted it.
    DeletedSharedObject(SequenceNumber, TransactionDigest),
    // A shared object in a cancelled transaction. The sequence number embeds cancellation reason.
    CancelledTransactionSharedObject(SequenceNumber),
}

impl std::fmt::Debug for ObjectReadResultKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectReadResultKind::Object(obj) => {
                write!(f, "Object({:?})", obj.compute_object_reference())
            }
            ObjectReadResultKind::DeletedSharedObject(seq, digest) => {
                write!(f, "DeletedSharedObject({}, {:?})", seq, digest)
            }
            ObjectReadResultKind::CancelledTransactionSharedObject(seq) => {
                write!(f, "CancelledTransactionSharedObject({})", seq)
            }
        }
    }
}

impl From<Object> for ObjectReadResultKind {
    fn from(object: Object) -> Self {
        Self::Object(object)
    }
}

impl ObjectReadResult {
    pub fn new(input_object_kind: InputObjectKind, object: ObjectReadResultKind) -> Self {
        if let (
            InputObjectKind::ImmOrOwnedMoveObject(_),
            ObjectReadResultKind::DeletedSharedObject(_, _),
        ) = (&input_object_kind, &object)
        {
            panic!("only shared objects can be DeletedSharedObject");
        }

        if let (
            InputObjectKind::ImmOrOwnedMoveObject(_),
            ObjectReadResultKind::CancelledTransactionSharedObject(_),
        ) = (&input_object_kind, &object)
        {
            panic!("only shared objects can be CancelledTransactionSharedObject");
        }

        Self {
            input_object_kind,
            object,
        }
    }

    pub fn id(&self) -> ObjectID {
        self.input_object_kind.object_id()
    }

    pub fn as_object(&self) -> Option<&Object> {
        match &self.object {
            ObjectReadResultKind::Object(object) => Some(object),
            ObjectReadResultKind::DeletedSharedObject(_, _) => None,
            ObjectReadResultKind::CancelledTransactionSharedObject(_) => None,
        }
    }

    pub fn new_from_gas_object(gas: &Object) -> Self {
        let objref = gas.compute_object_reference();
        Self {
            input_object_kind: InputObjectKind::ImmOrOwnedMoveObject(objref),
            object: ObjectReadResultKind::Object(gas.clone()),
        }
    }

    pub fn is_mutable(&self) -> bool {
        match (&self.input_object_kind, &self.object) {
            (InputObjectKind::MovePackage(_), _) => false,
            (InputObjectKind::ImmOrOwnedMoveObject(_), ObjectReadResultKind::Object(object)) => {
                !object.is_immutable()
            }
            (
                InputObjectKind::ImmOrOwnedMoveObject(_),
                ObjectReadResultKind::DeletedSharedObject(_, _),
            ) => unreachable!(),
            (
                InputObjectKind::ImmOrOwnedMoveObject(_),
                ObjectReadResultKind::CancelledTransactionSharedObject(_),
            ) => unreachable!(),
            (InputObjectKind::SharedMoveObject { mutable, .. }, _) => *mutable,
        }
    }

    pub fn is_shared_object(&self) -> bool {
        self.input_object_kind.is_shared_object()
    }

    pub fn is_deleted_shared_object(&self) -> bool {
        self.deletion_info().is_some()
    }

    pub fn deletion_info(&self) -> Option<(SequenceNumber, TransactionDigest)> {
        match &self.object {
            ObjectReadResultKind::DeletedSharedObject(v, tx) => Some((*v, *tx)),
            _ => None,
        }
    }

    /// Return the object ref iff the object is an owned object (i.e. not shared, not immutable).
    pub fn get_owned_objref(&self) -> Option<ObjectRef> {
        match (&self.input_object_kind, &self.object) {
            (InputObjectKind::MovePackage(_), _) => None,
            (
                InputObjectKind::ImmOrOwnedMoveObject(objref),
                ObjectReadResultKind::Object(object),
            ) => {
                if object.is_immutable() {
                    None
                } else {
                    Some(*objref)
                }
            }
            (
                InputObjectKind::ImmOrOwnedMoveObject(_),
                ObjectReadResultKind::DeletedSharedObject(_, _),
            ) => unreachable!(),
            (
                InputObjectKind::ImmOrOwnedMoveObject(_),
                ObjectReadResultKind::CancelledTransactionSharedObject(_),
            ) => unreachable!(),
            (InputObjectKind::SharedMoveObject { .. }, _) => None,
        }
    }

    pub fn is_owned(&self) -> bool {
        self.get_owned_objref().is_some()
    }

    pub fn to_shared_input(&self) -> Option<SharedInput> {
        match self.input_object_kind {
            InputObjectKind::MovePackage(_) => None,
            InputObjectKind::ImmOrOwnedMoveObject(_) => None,
            InputObjectKind::SharedMoveObject { id, mutable, .. } => Some(match &self.object {
                ObjectReadResultKind::Object(obj) => {
                    SharedInput::Existing(obj.compute_object_reference())
                }
                ObjectReadResultKind::DeletedSharedObject(seq, digest) => {
                    SharedInput::Deleted((id, *seq, mutable, *digest))
                }
                ObjectReadResultKind::CancelledTransactionSharedObject(seq) => {
                    SharedInput::Cancelled((id, *seq))
                }
            }),
        }
    }

    pub fn get_previous_transaction(&self) -> Option<TransactionDigest> {
        match &self.object {
            ObjectReadResultKind::Object(obj) => Some(obj.previous_transaction),
            ObjectReadResultKind::DeletedSharedObject(_, digest) => Some(*digest),
            ObjectReadResultKind::CancelledTransactionSharedObject(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct InputObjects {
    objects: Vec<ObjectReadResult>,
}

impl std::fmt::Debug for InputObjects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.objects.iter()).finish()
    }
}

// An InputObjects new-type that has been verified by sui-transaction-checks, and can be
// safely passed to execution.
pub struct CheckedInputObjects(InputObjects);

// DO NOT CALL outside of sui-transaction-checks, genesis, or replay.
//
// CheckedInputObjects should really be defined in sui-transaction-checks so that we can
// make public construction impossible. But we can't do that because it would result in circular
// dependencies.
impl CheckedInputObjects {
    // Only called by sui-transaction-checks.
    pub fn new_with_checked_transaction_inputs(inputs: InputObjects) -> Self {
        Self(inputs)
    }

    // Only called when building the genesis transaction
    pub fn new_for_genesis(input_objects: Vec<ObjectReadResult>) -> Self {
        Self(InputObjects::new(input_objects))
    }

    // Only called from the replay tool.
    pub fn new_for_replay(input_objects: InputObjects) -> Self {
        Self(input_objects)
    }

    pub fn inner(&self) -> &InputObjects {
        &self.0
    }

    pub fn into_inner(self) -> InputObjects {
        self.0
    }
}

impl From<Vec<ObjectReadResult>> for InputObjects {
    fn from(objects: Vec<ObjectReadResult>) -> Self {
        Self::new(objects)
    }
}

impl InputObjects {
    pub fn new(objects: Vec<ObjectReadResult>) -> Self {
        Self { objects }
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn contains_deleted_objects(&self) -> bool {
        self.objects
            .iter()
            .any(|obj| obj.is_deleted_shared_object())
    }

    // Returns IDs of objects responsible for a tranaction being cancelled, and the corresponding
    // reason for cancellation.
    pub fn get_cancelled_objects(&self) -> Option<(Vec<ObjectID>, SequenceNumber)> {
        let mut contains_cancelled = false;
        let mut cancel_reason = None;
        let mut cancelled_objects = Vec::new();
        for obj in &self.objects {
            if let ObjectReadResultKind::CancelledTransactionSharedObject(version) = obj.object {
                contains_cancelled = true;
                if version == SequenceNumber::CONGESTED
                    || version == SequenceNumber::RANDOMNESS_UNAVAILABLE
                {
                    // Verify we don't have multiple cancellation reasons.
                    assert!(cancel_reason.is_none() || cancel_reason == Some(version));
                    cancel_reason = Some(version);
                    cancelled_objects.push(obj.id());
                }
            }
        }

        if !cancelled_objects.is_empty() {
            Some((
                cancelled_objects,
                cancel_reason
                    .expect("there should be a cancel reason if there are cancelled objects"),
            ))
        } else {
            assert!(!contains_cancelled);
            None
        }
    }

    pub fn filter_owned_objects(&self) -> Vec<ObjectRef> {
        let owned_objects: Vec<_> = self
            .objects
            .iter()
            .filter_map(|obj| obj.get_owned_objref())
            .collect();

        trace!(
            num_mutable_objects = owned_objects.len(),
            "Checked locks and found mutable objects"
        );

        owned_objects
    }

    pub fn filter_shared_objects(&self) -> Vec<SharedInput> {
        self.objects
            .iter()
            .filter(|obj| obj.is_shared_object())
            .map(|obj| {
                obj.to_shared_input()
                    .expect("already filtered for shared objects")
            })
            .collect()
    }

    pub fn transaction_dependencies(&self) -> BTreeSet<TransactionDigest> {
        self.objects
            .iter()
            .filter_map(|obj| obj.get_previous_transaction())
            .collect()
    }

    pub fn mutable_inputs(&self) -> BTreeMap<ObjectID, (VersionDigest, Owner)> {
        self.objects
            .iter()
            .filter_map(
                |ObjectReadResult {
                     input_object_kind,
                     object,
                 }| match (input_object_kind, object) {
                    (InputObjectKind::MovePackage(_), _) => None,
                    (
                        InputObjectKind::ImmOrOwnedMoveObject(object_ref),
                        ObjectReadResultKind::Object(object),
                    ) => {
                        if object.is_immutable() {
                            None
                        } else {
                            Some((
                                object_ref.0,
                                ((object_ref.1, object_ref.2), object.owner.clone()),
                            ))
                        }
                    }
                    (
                        InputObjectKind::ImmOrOwnedMoveObject(_),
                        ObjectReadResultKind::DeletedSharedObject(_, _),
                    ) => {
                        unreachable!()
                    }
                    (
                        InputObjectKind::SharedMoveObject { .. },
                        ObjectReadResultKind::DeletedSharedObject(_, _),
                    ) => None,
                    (
                        InputObjectKind::SharedMoveObject { mutable, .. },
                        ObjectReadResultKind::Object(object),
                    ) => {
                        if *mutable {
                            let oref = object.compute_object_reference();
                            Some((oref.0, ((oref.1, oref.2), object.owner.clone())))
                        } else {
                            None
                        }
                    }
                    (
                        InputObjectKind::ImmOrOwnedMoveObject(_),
                        ObjectReadResultKind::CancelledTransactionSharedObject(_),
                    ) => {
                        unreachable!()
                    }
                    (
                        InputObjectKind::SharedMoveObject { .. },
                        ObjectReadResultKind::CancelledTransactionSharedObject(_),
                    ) => None,
                },
            )
            .collect()
    }

    /// The version to set on objects created by the computation that `self` is input to.
    /// Guaranteed to be strictly greater than the versions of all input objects and objects
    /// received in the transaction.
    pub fn lamport_timestamp(&self, receiving_objects: &[ObjectRef]) -> SequenceNumber {
        let input_versions = self
            .objects
            .iter()
            .filter_map(|object| match &object.object {
                ObjectReadResultKind::Object(object) => {
                    object.data.try_as_move().map(MoveObject::version)
                }
                ObjectReadResultKind::DeletedSharedObject(v, _) => Some(*v),
                ObjectReadResultKind::CancelledTransactionSharedObject(_) => None,
            })
            .chain(receiving_objects.iter().map(|object_ref| object_ref.1));

        SequenceNumber::lamport_increment(input_versions)
    }

    pub fn object_kinds(&self) -> impl Iterator<Item = &InputObjectKind> {
        self.objects.iter().map(
            |ObjectReadResult {
                 input_object_kind, ..
             }| input_object_kind,
        )
    }

    pub fn deleted_consensus_objects(&self) -> BTreeMap<ObjectID, SequenceNumber> {
        self.objects
            .iter()
            .filter_map(|obj| {
                if let InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version,
                    ..
                } = obj.input_object_kind
                {
                    obj.is_deleted_shared_object()
                        .then_some((id, initial_shared_version))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn into_object_map(self) -> BTreeMap<ObjectID, Object> {
        self.objects
            .into_iter()
            .filter_map(|o| o.as_object().map(|object| (o.id(), object.clone())))
            .collect()
    }

    pub fn push(&mut self, object: ObjectReadResult) {
        self.objects.push(object);
    }

    pub fn iter(&self) -> impl Iterator<Item = &ObjectReadResult> {
        self.objects.iter()
    }

    pub fn iter_objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter_map(|o| o.as_object())
    }
}

// Result of attempting to read a receiving object (currently only at signing time).
// Because an object may have been previously received and deleted, the result may be
// ReceivingObjectReadResultKind::PreviouslyReceivedObject.
#[derive(Clone, Debug)]
pub enum ReceivingObjectReadResultKind {
    Object(Object),
    // The object was received by some other transaction, and we were not able to read it
    PreviouslyReceivedObject,
}

impl ReceivingObjectReadResultKind {
    pub fn as_object(&self) -> Option<&Object> {
        match &self {
            Self::Object(object) => Some(object),
            Self::PreviouslyReceivedObject => None,
        }
    }
}

pub struct ReceivingObjectReadResult {
    pub object_ref: ObjectRef,
    pub object: ReceivingObjectReadResultKind,
}

impl ReceivingObjectReadResult {
    pub fn new(object_ref: ObjectRef, object: ReceivingObjectReadResultKind) -> Self {
        Self { object_ref, object }
    }

    pub fn is_previously_received(&self) -> bool {
        matches!(
            self.object,
            ReceivingObjectReadResultKind::PreviouslyReceivedObject
        )
    }
}

impl From<Object> for ReceivingObjectReadResultKind {
    fn from(object: Object) -> Self {
        Self::Object(object)
    }
}

pub struct ReceivingObjects {
    pub objects: Vec<ReceivingObjectReadResult>,
}

impl ReceivingObjects {
    pub fn iter(&self) -> impl Iterator<Item = &ReceivingObjectReadResult> {
        self.objects.iter()
    }

    pub fn iter_objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter_map(|o| o.object.as_object())
    }
}

impl From<Vec<ReceivingObjectReadResult>> for ReceivingObjects {
    fn from(objects: Vec<ReceivingObjectReadResult>) -> Self {
        Self { objects }
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

/// TransactionKey uniquely identifies a transaction across all epochs.
/// Note that a single transaction may have multiple keys, for example a RandomnessStateUpdate
/// could be identified by both `Digest` and `RandomnessRound`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TransactionKey {
    Digest(TransactionDigest),
    RandomnessRound(EpochId, RandomnessRound),
}

impl TransactionKey {
    pub fn unwrap_digest(&self) -> &TransactionDigest {
        match self {
            TransactionKey::Digest(d) => d,
            _ => panic!("called expect_digest on a non-Digest TransactionKey: {self:?}"),
        }
    }
}

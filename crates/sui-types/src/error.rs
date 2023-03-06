// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::*,
    committee::{Committee, EpochId, StakeUnit},
    messages::{CommandIndex, ExecutionFailureStatus, MoveLocation},
    object::Owner,
};
use fastcrypto::error::FastCryptoError;
use move_binary_format::access::ModuleAccess;
use move_binary_format::{
    errors::{Location, PartialVMError, VMError},
    file_format::FunctionDefinitionIndex,
};
use move_core_types::{
    resolver::{ModuleResolver, ResourceResolver},
    vm_status::{StatusCode, StatusType},
};
pub use move_vm_runtime::move_vm::MoveVM;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Debug};
use strum_macros::{AsRefStr, IntoStaticStr};
use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
use thiserror::Error;
use tonic::Status;
use typed_store::rocks::TypedStoreError;

pub const TRANSACTION_NOT_FOUND_MSG_PREFIX: &str = "Could not find the referenced transaction";
pub const TRANSACTIONS_NOT_FOUND_MSG_PREFIX: &str = "Could not find the referenced transactions";

#[macro_export]
macro_rules! fp_bail {
    ($e:expr) => {
        return Err($e)
    };
}

#[macro_export(local_inner_macros)]
macro_rules! fp_ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            fp_bail!($e);
        }
    };
}
use crate::digests::TransactionEventsDigest;
pub(crate) use fp_ensure;

#[macro_export]
macro_rules! exit_main {
    ($result:expr) => {
        match $result {
            Ok(_) => (),
            Err(err) => {
                println!("{}", err.to_string().bold().red());
                std::process::exit(1);
            }
        }
    };
}

#[derive(
    Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr, IntoStaticStr,
)]
pub enum UserInputError {
    #[error("Mutable object {object_id} cannot appear more than one in one transaction.")]
    MutableObjectUsedMoreThanOnce { object_id: ObjectID },
    #[error("Wrong number of parameters for the transaction.")]
    ObjectInputArityViolation,
    #[error(
        "Could not find the referenced object {:?} at version {:?}.",
        object_id,
        version
    )]
    ObjectNotFound {
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    },
    #[error("Object {provided_obj_ref:?} is not available for consumption, its current version: {current_version:?}.")]
    ObjectVersionUnavailableForConsumption {
        provided_obj_ref: ObjectRef,
        current_version: SequenceNumber,
    },
    #[error("Dependent package not found on-chain: {package_id:?}")]
    DependentPackageNotFound { package_id: ObjectID },
    #[error("Mutable parameter provided, immutable parameter expected.")]
    ImmutableParameterExpectedError { object_id: ObjectID },
    #[error("Size limit exceeded: {limit} is {value}")]
    SizeLimitExceeded { limit: String, value: String },
    #[error(
        "Object {child_id:?} is owned by object {parent_id:?}. \
        Objects owned by other objects cannot be used as input arguments."
    )]
    InvalidChildObjectArgument {
        child_id: ObjectID,
        parent_id: ObjectID,
    },
    #[error(
        "Invalid Object digest for object {object_id:?}. Expected digest : {expected_digest:?}."
    )]
    InvalidObjectDigest {
        object_id: ObjectID,
        expected_digest: ObjectDigest,
    },
    #[error("Sequence numbers above the maximal value are not usable for transfers.")]
    InvalidSequenceNumber,
    #[error("A move object is expected, instead a move package is passed: {object_id}")]
    MovePackageAsObject { object_id: ObjectID },
    #[error("A move package is expected, instead a move object is passed: {object_id}")]
    MoveObjectAsPackage { object_id: ObjectID },
    #[error("Transaction was not signed by the correct sender: {}", error)]
    IncorrectUserSignature { error: String },

    #[error("Object used as shared is not shared.")]
    NotSharedObjectError,
    #[error("The transaction inputs contain duplicated ObjectRef's")]
    DuplicateObjectRefInput,

    // Gas related errors
    #[error("Transaction gas payment missing.")]
    MissingGasPayment,
    #[error("Gas object is not an owned object with owner: {:?}.", owner)]
    GasObjectNotOwnedObject { owner: Owner },
    #[error("Gas budget: {:?} is higher than max: {:?}.", gas_budget, max_budget)]
    GasBudgetTooHigh { gas_budget: u64, max_budget: u64 },
    #[error("Gas budget: {:?} is lower than min: {:?}.", gas_budget, min_budget)]
    GasBudgetTooLow { gas_budget: u64, min_budget: u64 },
    #[error(
        "Balance of gas object {:?} is lower than gas budget: {:?}.",
        gas_balance,
        gas_budget
    )]
    GasBalanceTooLowToCoverGasBudget { gas_balance: u128, gas_budget: u128 },
    #[error("Transaction kind does not support Sponsored Transaction")]
    UnsupportedSponsoredTransactionKind,
    #[error(
        "Gas price {:?} under reference gas price (RGP) {:?}",
        gas_price,
        reference_gas_price
    )]
    GasPriceUnderRGP {
        gas_price: u64,
        reference_gas_price: u64,
    },
    #[error("Object {object_id} is not a gas object")]
    InvalidGasObject { object_id: ObjectID },
    #[error("Gas object does not have enough balance to cover minimal gas spend")]
    InsufficientBalanceToCoverMinimalGas,

    #[error("Could not find the referenced object {:?} as the asked version {:?} is higher than the latest {:?}", object_id, asked_version, latest_version)]
    ObjectSequenceNumberTooHigh {
        object_id: ObjectID,
        asked_version: SequenceNumber,
        latest_version: SequenceNumber,
    },
    #[error("Object deleted at reference {:?}.", object_ref)]
    ObjectDeleted { object_ref: ObjectRef },
    #[error("Invalid Batch Transaction: {}", error)]
    InvalidBatchTransaction { error: String },
    #[error("This Move function is currently disabled and not available for call")]
    BlockedMoveFunction,
    #[error("Empty input coins for Pay related transaction")]
    EmptyInputCoins,

    #[error("SUI payment transactions use first input coin for gas payment, but found a different gas object.")]
    UnexpectedGasPaymentObject,

    #[error("Wrong initial version given for shared object")]
    SharedObjectStartingVersionMismatch,

    #[error("Attempt to transfer object {object_id} that does not have public transfer. Object transfer must be done instead using a distinct Move function call.")]
    TransferObjectWithoutPublicTransferError { object_id: ObjectID },

    #[error(
        "TransferObjects, MergeCoin, and Publish cannot have empty arguments. \
        If MakeMoveVec has empty arguments, it must have a type specified"
    )]
    EmptyCommandInput,

    #[error("Feature is not yet supported: {0}")]
    Unsupported(String),
}

/// Custom error type for Sui.
#[derive(
    Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr, IntoStaticStr,
)]
pub enum SuiError {
    #[error("Error checking transaction input objects: {:?}", error)]
    UserInputError { error: UserInputError },
    #[error("Expecting a single owner, shared ownership found")]
    UnexpectedOwnerType,

    #[error("Input {object_id} already has {queue_len} transactions pending, above threshold of {threshold}")]
    TooManyTransactionsPendingOnObject {
        object_id: ObjectID,
        queue_len: usize,
        threshold: usize,
    },

    // Signature verification
    #[error("Signature is not valid: {}", error)]
    InvalidSignature { error: String },
    #[error("Required Signature from {signer} is absent.")]
    SignerSignatureAbsent { signer: String },
    #[error("Expect {actual} signer signatures but got {expected}.")]
    SignerSignatureNumberMismatch { expected: usize, actual: usize },
    #[error("Value was not signed by the correct sender: {}", error)]
    IncorrectSigner { error: String },
    #[error("Value was not signed by a known authority. signer: {:?}, index: {:?}, committee: {committee}", signer, index)]
    UnknownSigner {
        signer: Option<String>,
        index: Option<u32>,
        committee: Box<Committee>,
    },
    #[error(
        "Validator {:?} responded multiple signatures for the same message, conflicting: {:?}",
        signer,
        conflicting_sig
    )]
    StakeAggregatorRepeatedSigner {
        signer: AuthorityName,
        conflicting_sig: bool,
    },

    // Certificate verification and execution
    #[error(
        "Signature or certificate from wrong epoch, expected {expected_epoch}, got {actual_epoch}"
    )]
    WrongEpoch {
        expected_epoch: EpochId,
        actual_epoch: EpochId,
    },
    #[error("Signatures in a certificate must form a quorum")]
    CertificateRequiresQuorum,
    #[error("Transaction certificate processing failed: {err}")]
    ErrorWhileProcessingCertificate { err: String },
    #[error(
        "Failed to get a quorum of signed effects when processing transaction: {effects_map:?}"
    )]
    QuorumFailedToGetEffectsQuorumWhenProcessingTransaction {
        effects_map: BTreeMap<TransactionEffectsDigest, (Vec<AuthorityName>, StakeUnit)>,
    },
    #[error("System Transaction not accepted")]
    InvalidSystemTransaction,

    // Account access
    #[error("Invalid authenticator")]
    InvalidAuthenticator,
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid transaction digest.")]
    InvalidTransactionDigest,

    #[error("Unexpected message.")]
    UnexpectedMessage,

    // Move module publishing related errors
    #[error("Failed to verify the Move module, reason: {error:?}.")]
    ModuleVerificationFailure { error: String },
    #[error("Failed to verify the Move module, reason: {error:?}.")]
    ModuleDeserializationFailure { error: String },
    #[error("Failed to publish the Move module(s), reason: {error:?}.")]
    ModulePublishFailure { error: String },
    #[error("Failed to build Move modules: {error}.")]
    ModuleBuildFailure { error: String },

    // Move call related errors
    #[error("Function resolution failure: {error:?}.")]
    FunctionNotFound { error: String },
    #[error("Module not found in package: {module_name:?}.")]
    ModuleNotFound { module_name: String },
    #[error("Type error while binding function arguments: {error:?}.")]
    TypeError { error: String },
    #[error("Circular object ownership detected")]
    CircularObjectOwnership,

    // Internal state errors
    #[error("Attempt to re-initialize a transaction lock for objects {:?}.", refs)]
    ObjectLockAlreadyInitialized { refs: Vec<ObjectRef> },
    #[error(
        "Object {obj_ref:?} already locked by a different transaction: {pending_transaction:?}"
    )]
    ObjectLockConflict {
        obj_ref: ObjectRef,
        pending_transaction: TransactionDigest,
    },
    #[error("Objects {obj_refs:?} are already locked by a transaction from a future epoch {locked_epoch:?}), attempt to override with a transaction from epoch {new_epoch:?}")]
    ObjectLockedAtFutureEpoch {
        obj_refs: Vec<ObjectRef>,
        locked_epoch: EpochId,
        new_epoch: EpochId,
        locked_by_tx: TransactionDigest,
    },
    #[error("{TRANSACTION_NOT_FOUND_MSG_PREFIX} [{:?}].", digest)]
    TransactionNotFound { digest: TransactionDigest },
    #[error("{TRANSACTIONS_NOT_FOUND_MSG_PREFIX} [{:?}].", digests)]
    TransactionsNotFound { digests: Vec<TransactionDigest> },
    #[error("Could not find the referenced transaction events [{digest:?}].")]
    TransactionEventsNotFound { digest: TransactionEventsDigest },
    #[error(
        "Attempt to move to `Executed` state an transaction that has already been executed: {:?}.",
        digest
    )]
    TransactionAlreadyExecuted { digest: TransactionDigest },
    #[error("Object ID did not have the expected type")]
    BadObjectType { error: String },

    #[error("Execution invariant violated")]
    ExecutionInvariantViolation,
    #[error("Validator {authority:?} is faulty in a Byzantine manner: {reason:?}")]
    ByzantineAuthoritySuspicion {
        authority: AuthorityName,
        reason: String,
    },
    #[error("Storage error")]
    StorageError(#[from] TypedStoreError),
    #[error("Non-RocksDB Storage error: {0}")]
    GenericStorageError(String),
    #[error(
        "Attempted to access {object} through parent {given_parent}, \
        but it's actual parent is {actual_owner}"
    )]
    InvalidChildObjectAccess {
        object: ObjectID,
        given_parent: ObjectID,
        actual_owner: Owner,
    },

    #[error("Missing fields/data in storage error: {0}")]
    StorageMissingFieldError(String),
    #[error("Corrupted fields/data in storage error: {0}")]
    StorageCorruptedFieldError(String),

    #[error("Authority Error: {error:?}")]
    GenericAuthorityError { error: String },

    #[error("Failed to dispatch event: {error:?}")]
    EventFailedToDispatch { error: String },

    #[error("Failed to serialize Owner: {error:?}")]
    OwnerFailedToSerialize { error: String },

    #[error("Failed to deserialize fields into JSON: {error:?}")]
    ExtraFieldFailedToDeserialize { error: String },

    #[error("Failed to execute transaction locally by Orchestrator: {error:?}")]
    TransactionOrchestratorLocalExecutionError { error: String },

    // Errors returned by authority and client read API's
    #[error("Failure serializing object in the requested format: {:?}", error)]
    ObjectSerializationError { error: String },
    #[error("Failure deserializing object in the requested format: {:?}", error)]
    ObjectDeserializationError { error: String },
    #[error("Event store component is not active on this node")]
    NoEventStore,

    // Client side error
    #[error("Too many authority errors were detected for {}: {:?}", action, errors)]
    TooManyIncorrectAuthorities {
        errors: Vec<(AuthorityName, SuiError)>,
        action: String,
    },
    #[error("Invalid transaction range query to the fullnode: {:?}", error)]
    FullNodeInvalidTxRangeQuery { error: String },

    // Errors related to the authority-consensus interface.
    #[error("Failed to connect with consensus node: {0}")]
    ConsensusConnectionBroken(String),
    #[error("Failed to hear back from consensus: {0}")]
    FailedToHearBackFromConsensus(String),
    #[error("Failed to execute handle_consensus_transaction on Sui: {0}")]
    HandleConsensusTransactionFailure(String),

    // Cryptography errors.
    #[error("Signature seed invalid length, input byte size was: {0}")]
    SignatureSeedInvalidLength(usize),
    #[error("HKDF error: {0}")]
    HkdfError(String),
    #[error("Signature key generation error: {0}")]
    SignatureKeyGenError(String),
    #[error("Key Conversion Error: {0}")]
    KeyConversionError(String),
    #[error("Invalid Private Key provided")]
    InvalidPrivateKey,

    // Epoch related errors.
    #[error("Validator temporarily stopped processing transactions due to epoch change")]
    ValidatorHaltedAtEpochEnd,
    #[error("Error when advancing epoch: {:?}", error)]
    AdvanceEpochError { error: String },

    #[error("Transaction Expired")]
    TransactionExpired,

    // These are errors that occur when an RPC fails and is simply the utf8 message sent in a
    // Tonic::Status
    #[error("{1} - {0}")]
    RpcError(String, String),

    #[error("Use of disabled feature: {:?}", error)]
    UnsupportedFeatureError { error: String },

    #[error("Unable to communicate with the Quorum Driver channel: {:?}", error)]
    QuorumDriverCommunicationError { error: String },

    #[error("Operation timed out")]
    TimeoutError,

    #[error("Error executing {0}")]
    ExecutionError(String),

    #[error("Invalid committee composition")]
    InvalidCommittee(String),

    #[error("Missing committee information for epoch {0}")]
    MissingCommitteeAtEpoch(EpochId),

    #[error("Index store not available on this Fullnode.")]
    IndexStoreNotAvailable,

    #[error("Failed to get the system state object content")]
    SuiSystemStateNotFound,

    #[error("Found the sui system state object but it has an unexpected version")]
    SuiSystemStateUnexpectedVersion,

    #[error("Message version is not supported at the current protocol version")]
    WrongMessageVersion {
        message_version: u64,
        // the range in which the given message version is supported
        supported: SupportedProtocolVersions,
        // the current protocol version which is outside of that range
        current_protocol_version: ProtocolVersion,
    },

    #[error("unknown error: {0}")]
    Unknown(String),
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
/// Sub-status codes for the `MEMORY_LIMIT_EXCEEDED` VM Status Code which provides more context
pub enum VMMemoryLimitExceededSubStatusCode {
    EVENT_COUNT_LIMIT_EXCEEDED = 0,
    EVENT_SIZE_LIMIT_EXCEEDED = 1,
    NEW_ID_COUNT_LIMIT_EXCEEDED = 2,
    DELETED_ID_COUNT_LIMIT_EXCEEDED = 3,
    TRANSFER_ID_COUNT_LIMIT_EXCEEDED = 4,
}

pub type SuiResult<T = ()> = Result<T, SuiError>;
pub type UserInputResult<T = ()> = Result<T, UserInputError>;

// TODO these are both horribly wrong, categorization needs to be considered
impl From<PartialVMError> for SuiError {
    fn from(error: PartialVMError) -> Self {
        SuiError::ModuleVerificationFailure {
            error: error.to_string(),
        }
    }
}

impl From<ExecutionError> for SuiError {
    fn from(error: ExecutionError) -> Self {
        SuiError::ExecutionError(error.to_string())
    }
}

impl From<VMError> for SuiError {
    fn from(error: VMError) -> Self {
        SuiError::ModuleVerificationFailure {
            error: error.to_string(),
        }
    }
}

impl From<Status> for SuiError {
    fn from(status: Status) -> Self {
        let result = bcs::from_bytes::<SuiError>(status.details());
        if let Ok(sui_error) = result {
            sui_error
        } else {
            Self::RpcError(
                status.message().to_owned(),
                status.code().description().to_owned(),
            )
        }
    }
}

impl From<SuiError> for Status {
    fn from(error: SuiError) -> Self {
        let bytes = bcs::to_bytes(&error).unwrap();
        Status::with_details(tonic::Code::Internal, error.to_string(), bytes.into())
    }
}

impl From<ExecutionErrorKind> for SuiError {
    fn from(kind: ExecutionErrorKind) -> Self {
        ExecutionError::from_kind(kind).into()
    }
}

impl From<&str> for SuiError {
    fn from(error: &str) -> Self {
        SuiError::GenericAuthorityError {
            error: error.to_string(),
        }
    }
}

impl From<FastCryptoError> for SuiError {
    fn from(kind: FastCryptoError) -> Self {
        match kind {
            FastCryptoError::InvalidSignature => SuiError::InvalidSignature {
                error: "Invalid signature".to_string(),
            },
            _ => SuiError::Unknown("Unknown cryptography error".to_string()),
        }
    }
}

impl TryFrom<SuiError> for UserInputError {
    type Error = anyhow::Error;

    fn try_from(err: SuiError) -> Result<Self, Self::Error> {
        match err {
            SuiError::UserInputError { error } => Ok(error),
            other => anyhow::bail!("error {:?} is not UserInputError", other),
        }
    }
}

impl From<UserInputError> for SuiError {
    fn from(error: UserInputError) -> Self {
        SuiError::UserInputError { error }
    }
}

impl SuiError {
    pub fn individual_error_indicates_epoch_change(&self) -> bool {
        matches!(
            self,
            SuiError::ValidatorHaltedAtEpochEnd | SuiError::MissingCommitteeAtEpoch(_)
        )
    }

    /// Returns if the error is retryable and if the error's retryability is
    /// explicitly categorized.
    /// There should be only a handful of retryable errors. For now we list common
    /// non-retryable error below to help us find more retryable errors in logs.
    pub fn is_retryable(&self) -> (bool, bool) {
        match self {
            // Network error
            SuiError::RpcError { .. } => (true, true),

            // Reconfig error
            SuiError::ValidatorHaltedAtEpochEnd => (true, true),
            SuiError::MissingCommitteeAtEpoch(..) => (true, true),
            SuiError::WrongEpoch { .. } => (true, true),

            SuiError::UserInputError { error } => {
                match error {
                    // Only ObjectNotFound and DependentPackageNotFound is potentially retryable
                    UserInputError::ObjectNotFound { .. } => (true, true),
                    UserInputError::DependentPackageNotFound { .. } => (true, true),
                    _ => (false, true),
                }
            }

            // Non retryable error
            SuiError::ExecutionError(..) => (false, true),
            SuiError::ByzantineAuthoritySuspicion { .. } => (false, true),
            SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. } => {
                (false, true)
            }
            _ => (false, false),
        }
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type ExecutionErrorKind = ExecutionFailureStatus;

#[derive(Debug)]
pub struct ExecutionError {
    inner: Box<ExecutionErrorInner>,
}

#[derive(Debug)]
struct ExecutionErrorInner {
    kind: ExecutionErrorKind,
    source: Option<BoxError>,
    command: Option<CommandIndex>,
}

impl ExecutionError {
    pub fn new(kind: ExecutionErrorKind, source: Option<BoxError>) -> Self {
        Self {
            inner: Box::new(ExecutionErrorInner {
                kind,
                source,
                command: None,
            }),
        }
    }

    pub fn new_with_source<E: Into<BoxError>>(kind: ExecutionErrorKind, source: E) -> Self {
        Self::new(kind, Some(source.into()))
    }

    pub fn with_command_index(mut self, command: CommandIndex) -> Self {
        self.inner.command = Some(command);
        self
    }

    pub fn from_kind(kind: ExecutionErrorKind) -> Self {
        Self::new(kind, None)
    }

    pub fn kind(&self) -> &ExecutionErrorKind {
        &self.inner.kind
    }

    pub fn command(&self) -> Option<CommandIndex> {
        self.inner.command
    }

    pub fn source(&self) -> &Option<BoxError> {
        &self.inner.source
    }

    pub fn to_execution_status(&self) -> (ExecutionFailureStatus, Option<CommandIndex>) {
        (self.kind().clone(), self.command())
    }
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExecutionError: {:?}", self)
    }
}

impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source.as_ref().map(|e| &**e as _)
    }
}

impl From<ExecutionErrorKind> for ExecutionError {
    fn from(kind: ExecutionErrorKind) -> Self {
        Self::from_kind(kind)
    }
}

pub fn convert_vm_error<
    'r,
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E>,
>(
    error: VMError,
    vm: &'r MoveVM,
    state_view: &'r S,
) -> ExecutionError {
    let kind = match (error.major_status(), error.sub_status(), error.location()) {
        (StatusCode::EXECUTED, _, _) => {
            // If we have an error the status probably shouldn't ever be Executed
            debug_assert!(false, "VmError shouldn't ever report successful execution");
            ExecutionFailureStatus::VMInvariantViolation
        }
        (StatusCode::ABORTED, None, _) => {
            debug_assert!(false, "No abort code");
            // this is a Move VM invariant violation, the code should always be there
            ExecutionFailureStatus::VMInvariantViolation
        }
        (StatusCode::ABORTED, _, Location::Script) => {
            debug_assert!(false, "Scripts are not used in Sui");
            // this is a Move VM invariant violation, in the sense that the location
            // is malformed
            ExecutionFailureStatus::VMInvariantViolation
        }
        (StatusCode::ABORTED, Some(code), Location::Module(id)) => {
            let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
            debug_assert!(offset.is_some(), "Move should set the location on aborts");
            let (function, instruction) = offset.unwrap_or((0, 0));
            let function_name = vm.load_module(id, state_view).ok().map(|module| {
                let fdef = module.function_def_at(FunctionDefinitionIndex(function));
                let fhandle = module.function_handle_at(fdef.function);
                module.identifier_at(fhandle.name).to_string()
            });
            ExecutionFailureStatus::MoveAbort(
                MoveLocation {
                    module: id.clone(),
                    function,
                    instruction,
                    function_name,
                },
                code,
            )
        }
        (StatusCode::OUT_OF_GAS, _, _) => ExecutionFailureStatus::InsufficientGas,
        (_, _, location) => match error.major_status().status_type() {
            StatusType::Execution => {
                debug_assert!(error.major_status() != StatusCode::ABORTED);
                let location = match location {
                    Location::Module(id) => {
                        let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
                        debug_assert!(
                            offset.is_some(),
                            "Move should set the location on all execution errors. Error {error}"
                        );
                        let (function, instruction) = offset.unwrap_or((0, 0));
                        let function_name = vm.load_module(id, state_view).ok().map(|module| {
                            let fdef = module.function_def_at(FunctionDefinitionIndex(function));
                            let fhandle = module.function_handle_at(fdef.function);
                            module.identifier_at(fhandle.name).to_string()
                        });
                        Some(MoveLocation {
                            module: id.clone(),
                            function,
                            instruction,
                            function_name,
                        })
                    }
                    _ => None,
                };
                ExecutionFailureStatus::MovePrimitiveRuntimeError(location)
            }
            StatusType::Validation
            | StatusType::Verification
            | StatusType::Deserialization
            | StatusType::Unknown => ExecutionFailureStatus::VMVerificationOrDeserializationError,
            StatusType::InvariantViolation => ExecutionFailureStatus::VMInvariantViolation,
        },
    };
    ExecutionError::new_with_source(kind, error)
}

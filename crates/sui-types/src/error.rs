// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::*,
    committee::{Committee, EpochId, StakeUnit},
    messages::CommandIndex,
    object::Owner,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Debug};
use strum_macros::{AsRefStr, IntoStaticStr};
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
use crate::execution_status::ExecutionFailureStatus;
pub(crate) use fp_ensure;

#[macro_export]
macro_rules! exit_main {
    ($result:expr) => {
        match $result {
            Ok(_) => (),
            Err(err) => {
                let err = format!("{:?}", err);
                println!("{}", err.bold().red());
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
    #[error("Package verification failed: {err:?}")]
    PackageVerificationTimedout { err: String },
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
        "Balance of gas object {:?} is lower than the needed amount: {:?}.",
        gas_balance,
        needed_gas_amount
    )]
    GasBalanceTooLow {
        gas_balance: u128,
        needed_gas_amount: u128,
    },
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
    #[error("Gas price cannot exceed {:?} mist", max_gas_price)]
    GasPriceTooHigh { max_gas_price: u64 },
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

    #[error("Transaction is denied: {}", error)]
    TransactionDenied { error: String },

    #[error("Feature is not yet supported: {0}")]
    Unsupported(String),

    #[error("Query transactions with move function input error: {0}")]
    MoveFunctionInputError(String),
}

#[derive(
    Eq,
    PartialEq,
    Clone,
    Debug,
    Serialize,
    Deserialize,
    Hash,
    AsRefStr,
    IntoStaticStr,
    JsonSchema,
    Error,
)]
#[serde(tag = "code", rename = "ObjectResponseError", rename_all = "camelCase")]
pub enum SuiObjectResponseError {
    #[error("Object {:?} does not exist.", object_id)]
    NotExists { object_id: ObjectID },
    #[error("Cannot find dynamic field for parent object {:?}.", parent_object_id)]
    DynamicFieldNotFound { parent_object_id: ObjectID },
    #[error(
        "Object has been deleted object_id: {:?} at version: {:?} in digest {:?}",
        object_id,
        version,
        digest
    )]
    Deleted {
        object_id: ObjectID,
        /// Object version.
        version: SequenceNumber,
        /// Base64 string representing the object digest
        digest: ObjectDigest,
    },
    #[error("Unknown Error.")]
    Unknown,
    #[error("Display Error: {:?}", error)]
    DisplayError { error: String },
    // TODO: also integrate SuiPastObjectResponse (VersionNotFound,  VersionTooHigh)
}

/// Custom error type for Sui.
#[derive(
    Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr, IntoStaticStr,
)]
pub enum SuiError {
    #[error("Error checking transaction input objects: {:?}", error)]
    UserInputError { error: UserInputError },

    #[error("Error checking transaction object: {:?}", error)]
    SuiObjectResponseError { error: SuiObjectResponseError },

    #[error("Expecting a single owner, shared ownership found")]
    UnexpectedOwnerType,

    #[error("There are already {queue_len} transactions pending, above threshold of {threshold}")]
    TooManyTransactionsPendingExecution { queue_len: usize, threshold: usize },

    #[error("There are too many transactions pending in consensus")]
    TooManyTransactionsPendingConsensus,

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
    // TODO: Used for distinguishing between different occurrences of invalid signatures, to allow retries in some cases.
    #[error(
        "Signature is not valid, but a retry may result in a valid one: {}",
        error
    )]
    PotentiallyTemporarilyInvalidSignature { error: String },

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
    #[error("Failed to deserialize the Move module, reason: {error:?}.")]
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
    #[error("Failure serializing transaction in the requested format: {:?}", error)]
    TransactionSerializationError { error: String },
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
    #[error("Failed to submit transaction to consensus: {0}")]
    FailedToSubmitToConsensus(String),
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

    // Unsupported Operations on Fullnode
    #[error("Fullnode does not support handle_certificate")]
    FullNodeCantHandleCertificate,

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

    #[error("Failed to read dynamic field from table in the object store: {0}")]
    DynamicFieldReadError(String),

    #[error("Failed to read or deserialize system state related data structures on-chain: {0}")]
    SuiSystemStateReadError(String),

    #[error("Unexpected version error: {0}")]
    UnexpectedVersion(String),

    #[error("Message version is not supported at the current protocol version: {error}")]
    WrongMessageVersion { error: String },

    #[error("unknown error: {0}")]
    Unknown(String),

    #[error("Failed to perform file operation: {0}")]
    FileIOError(String),
}

#[repr(u64)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
/// Sub-status codes for the `UNKNOWN_VERIFICATION_ERROR` VM Status Code which provides more context
/// TODO: add more Vm Status errors. We use `UNKNOWN_VERIFICATION_ERROR` as a catchall for now.
pub enum VMMVerifierErrorSubStatusCode {
    MULTIPLE_RETURN_VALUES_NOT_ALLOWED = 0,
    INVALID_OBJECT_CREATION = 1,
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
    OBJECT_RUNTIME_CACHE_LIMIT_EXCEEDED = 5,
    OBJECT_RUNTIME_STORE_LIMIT_EXCEEDED = 6,
}

pub type SuiResult<T = ()> = Result<T, SuiError>;
pub type UserInputResult<T = ()> = Result<T, UserInputError>;

impl From<sui_protocol_config::Error> for SuiError {
    fn from(error: sui_protocol_config::Error) -> Self {
        SuiError::WrongMessageVersion { error: error.0 }
    }
}

impl From<ExecutionError> for SuiError {
    fn from(error: ExecutionError) -> Self {
        SuiError::ExecutionError(error.to_string())
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

// impl From<FastCryptoError> for SuiError {
//     fn from(kind: FastCryptoError) -> Self {
//         match kind {
//             FastCryptoError::InvalidSignature => SuiError::InvalidSignature {
//                 error: "Invalid signature".to_string(),
//             },
//             _ => SuiError::Unknown("Unknown cryptography error".to_string()),
//         }
//     }
// }

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

impl From<SuiObjectResponseError> for SuiError {
    fn from(error: SuiObjectResponseError) -> Self {
        SuiError::SuiObjectResponseError { error }
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

            SuiError::PotentiallyTemporarilyInvalidSignature { .. } => (true, true),

            // Overload errors
            SuiError::TooManyTransactionsPendingExecution { .. } => (true, true),
            SuiError::TooManyTransactionsPendingOnObject { .. } => (true, true),
            SuiError::TooManyTransactionsPendingConsensus => (true, true),

            // Non retryable error
            SuiError::ExecutionError(..) => (false, true),
            SuiError::ByzantineAuthoritySuspicion { .. } => (false, true),
            SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. } => {
                (false, true)
            }
            SuiError::ObjectLockConflict { .. } => (false, true),

            _ => (false, false),
        }
    }

    pub fn is_object_or_package_not_found(&self) -> bool {
        match self {
            SuiError::UserInputError { error } => {
                matches!(
                    error,
                    UserInputError::ObjectNotFound { .. }
                        | UserInputError::DependentPackageNotFound { .. }
                )
            }
            _ => false,
        }
    }

    pub fn is_overload(&self) -> bool {
        matches!(
            self,
            SuiError::TooManyTransactionsPendingExecution { .. }
                | SuiError::TooManyTransactionsPendingOnObject { .. }
                | SuiError::TooManyTransactionsPendingConsensus
        )
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

    pub fn invariant_violation<E: Into<BoxError>>(source: E) -> Self {
        Self::new_with_source(ExecutionFailureStatus::InvariantViolation, source)
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

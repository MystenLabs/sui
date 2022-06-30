// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{base_types::*, committee::EpochId, messages::ExecutionFailureStatus};
use move_binary_format::errors::{Location, PartialVMError, VMError};
use move_core_types::vm_status::{AbortLocation, StatusCode};
use narwhal_executor::{ExecutionStateError, SubscriberError};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt::Debug};
use thiserror::Error;
use typed_store::rocks::TypedStoreError;

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

/// Custom error type for Sui.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash)]
#[allow(clippy::large_enum_variant)]
pub enum SuiError {
    // Object misuse issues
    #[error("Error acquiring lock for object(s): {:?}", errors)]
    LockErrors { errors: Vec<SuiError> },
    #[error("Attempt to transfer an object that's not owned.")]
    TransferUnownedError,
    #[error("Attempt to transfer an object that does not have public transfer. Object transfer must be done instead using a distinct Move function call.")]
    TransferObjectWithoutPublicTransferError,
    #[error("A move package is expected, instead a move object is passed: {object_id}")]
    MoveObjectAsPackage { object_id: ObjectID },
    #[error("The SUI coin to be transferred has balance {balance}, which is not enough to cover the transfer amount {required}")]
    TransferInsufficientBalance { balance: u64, required: u64 },
    #[error("A move object is expected, instead a move package is passed: {object_id}")]
    MovePackageAsObject { object_id: ObjectID },
    #[error("Expecting a singler owner, shared ownership found")]
    UnexpectedOwnerType,
    #[error("Shared object not yet supported")]
    UnsupportedSharedObjectError,
    #[error("Object used as shared is not shared.")]
    NotSharedObjectError,
    #[error("An object that's owned by another object cannot be deleted or wrapped. It must be transferred to an account address first before deletion")]
    DeleteObjectOwnedObject,
    #[error("The shared locks for this transaction have not yet been set.")]
    SharedObjectLockNotSetObject,
    #[error("Invalid Batch Transaction: {}", error)]
    InvalidBatchTransaction { error: String },
    #[error("Object {child_id:?} is owned by object {parent_id:?}, which is not in the input")]
    MissingObjectOwner {
        child_id: ObjectID,
        parent_id: ObjectID,
    },

    // Signature verification
    #[error("Signature is not valid: {}", error)]
    InvalidSignature { error: String },
    #[error("Value was not signed by the correct sender: {}", error)]
    IncorrectSigner { error: String },
    #[error("Value was not signed by a known authority")]
    UnknownSigner,
    // Certificate verification
    #[error("Signature or certificate from wrong epoch, expected {expected_epoch}")]
    WrongEpoch { expected_epoch: EpochId },
    #[error("Signatures in a certificate must form a quorum")]
    CertificateRequiresQuorum,
    #[error(
        "The given sequence number ({given_sequence:?}) must match the next expected sequence ({expected_sequence:?}) number of the object ({object_id:?})"
    )]
    UnexpectedSequenceNumber {
        object_id: ObjectID,
        expected_sequence: SequenceNumber,
        given_sequence: SequenceNumber,
    },
    #[error("Conflicting transaction already received: {pending_transaction:?}")]
    ConflictingTransaction {
        pending_transaction: TransactionDigest,
    },
    #[error("Transaction processing failed: {err}")]
    ErrorWhileProcessingTransactionTransaction { err: String },
    #[error("Confirmation transaction processing failed: {err}")]
    ErrorWhileProcessingConfirmationTransaction { err: String },
    #[error(
    "Failed to execute certificate on a quorum of validators, cause by : {:#?}",
    errors.iter().map(| e | ToString::to_string(&e)).collect::<Vec<String>>()
    )]
    QuorumFailedToExecuteCertificate { errors: Vec<SuiError> },
    #[error("Module publish failed: {err}")]
    ErrorWhileProcessingPublish { err: String },
    #[error("Move call failed: {err}")]
    ErrorWhileProcessingMoveCall { err: String },
    #[error("An invalid answer was returned by the authority while requesting information")]
    ErrorWhileRequestingInformation,
    #[error("Object fetch failed for {object_id:?}, err {err:?}.")]
    ObjectFetchFailed { object_id: ObjectID, err: String },
    #[error("Object {object_id:?} at old version: {current_sequence_number:?}")]
    MissingEarlierConfirmations {
        object_id: ObjectID,
        current_sequence_number: VersionNumber,
    },
    #[error("System Transaction not accepted")]
    InvalidSystemTransaction,
    // Synchronization validation
    #[error("Transaction index must increase by one")]
    UnexpectedTransactionIndex,
    #[error("Once one iterator is allowed on a stream at once.")]
    ConcurrentIteratorError,
    #[error("The notifier subsystem is closed.")]
    ClosedNotifierError,

    // Account access
    #[error("No certificate with digest: {certificate_digest:?}")]
    CertificateNotfound {
        certificate_digest: TransactionDigest,
    },
    #[error("No parent for object {object_id:?} at this sequence number {sequence:?}")]
    ParentNotfound {
        object_id: ObjectID,
        sequence: SequenceNumber,
    },
    #[error("Unknown sender's account")]
    UnknownSenderAccount,
    #[error("Signatures in a certificate must be from different authorities.")]
    CertificateAuthorityReuse,
    #[error("Sequence numbers above the maximal value are not usable for transfers.")]
    InvalidSequenceNumber,
    #[error("Sequence number overflow.")]
    SequenceOverflow,
    #[error("Sequence number underflow.")]
    SequenceUnderflow,
    #[error("Wrong shard used.")]
    WrongShard,
    #[error("Invalid cross shard update.")]
    InvalidCrossShardUpdate,
    #[error("Invalid authenticator")]
    InvalidAuthenticator,
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid transaction digest.")]
    InvalidTransactionDigest,
    #[error(
        "Invalid Object digest for object {object_id:?}. Expected digest : {expected_digest:?}."
    )]
    InvalidObjectDigest {
        object_id: ObjectID,
        expected_digest: ObjectDigest,
    },
    #[error("Cannot deserialize.")]
    InvalidDecoding,
    #[error("Unexpected message.")]
    UnexpectedMessage,
    #[error("The transaction inputs contain duplicates ObjectRef's")]
    DuplicateObjectRefInput,
    #[error("Network error while querying service: {:?}.", error)]
    ClientIoError { error: String },
    #[error("Cannot transfer immutable object.")]
    TransferImmutableError,

    // Errors related to batches
    #[error("The number of items requested exceeds defined limits of {0}.")]
    TooManyItemsError(u64),
    #[error("The range specified is invalid.")]
    InvalidSequenceRangeError,
    #[error("No batches matched the range requested.")]
    NoBatchesFoundError,
    #[error("The channel to repond to the client returned an error.")]
    CannotSendClientMessageError,
    #[error("Subscription service had to drop {0} items")]
    SubscriptionItemsDroppedError(u64),
    #[error("Subscription service closed.")]
    SubscriptionServiceClosed,
    #[error("Checkpointing error: {}", error)]
    CheckpointingError { error: String },

    // Move module publishing related errors
    #[error("Failed to load the Move module, reason: {error:?}.")]
    ModuleLoadFailure { error: String },
    #[error("Failed to verify the Move module, reason: {error:?}.")]
    ModuleVerificationFailure { error: String },
    #[error("Failed to verify the Move module, reason: {error:?}.")]
    ModuleDeserializationFailure { error: String },
    #[error("Failed to publish the Move module(s), reason: {error:?}.")]
    ModulePublishFailure { error: String },
    #[error("Failed to build Move modules: {error:?}.")]
    ModuleBuildFailure { error: String },
    #[error("Dependent package not found on-chain: {package_id:?}")]
    DependentPackageNotFound { package_id: ObjectID },
    #[error("Move unit tests failed: {error:?}")]
    MoveUnitTestFailure { error: String },

    // Move call related errors
    #[error("Function resolution failure: {error:?}.")]
    FunctionNotFound { error: String },
    #[error("Module not found in package: {module_name:?}.")]
    ModuleNotFound { module_name: String },
    #[error("Function signature is invalid: {error:?}.")]
    InvalidFunctionSignature { error: String },
    #[error("Non-`entry` function used for entry point to execution: {error:?}.")]
    InvalidNonEntryFunction { error: String },
    #[error("Type error while binding function arguments: {error:?}.")]
    TypeError { error: String },
    #[error("Execution aborted: {error:?}.")]
    AbortedExecution { error: String },
    #[error("Invalid move event: {error:?}.")]
    InvalidMoveEvent { error: String },
    #[error("Circular object ownership detected")]
    CircularObjectOwnership,
    #[error("When an (either direct or indirect) child object of a shared object is passed as a Move argument,\
        either the child object's type or the shared object's type must be defined in the same module \
        as the called function. This is violated by object {child} (defined in module '{child_module}'), \
        whose ancestor {ancestor} is a shared object (defined in module '{ancestor_module}'), \
        and neither are defined in this module '{current_module}'")]
    InvalidSharedChildUse {
        child: ObjectID,
        child_module: String,
        ancestor: ObjectID,
        ancestor_module: String,
        current_module: String,
    },

    // Gas related errors
    #[error("Gas budget set higher than max: {error:?}.")]
    GasBudgetTooHigh { error: String },
    #[error("Insufficient gas: {error:?}.")]
    InsufficientGas { error: String },

    // Internal state errors
    #[error("Attempt to update state of TxContext from a different instance than original.")]
    InvalidTxUpdate,
    #[error("Attempt to re-initialize a transaction lock for objects {:?}.", refs)]
    TransactionLockExists { refs: Vec<ObjectRef> },
    #[error("Attempt to set an non-existing transaction lock.")]
    TransactionLockDoesNotExist,
    #[error("Attempt to reset a set transaction lock to a different value.")]
    TransactionLockReset,
    #[error("Could not find the referenced transaction [{:?}].", digest)]
    TransactionNotFound { digest: TransactionDigest },
    #[error("Could not find the referenced object {:?}.", object_id)]
    ObjectNotFound { object_id: ObjectID },
    #[error("Object deleted at reference {:?}.", object_ref)]
    ObjectDeleted { object_ref: ObjectRef },
    #[error("Object ID did not have the expected type")]
    BadObjectType { error: String },
    #[error("Move Execution failed")]
    MoveExecutionFailure,
    #[error("Wrong number of parameters for the transaction.")]
    ObjectInputArityViolation,
    #[error("Execution invariant violated")]
    ExecutionInvariantViolation,
    #[error("Authority did not return the information it is expected to have.")]
    AuthorityInformationUnavailable,
    #[error("Failed to update authority.")]
    AuthorityUpdateFailure,
    #[error(
        "We have received cryptographic level of evidence that authority {authority:?} is faulty in a Byzantine manner."
    )]
    ByzantineAuthoritySuspicion { authority: AuthorityName },
    #[error(
        "Sync from authority failed. From {xsource:?} to {destination:?}, digest {tx_digest:?}: {error:?}",
    )]
    PairwiseSyncFailed {
        xsource: AuthorityName,
        destination: AuthorityName,
        tx_digest: TransactionDigest,
        error: Box<SuiError>,
    },
    #[error("Storage error")]
    StorageError(#[from] TypedStoreError),
    #[error("Non-RocksDB Storage error: {0}")]
    GenericStorageError(String),
    #[error("Batch error: cannot send transaction to batch.")]
    BatchErrorSender,
    #[error("Authority Error: {error:?}")]
    GenericAuthorityError { error: String },

    #[error("Failed to dispatch event: {error:?}")]
    EventFailedToDispatch { error: String },

    #[error(
    "Failed to achieve quorum between authorities, cause by : {:#?}",
    errors.iter().map(| e | ToString::to_string(&e)).collect::<Vec<String>>()
    )]
    QuorumNotReached { errors: Vec<SuiError> },

    // Errors returned by authority and client read API's
    #[error("Failure serializing object in the requested format: {:?}", error)]
    ObjectSerializationError { error: String },
    #[error("Event store component is not active on this node")]
    NoEventStore,

    // Client side error
    #[error("Client state has a different pending transaction.")]
    ConcurrentTransactionError,
    #[error("Transfer should be received by us.")]
    IncorrectRecipientError,
    #[error("Too many authority errors were detected: {:?}", errors)]
    TooManyIncorrectAuthorities {
        errors: Vec<(AuthorityName, SuiError)>,
    },
    #[error("Inconsistent results observed in the Gateway. This should not happen and typically means there is a bug in the Sui implementation. Details: {error:?}")]
    InconsistentGatewayResult { error: String },
    #[error("Invalid transaction range query to the gateway: {:?}", error)]
    GatewayInvalidTxRangeQuery { error: String },

    // Errors related to the authority-consensus interface.
    #[error("Authority state can be modified by a single consensus client at the time")]
    OnlyOneConsensusClientPermitted,
    #[error("Failed to connect with consensus node: {0}")]
    ConsensusConnectionBroken(String),
    #[error("Failed to hear back from consensus: {0}")]
    FailedToHearBackFromConsensus(String),
    #[error("Failed to lock shared objects: {0}")]
    SharedObjectLockingFailure(String),
    #[error("Consensus listener is out of capacity")]
    ListenerCapacityExceeded,
    #[error("Failed to serialize/deserialize Narwhal message: {0}")]
    ConsensusSuiSerializationError(String),
    #[error("Only shared object transactions need to be sequenced")]
    NotASharedObjectTransaction,

    // Cryptography errors.
    #[error("Signature seed invalid length, input byte size was: {0}")]
    SignatureSeedInvalidLength(usize),
    #[error("HKDF error: {0}")]
    HkdfError(String),
    #[error("Signature key generation error: {0}")]
    SignatureKeyGenError(String),

    // Epoch related errors.
    #[error("Validator temporarily stopped processing transactions due to epoch change")]
    ValidatorHaltedAtEpochEnd,
    #[error("Inconsistent state detected during epoch change: {:?}", error)]
    InconsistentEpochState { error: String },

    // These are errors that occur when an RPC fails and is simply the utf8 message sent in a
    // Tonic::Status
    #[error("{0}")]
    RpcError(String),

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
}

pub type SuiResult<T = ()> = Result<T, SuiError>;

// TODO these are both horribly wrong, categorization needs to be considered
impl std::convert::From<PartialVMError> for SuiError {
    fn from(error: PartialVMError) -> Self {
        SuiError::ModuleVerificationFailure {
            error: error.to_string(),
        }
    }
}

impl std::convert::From<ExecutionError> for SuiError {
    fn from(error: ExecutionError) -> Self {
        SuiError::ExecutionError(error.to_string())
    }
}

impl std::convert::From<VMError> for SuiError {
    fn from(error: VMError) -> Self {
        SuiError::ModuleVerificationFailure {
            error: error.to_string(),
        }
    }
}

impl std::convert::From<SubscriberError> for SuiError {
    fn from(error: SubscriberError) -> Self {
        SuiError::SharedObjectLockingFailure(error.to_string())
    }
}

impl From<tonic::Status> for SuiError {
    fn from(status: tonic::Status) -> Self {
        Self::RpcError(status.message().to_owned())
    }
}

impl From<ExecutionErrorKind> for SuiError {
    fn from(kind: ExecutionErrorKind) -> Self {
        ExecutionError::from_kind(kind).into()
    }
}

impl std::convert::From<&str> for SuiError {
    fn from(error: &str) -> Self {
        SuiError::GenericAuthorityError {
            error: error.to_string(),
        }
    }
}

impl ExecutionStateError for SuiError {
    fn node_error(&self) -> bool {
        matches!(
            self,
            Self::ObjectFetchFailed { .. }
                | Self::ByzantineAuthoritySuspicion { .. }
                | Self::StorageError(..)
                | Self::GenericAuthorityError { .. }
        )
    }

    fn to_string(&self) -> String {
        ToString::to_string(&self)
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug, Serialize, Deserialize)]
pub enum ExecutionErrorKind {
    InsufficientGas,

    // Naitive Transfer errors
    TransferUnowned,
    TransferNonCoin,
    TransferObjectWithoutPublicTransfer,
    TransferInsufficientBalance,

    InvalidTransactionUpdate,

    ObjectNotFound,
    /// An object that's owned by another object cannot be deleted or wrapped. It must be
    /// transferred to an account address first before deletion
    DeleteObjectOwnedObject,
    /// #[error("Function resolution failure: {error:?}.")]
    FunctionNotFound,
    /// #[error("Module not found in package: {module_name:?}.")]
    ModuleNotFound,
    /// #[error("Function signature is invalid: {error:?}.")]
    InvalidFunctionSignature,
    /// #[error("Function visibility is invalid for an entry point to execution: {error:?}.")]
    InvalidFunctionVisibility,
    InvalidNonEntryFunction,
    ExecutionInvariantViolation,
    /// #[error("Type error while binding function arguments: {error:?}.")]
    TypeError,
    /// Circular object ownership detected
    CircularObjectOwnership,
    MissingObjectOwner,
    InvalidSharedChildUse,

    ModulePublishFailure,
    ModuleVerificationFailure,

    // Move Error
    VmError,
}

impl std::fmt::Display for ExecutionErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExecutionErrorKind: {:?}", self)
    }
}

impl std::error::Error for ExecutionErrorKind {}

#[derive(Debug)]
pub struct ExecutionError {
    inner: Box<ExecutionErrorInner>,
}

#[derive(Debug)]
struct ExecutionErrorInner {
    kind: ExecutionErrorKind,
    source: Option<BoxError>,
}

impl ExecutionError {
    pub fn new(kind: ExecutionErrorKind, source: Option<BoxError>) -> Self {
        Self {
            inner: Box::new(ExecutionErrorInner { kind, source }),
        }
    }

    pub fn new_with_source<E: Into<BoxError>>(kind: ExecutionErrorKind, source: E) -> Self {
        Self::new(kind, Some(source.into()))
    }

    pub fn from_kind(kind: ExecutionErrorKind) -> Self {
        Self::new(kind, None)
    }

    pub fn kind(&self) -> &ExecutionErrorKind {
        &self.inner.kind
    }

    pub fn to_execution_status(&self) -> ExecutionFailureStatus {
        match self.kind() {
            ExecutionErrorKind::InsufficientGas => ExecutionFailureStatus::InsufficientGas,
            ExecutionErrorKind::TransferUnowned
            | ExecutionErrorKind::TransferNonCoin
            | ExecutionErrorKind::TransferObjectWithoutPublicTransfer
            | ExecutionErrorKind::TransferInsufficientBalance
            | ExecutionErrorKind::InvalidTransactionUpdate
            | ExecutionErrorKind::ObjectNotFound
            | ExecutionErrorKind::DeleteObjectOwnedObject
            | ExecutionErrorKind::FunctionNotFound
            | ExecutionErrorKind::ModuleNotFound
            | ExecutionErrorKind::InvalidFunctionSignature
            | ExecutionErrorKind::InvalidFunctionVisibility
            | ExecutionErrorKind::InvalidNonEntryFunction
            | ExecutionErrorKind::ExecutionInvariantViolation
            | ExecutionErrorKind::TypeError
            | ExecutionErrorKind::CircularObjectOwnership
            | ExecutionErrorKind::MissingObjectOwner
            | ExecutionErrorKind::InvalidSharedChildUse
            | ExecutionErrorKind::ModulePublishFailure
            | ExecutionErrorKind::ModuleVerificationFailure => {
                ExecutionFailureStatus::MiscellaneousError
            }
            ExecutionErrorKind::VmError => {
                let source = if let Some(source) = self.source() {
                    source
                } else {
                    return ExecutionFailureStatus::MiscellaneousError;
                };

                if let Some(vmerror) = source.downcast_ref::<VMError>() {
                    match (
                        vmerror.major_status(),
                        vmerror.sub_status(),
                        vmerror.location(),
                    ) {
                        (StatusCode::EXECUTED, _, _) => {
                            // If we have an error the status probably shouldn't ever be Executed
                            debug_assert!(
                                false,
                                "VmError shouldn't ever report successful execution"
                            );
                        }
                        (StatusCode::ABORTED, Some(code), Location::Script) => {
                            return ExecutionFailureStatus::MoveAbort(AbortLocation::Script, code);
                        }
                        (StatusCode::ABORTED, Some(code), Location::Module(id)) => {
                            return ExecutionFailureStatus::MoveAbort(
                                AbortLocation::Module(id.to_owned()),
                                code,
                            );
                        }
                        (StatusCode::OUT_OF_GAS, _, _) => {
                            return ExecutionFailureStatus::InsufficientGas;
                        }
                        _ => return ExecutionFailureStatus::MiscellaneousError,
                    }
                }

                if let Some(partial_vmerror) = source.downcast_ref::<PartialVMError>() {
                    match partial_vmerror.major_status() {
                        StatusCode::EXECUTED => {
                            // If we have an error the status probably shouldn't ever be Executed
                            debug_assert!(
                                false,
                                "VmError shouldn't ever report successful execution"
                            );
                        }
                        StatusCode::OUT_OF_GAS => {
                            return ExecutionFailureStatus::InsufficientGas;
                        }
                        _ => return ExecutionFailureStatus::MiscellaneousError,
                    }
                }

                ExecutionFailureStatus::MiscellaneousError
            }
        }
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

impl From<VMError> for ExecutionError {
    fn from(error: VMError) -> Self {
        Self::new_with_source(ExecutionErrorKind::VmError, error)
    }
}

impl From<PartialVMError> for ExecutionError {
    fn from(error: PartialVMError) -> Self {
        Self::new_with_source(ExecutionErrorKind::VmError, error)
    }
}

// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::*,
    committee::{Committee, EpochId, StakeUnit},
    digests::CheckpointContentsDigest,
    execution_status::CommandArgumentError,
    messages_checkpoint::CheckpointSequenceNumber,
    object::Owner,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Debug};
use strum_macros::{AsRefStr, IntoStaticStr};
use thiserror::Error;
use tonic::Status;
use typed_store_error::TypedStoreError;

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
use crate::execution_status::{CommandIndex, ExecutionFailureStatus};
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

#[macro_export]
macro_rules! make_invariant_violation {
    ($($args:expr),* $(,)?) => {{
        if cfg!(debug_assertions) {
            panic!($($args),*)
        }
        $crate::error::ExecutionError::invariant_violation(format!($($args),*))
    }}
}

#[macro_export]
macro_rules! invariant_violation {
    ($($args:expr),* $(,)?) => {
        return Err(make_invariant_violation!($($args),*).into())
    };
}

#[macro_export]
macro_rules! assert_invariant {
    ($cond:expr, $($args:expr),* $(,)?) => {{
        if !$cond {
            invariant_violation!($($args),*)
        }
    }};
}

#[derive(
    Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr, IntoStaticStr,
)]
pub enum UserInputError {
    #[error("Mutable object {object_id} cannot appear more than one in one transaction")]
    MutableObjectUsedMoreThanOnce { object_id: ObjectID },
    #[error("Wrong number of parameters for the transaction")]
    ObjectInputArityViolation,
    #[error(
        "Could not find the referenced object {} at version {:?}",
        object_id,
        version
    )]
    ObjectNotFound {
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    },
    #[error(
        "Object ID {} Version {} Digest {} is not available for consumption, current version: {current_version}",
        .provided_obj_ref.0, .provided_obj_ref.1, .provided_obj_ref.2
    )]
    ObjectVersionUnavailableForConsumption {
        provided_obj_ref: ObjectRef,
        current_version: SequenceNumber,
    },
    #[error("Package verification failed: {err}")]
    PackageVerificationTimeout { err: String },
    #[error("Dependent package not found on-chain: {package_id}")]
    DependentPackageNotFound { package_id: ObjectID },
    #[error("Mutable parameter provided, immutable parameter expected")]
    ImmutableParameterExpectedError { object_id: ObjectID },
    #[error("Size limit exceeded: {limit} is {value}")]
    SizeLimitExceeded { limit: String, value: String },
    #[error(
        "Object {child_id} is owned by object {parent_id}. \
        Objects owned by other objects cannot be used as input arguments"
    )]
    InvalidChildObjectArgument {
        child_id: ObjectID,
        parent_id: ObjectID,
    },
    #[error("Invalid Object digest for object {object_id}. Expected digest : {expected_digest}")]
    InvalidObjectDigest {
        object_id: ObjectID,
        expected_digest: ObjectDigest,
    },
    #[error("Sequence numbers above the maximal value are not usable for transfers")]
    InvalidSequenceNumber,
    #[error("A move object is expected, instead a move package is passed: {object_id}")]
    MovePackageAsObject { object_id: ObjectID },
    #[error("A move package is expected, instead a move object is passed: {object_id}")]
    MoveObjectAsPackage { object_id: ObjectID },
    #[error("Transaction was not signed by the correct sender: {}", error)]
    IncorrectUserSignature { error: String },

    #[error("Object used as shared is not shared")]
    NotSharedObjectError,
    #[error("The transaction inputs contain duplicated ObjectRef's")]
    DuplicateObjectRefInput,

    // Gas related errors
    #[error("Transaction gas payment missing")]
    MissingGasPayment,
    #[error("Gas object is not an owned object with owner: {:?}", owner)]
    GasObjectNotOwnedObject { owner: Owner },
    #[error("Gas budget: {gas_budget} is higher than max: {max_budget}")]
    GasBudgetTooHigh { gas_budget: u64, max_budget: u64 },
    #[error("Gas budget: {gas_budget} is lower than min: {min_budget}")]
    GasBudgetTooLow { gas_budget: u64, min_budget: u64 },
    #[error(
        "Balance of gas object {gas_balance} is lower than the needed amount: {needed_gas_amount}"
    )]
    GasBalanceTooLow {
        gas_balance: u128,
        needed_gas_amount: u128,
    },
    #[error("Transaction kind does not support Sponsored Transaction")]
    UnsupportedSponsoredTransactionKind,
    #[error("Gas price {gas_price} under reference gas price (RGP) {reference_gas_price}")]
    GasPriceUnderRGP {
        gas_price: u64,
        reference_gas_price: u64,
    },
    #[error("Gas price cannot exceed {max_gas_price} mist")]
    GasPriceTooHigh { max_gas_price: u64 },
    #[error("Object {object_id} is not a gas object")]
    InvalidGasObject { object_id: ObjectID },
    #[error("Gas object does not have enough balance to cover minimal gas spend")]
    InsufficientBalanceToCoverMinimalGas,

    #[error(
        "Could not find the referenced object {object_id} as the asked version {asked_version:?} is higher than the latest {latest_version:?}"
    )]
    ObjectSequenceNumberTooHigh {
        object_id: ObjectID,
        asked_version: SequenceNumber,
        latest_version: SequenceNumber,
    },
    #[error("Object deleted at reference ({}, {:?}, {})", object_ref.0, object_ref.1, object_ref.2)]
    ObjectDeleted { object_ref: ObjectRef },
    #[error("Invalid Batch Transaction: {error}")]
    InvalidBatchTransaction { error: String },
    #[error("This Move function is currently disabled and not available for call")]
    BlockedMoveFunction,
    #[error("Empty input coins for Pay related transaction")]
    EmptyInputCoins,

    #[error(
        "SUI payment transactions use first input coin for gas payment, but found a different gas object"
    )]
    UnexpectedGasPaymentObject,

    #[error("Wrong initial version given for shared object")]
    SharedObjectStartingVersionMismatch,

    #[error(
        "Attempt to transfer object {object_id} that does not have public transfer. Object transfer must be done instead using a distinct Move function call"
    )]
    TransferObjectWithoutPublicTransferError { object_id: ObjectID },

    #[error(
        "TransferObjects, MergeCoin, and Publish cannot have empty arguments. \
        If MakeMoveVec has empty arguments, it must have a type specified"
    )]
    EmptyCommandInput,

    #[error("Transaction is denied: {error}")]
    TransactionDenied { error: String },

    #[error("Feature is not supported: {0}")]
    Unsupported(String),

    #[error("Query transactions with move function input error: {0}")]
    MoveFunctionInputError(String),

    #[error("Verified checkpoint not found for sequence number: {0}")]
    VerifiedCheckpointNotFound(CheckpointSequenceNumber),

    #[error("Verified checkpoint not found for digest: {0}")]
    VerifiedCheckpointDigestNotFound(String),

    #[error("Latest checkpoint sequence number not found")]
    LatestCheckpointSequenceNumberNotFound,

    #[error("Checkpoint contents not found for digest: {0}")]
    CheckpointContentsNotFound(CheckpointContentsDigest),

    #[error("Genesis transaction not found")]
    GenesisTransactionNotFound,

    #[error("Transaction {0} not found")]
    TransactionCursorNotFound(u64),

    #[error(
        "Object {} is a system object and cannot be accessed by user transactions",
        object_id
    )]
    InaccessibleSystemObject { object_id: ObjectID },
    #[error(
        "{max_publish_commands} max publish/upgrade commands allowed, {publish_count} provided"
    )]
    MaxPublishCountExceeded {
        max_publish_commands: u64,
        publish_count: u64,
    },

    #[error("Immutable parameter provided, mutable parameter expected")]
    MutableParameterExpected { object_id: ObjectID },

    #[error("Address {address} is denied for coin {coin_type}")]
    AddressDeniedForCoin {
        address: SuiAddress,
        coin_type: String,
    },

    #[error("Commands following a command with Random can only be TransferObjects or MergeCoins")]
    PostRandomCommandRestrictions,

    // Soft Bundle related errors
    #[error("Number of transactions ({size}) exceeds the maximum allowed ({limit}) in a batch")]
    TooManyTransactionsInBatch { size: usize, limit: u64 },
    #[error(
        "Total transactions size ({size}) bytes exceeds the maximum allowed ({limit}) bytes in a Soft Bundle"
    )]
    TotalTransactionSizeTooLargeInBatch { size: usize, limit: u64 },
    #[error("Transaction {digest} in Soft Bundle contains no shared objects")]
    NoSharedObjectError { digest: TransactionDigest },
    #[error("Transaction {digest} in Soft Bundle has already been executed")]
    AlreadyExecutedInSoftBundleError { digest: TransactionDigest },
    #[error("At least one certificate in Soft Bundle has already been processed")]
    CertificateAlreadyProcessed,
    #[error(
        "Gas price for transaction {digest} in Soft Bundle mismatch: want {expected}, have {actual}"
    )]
    GasPriceMismatchError {
        digest: TransactionDigest,
        expected: u64,
        actual: u64,
    },

    #[error("Coin type is globally paused for use: {coin_type}")]
    CoinTypeGlobalPause { coin_type: String },

    #[error("Invalid identifier found in the transaction: {error}")]
    InvalidIdentifier { error: String },

    #[error("Object used as owned is not owned")]
    NotOwnedObjectError,

    #[error("Invalid withdraw reservation: {error}")]
    InvalidWithdrawReservation { error: String },

    #[error("Transaction with empty gas payment must specify an expiration.")]
    MissingTransactionExpiration,

    #[error("Invalid transaction expiration: {error}")]
    InvalidExpiration { error: String },

    #[error("Transaction chain ID {provided} does not match network chain ID {expected}.")]
    InvalidChainId { provided: String, expected: String },
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
    #[error("Object {object_id} does not exist")]
    NotExists { object_id: ObjectID },
    #[error("Cannot find dynamic field for parent object {parent_object_id}")]
    DynamicFieldNotFound { parent_object_id: ObjectID },
    #[error(
        "Object has been deleted object_id: {object_id} at version: {version:?} in digest {digest}"
    )]
    Deleted {
        object_id: ObjectID,
        /// Object version.
        version: SequenceNumber,
        /// Base64 string representing the object digest
        digest: ObjectDigest,
    },
    #[error("Unknown Error")]
    Unknown,
    #[error("Display Error: {error}")]
    DisplayError { error: String },
    // TODO: also integrate SuiPastObjectResponse (VersionNotFound,  VersionTooHigh)
}

/// Custom error type for Sui.
#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Error, Hash)]
#[error(transparent)]
pub struct SuiError(#[from] pub Box<SuiErrorKind>);

/// Custom error type for Sui.
#[derive(
    Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr, IntoStaticStr,
)]
pub enum SuiErrorKind {
    #[error("Error checking transaction input objects: {error}")]
    UserInputError { error: UserInputError },

    #[error("Error checking transaction object: {error}")]
    SuiObjectResponseError { error: SuiObjectResponseError },

    #[error("Expecting a single owner, shared ownership found")]
    UnexpectedOwnerType,

    #[error("There are already {queue_len} transactions pending, above threshold of {threshold}")]
    TooManyTransactionsPendingExecution { queue_len: usize, threshold: usize },

    #[error("There are too many transactions pending in consensus")]
    TooManyTransactionsPendingConsensus,

    #[error(
        "Input {object_id} already has {queue_len} transactions pending, above threshold of {threshold}"
    )]
    TooManyTransactionsPendingOnObject {
        object_id: ObjectID,
        queue_len: usize,
        threshold: usize,
    },

    #[error(
        "Input {object_id} has a transaction {txn_age_sec} seconds old pending, above threshold of {threshold} seconds"
    )]
    TooOldTransactionPendingOnObject {
        object_id: ObjectID,
        txn_age_sec: u64,
        threshold: u64,
    },

    #[error("Soft bundle must only contain transactions of UserTransaction kind")]
    InvalidTxKindInSoftBundle,

    // Signature verification
    #[error("Signature is not valid: {}", error)]
    InvalidSignature { error: String },
    #[error("Required Signature from {expected} is absent {:?}", actual)]
    SignerSignatureAbsent {
        expected: String,
        actual: Vec<String>,
    },
    #[error("Expect {expected} signer signatures but got {actual}")]
    SignerSignatureNumberMismatch { expected: usize, actual: usize },
    #[error("Value was not signed by the correct sender: {}", error)]
    IncorrectSigner { error: String },
    #[error(
        "Value was not signed by a known authority. signer: {:?}, index: {:?}, committee: {committee}",
        signer,
        index
    )]
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
    #[allow(non_camel_case_types)]
    #[error("DEPRECATED")]
    DEPRECATED_ErrorWhileProcessingCertificate,
    #[error(
        "Failed to get a quorum of signed effects when processing transaction: {effects_map:?}"
    )]
    QuorumFailedToGetEffectsQuorumWhenProcessingTransaction {
        effects_map: BTreeMap<TransactionEffectsDigest, (Vec<AuthorityName>, StakeUnit)>,
    },
    #[error(
        "Failed to verify Tx certificate with executed effects, error: {error:?}, validator: {validator_name:?}"
    )]
    FailedToVerifyTxCertWithExecutedEffects {
        validator_name: AuthorityName,
        error: String,
    },
    #[error("Transaction is already finalized but with different user signatures")]
    TxAlreadyFinalizedWithDifferentUserSigs,

    // Account access
    #[error("Invalid authenticator")]
    InvalidAuthenticator,
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid transaction digest.")]
    InvalidTransactionDigest,

    #[error("Invalid digest length. Expected {expected}, got {actual}")]
    InvalidDigestLength { expected: usize, actual: usize },
    #[error("Invalid DKG message size")]
    InvalidDkgMessageSize,

    #[error("Unexpected message: {0}")]
    UnexpectedMessage(String),

    // Move module publishing related errors
    #[error("Failed to verify the Move module, reason: {error}.")]
    ModuleVerificationFailure { error: String },
    #[error("Failed to deserialize the Move module, reason: {error}.")]
    ModuleDeserializationFailure { error: String },
    #[error("Failed to publish the Move module(s), reason: {error}")]
    ModulePublishFailure { error: String },
    #[error("Failed to build Move modules: {error}.")]
    ModuleBuildFailure { error: String },

    // Move call related errors
    #[error("Function resolution failure: {error}.")]
    FunctionNotFound { error: String },
    #[error("Module not found in package: {module_name}.")]
    ModuleNotFound { module_name: String },
    #[error("Type error while binding function arguments: {error}.")]
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
    #[error(
        "Objects {obj_refs:?} are already locked by a transaction from a future epoch {locked_epoch:?}), attempt to override with a transaction from epoch {new_epoch:?}"
    )]
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
    TransactionEventsNotFound { digest: TransactionDigest },
    #[error("Could not find the referenced transaction effects [{digest:?}].")]
    TransactionEffectsNotFound { digest: TransactionDigest },
    #[error(
        "Attempt to move to `Executed` state an transaction that has already been executed: {:?}.",
        digest
    )]
    TransactionAlreadyExecuted { digest: TransactionDigest },
    #[error("Transaction reject reason not found for transaction {digest:?}")]
    TransactionRejectReasonNotFound { digest: TransactionDigest },
    #[error("Object ID did not have the expected type")]
    BadObjectType { error: String },
    #[error("Fail to retrieve Object layout for {st}")]
    FailObjectLayout { st: String },

    #[error("Execution invariant violated")]
    ExecutionInvariantViolation,
    #[error("Validator {authority:?} is faulty in a Byzantine manner: {reason:?}")]
    ByzantineAuthoritySuspicion {
        authority: AuthorityName,
        reason: String,
    },
    #[allow(non_camel_case_types)]
    #[serde(rename = "StorageError")]
    #[error("DEPRECATED")]
    DEPRECATED_StorageError,
    #[allow(non_camel_case_types)]
    #[serde(rename = "GenericStorageError")]
    #[error("DEPRECATED")]
    DEPRECATED_GenericStorageError,
    #[error(
        "Attempted to access {object} through parent {given_parent}, \
        but it's actual parent is {actual_owner}"
    )]
    InvalidChildObjectAccess {
        object: ObjectID,
        given_parent: ObjectID,
        actual_owner: Owner,
    },

    #[allow(non_camel_case_types)]
    #[serde(rename = "StorageMissingFieldError")]
    #[error("DEPRECATED")]
    DEPRECATED_StorageMissingFieldError,
    #[allow(non_camel_case_types)]
    #[serde(rename = "StorageCorruptedFieldError")]
    #[error("DEPRECATED")]
    DEPRECATED_StorageCorruptedFieldError,

    #[error("Authority Error: {error}")]
    GenericAuthorityError { error: String },

    #[error("Generic Bridge Error: {error}")]
    GenericBridgeError { error: String },

    #[error("Failed to dispatch subscription: {error}")]
    FailedToDispatchSubscription { error: String },

    #[error("Failed to serialize Owner: {error}")]
    OwnerFailedToSerialize { error: String },

    #[error("Failed to deserialize fields into JSON: {error}")]
    ExtraFieldFailedToDeserialize { error: String },

    #[error("Failed to execute transaction locally by Orchestrator: {error}")]
    TransactionOrchestratorLocalExecutionError { error: String },

    // Errors returned by authority and client read API's
    #[error("Failure serializing transaction in the requested format: {error}")]
    TransactionSerializationError { error: String },
    #[error("Failure deserializing transaction from the provided format: {error}")]
    TransactionDeserializationError { error: String },
    #[error("Failure serializing transaction effects from the provided format: {error}")]
    TransactionEffectsSerializationError { error: String },
    #[error("Failure deserializing transaction effects from the provided format: {error}")]
    TransactionEffectsDeserializationError { error: String },
    #[error("Failure serializing transaction events from the provided format: {error}")]
    TransactionEventsSerializationError { error: String },
    #[error("Failure deserializing transaction events from the provided format: {error}")]
    TransactionEventsDeserializationError { error: String },
    #[error("Failure serializing object in the requested format: {error}")]
    ObjectSerializationError { error: String },
    #[error("Failure deserializing object in the requested format: {error}")]
    ObjectDeserializationError { error: String },
    #[error("Event store component is not active on this node")]
    NoEventStore,

    // Client side error
    #[error("Too many authority errors were detected for {}: {:?}", action, errors)]
    TooManyIncorrectAuthorities {
        errors: Vec<(AuthorityName, SuiError)>,
        action: String,
    },
    #[error("Invalid transaction range query to the fullnode: {error}")]
    FullNodeInvalidTxRangeQuery { error: String },

    // Errors related to the authority-consensus interface.
    #[error("Failed to submit transaction to consensus: {0}")]
    FailedToSubmitToConsensus(String),
    #[error("Failed to connect with consensus node: {0}")]
    ConsensusConnectionBroken(String),
    #[error("Failed to execute handle_consensus_transaction on Sui: {0}")]
    HandleConsensusTransactionFailure(String),

    // Cryptography errors.
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
    #[error("Operations for epoch {0} have ended")]
    EpochEnded(EpochId),
    #[error("Error when advancing epoch: {error}")]
    AdvanceEpochError { error: String },

    #[error("Transaction Expired")]
    TransactionExpired,

    // These are errors that occur when an RPC fails and is simply the utf8 message sent in a
    // Tonic::Status
    #[error("{1} - {0}")]
    RpcError(String, String),

    #[error("Method not allowed")]
    InvalidRpcMethodError,

    #[error("Use of disabled feature: {error}")]
    UnsupportedFeatureError { error: String },

    #[error("Unable to communicate with the Quorum Driver channel: {error}")]
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

    #[error("Failed to read or deserialize bridge related data structures on-chain: {0}")]
    SuiBridgeReadError(String),

    #[error("Unexpected version error: {0}")]
    UnexpectedVersion(String),

    #[error("Message version is not supported at the current protocol version: {error}")]
    WrongMessageVersion { error: String },

    #[error("unknown error: {0}")]
    Unknown(String),

    #[error("Failed to perform file operation: {0}")]
    FileIOError(String),

    #[error("Failed to get JWK")]
    JWKRetrievalError,

    #[error("Storage error: {0}")]
    Storage(String),

    #[error(
        "Validator cannot handle the request at the moment. Please retry after at least {retry_after_secs} seconds."
    )]
    ValidatorOverloadedRetryAfter { retry_after_secs: u64 },

    #[error("Too many requests")]
    TooManyRequests,

    #[error("The request did not contain a certificate")]
    NoCertificateProvidedError,

    #[error("Nitro attestation verify failed: {0}")]
    NitroAttestationFailedToVerify(String),

    #[error("Failed to serialize {type_info}, error: {error}")]
    GrpcMessageSerializeError { type_info: String, error: String },

    #[error("Failed to deserialize {type_info}, error: {error}")]
    GrpcMessageDeserializeError { type_info: String, error: String },

    #[error(
        "Validator consensus rounds are lagging behind. last committed leader round: {last_committed_round}, requested round: {round}"
    )]
    ValidatorConsensusLagging {
        round: u32,
        last_committed_round: u32,
    },

    #[error("Invalid admin request: {0}")]
    InvalidAdminRequest(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error(
        "The current set of aliases for a required signer changed after the transaction was submitted"
    )]
    AliasesChanged,
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
    TOTAL_EVENT_SIZE_LIMIT_EXCEEDED = 7,
}

pub type SuiResult<T = ()> = Result<T, SuiError>;
pub type UserInputResult<T = ()> = Result<T, UserInputError>;

impl From<SuiErrorKind> for SuiError {
    fn from(error: SuiErrorKind) -> Self {
        SuiError(Box::new(error))
    }
}

impl std::ops::Deref for SuiError {
    type Target = SuiErrorKind;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<sui_protocol_config::Error> for SuiError {
    fn from(error: sui_protocol_config::Error) -> Self {
        SuiErrorKind::WrongMessageVersion { error: error.0 }.into()
    }
}

impl From<ExecutionError> for SuiError {
    fn from(error: ExecutionError) -> Self {
        SuiErrorKind::ExecutionError(error.to_string()).into()
    }
}

impl From<Status> for SuiError {
    fn from(status: Status) -> Self {
        if status.message() == "Too many requests" {
            return SuiErrorKind::TooManyRequests.into();
        }

        let result = bcs::from_bytes::<SuiError>(status.details());
        if let Ok(sui_error) = result {
            sui_error
        } else {
            SuiErrorKind::RpcError(
                status.message().to_owned(),
                status.code().description().to_owned(),
            )
            .into()
        }
    }
}

impl From<TypedStoreError> for SuiError {
    fn from(e: TypedStoreError) -> Self {
        SuiErrorKind::Storage(e.to_string()).into()
    }
}

impl From<crate::storage::error::Error> for SuiError {
    fn from(e: crate::storage::error::Error) -> Self {
        SuiErrorKind::Storage(e.to_string()).into()
    }
}

impl From<SuiErrorKind> for Status {
    fn from(error: SuiErrorKind) -> Self {
        let bytes = bcs::to_bytes(&error).unwrap();
        Status::with_details(tonic::Code::Internal, error.to_string(), bytes.into())
    }
}

impl From<SuiError> for Status {
    fn from(error: SuiError) -> Self {
        Status::from(error.into_inner())
    }
}

impl From<ExecutionErrorKind> for SuiError {
    fn from(kind: ExecutionErrorKind) -> Self {
        ExecutionError::from_kind(kind).into()
    }
}

impl From<&str> for SuiError {
    fn from(error: &str) -> Self {
        SuiErrorKind::GenericAuthorityError {
            error: error.to_string(),
        }
        .into()
    }
}

impl From<String> for SuiError {
    fn from(error: String) -> Self {
        SuiErrorKind::GenericAuthorityError { error }.into()
    }
}

impl TryFrom<SuiErrorKind> for UserInputError {
    type Error = anyhow::Error;

    fn try_from(err: SuiErrorKind) -> Result<Self, Self::Error> {
        match err {
            SuiErrorKind::UserInputError { error } => Ok(error),
            other => anyhow::bail!("error {:?} is not UserInputError", other),
        }
    }
}

impl TryFrom<SuiError> for UserInputError {
    type Error = anyhow::Error;

    fn try_from(err: SuiError) -> Result<Self, Self::Error> {
        err.into_inner().try_into()
    }
}

impl From<UserInputError> for SuiError {
    fn from(error: UserInputError) -> Self {
        SuiErrorKind::UserInputError { error }.into()
    }
}

impl From<SuiObjectResponseError> for SuiError {
    fn from(error: SuiObjectResponseError) -> Self {
        SuiErrorKind::SuiObjectResponseError { error }.into()
    }
}

impl PartialEq<SuiErrorKind> for SuiError {
    fn eq(&self, other: &SuiErrorKind) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<SuiError> for SuiErrorKind {
    fn eq(&self, other: &SuiError) -> bool {
        self == &*other.0
    }
}

impl SuiError {
    pub fn as_inner(&self) -> &SuiErrorKind {
        &self.0
    }

    pub fn into_inner(self) -> SuiErrorKind {
        *self.0
    }
}

impl SuiErrorKind {
    /// Returns the variant name of the error. Sub-variants within UserInputError are unpacked too.
    pub fn to_variant_name(&self) -> &'static str {
        match &self {
            SuiErrorKind::UserInputError { error } => error.into(),
            _ => self.into(),
        }
    }

    pub fn individual_error_indicates_epoch_change(&self) -> bool {
        matches!(
            self,
            SuiErrorKind::ValidatorHaltedAtEpochEnd | SuiErrorKind::MissingCommitteeAtEpoch(_)
        )
    }

    /// Returns if the error is retryable and if the error's retryability is
    /// explicitly categorized.
    /// There should be only a handful of retryable errors. For now we list common
    /// non-retryable error below to help us find more retryable errors in logs.
    pub fn is_retryable(&self) -> (bool, bool) {
        let retryable = match self {
            // Network error
            SuiErrorKind::RpcError { .. } => true,

            // Reconfig error
            SuiErrorKind::ValidatorHaltedAtEpochEnd => true,
            SuiErrorKind::MissingCommitteeAtEpoch(..) => true,
            SuiErrorKind::WrongEpoch { .. } => true,
            SuiErrorKind::EpochEnded(..) => true,

            SuiErrorKind::UserInputError { error } => {
                match error {
                    // Only ObjectNotFound and DependentPackageNotFound is potentially retryable
                    UserInputError::ObjectNotFound { .. } => true,
                    UserInputError::DependentPackageNotFound { .. } => true,
                    _ => false,
                }
            }

            SuiErrorKind::PotentiallyTemporarilyInvalidSignature { .. } => true,

            // Overload errors
            SuiErrorKind::TooManyTransactionsPendingExecution { .. } => true,
            SuiErrorKind::TooManyTransactionsPendingOnObject { .. } => true,
            SuiErrorKind::TooOldTransactionPendingOnObject { .. } => true,
            SuiErrorKind::TooManyTransactionsPendingConsensus => true,
            SuiErrorKind::ValidatorOverloadedRetryAfter { .. } => true,

            // Non retryable error
            SuiErrorKind::ExecutionError(..) => false,
            SuiErrorKind::ByzantineAuthoritySuspicion { .. } => false,
            SuiErrorKind::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. } => false,
            SuiErrorKind::TxAlreadyFinalizedWithDifferentUserSigs => false,
            SuiErrorKind::FailedToVerifyTxCertWithExecutedEffects { .. } => false,
            SuiErrorKind::ObjectLockConflict { .. } => false,

            // NB: This is not an internal overload, but instead an imposed rate
            // limit / blocking of a client. It must be non-retryable otherwise
            // we will make the threat worse through automatic retries.
            SuiErrorKind::TooManyRequests => false,

            // For all un-categorized errors, return here with categorized = false.
            _ => return (false, false),
        };

        (retryable, true)
    }

    pub fn is_object_or_package_not_found(&self) -> bool {
        match self {
            SuiErrorKind::UserInputError { error } => {
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
            SuiErrorKind::TooManyTransactionsPendingExecution { .. }
                | SuiErrorKind::TooManyTransactionsPendingOnObject { .. }
                | SuiErrorKind::TooOldTransactionPendingOnObject { .. }
                | SuiErrorKind::TooManyTransactionsPendingConsensus
        )
    }

    pub fn is_retryable_overload(&self) -> bool {
        matches!(self, SuiErrorKind::ValidatorOverloadedRetryAfter { .. })
    }

    pub fn retry_after_secs(&self) -> u64 {
        match self {
            SuiErrorKind::ValidatorOverloadedRetryAfter { retry_after_secs } => *retry_after_secs,
            _ => 0,
        }
    }

    /// Categorizes SuiError into ErrorCategory.
    pub fn categorize(&self) -> ErrorCategory {
        match self {
            SuiErrorKind::UserInputError { error } => {
                match error {
                    // ObjectNotFound and DependentPackageNotFound are potentially valid because the missing
                    // input can be created by other transactions.
                    UserInputError::ObjectNotFound { .. } => ErrorCategory::Aborted,
                    UserInputError::DependentPackageNotFound { .. } => ErrorCategory::Aborted,
                    // Other UserInputError variants indeed indicate invalid transaction.
                    _ => ErrorCategory::InvalidTransaction,
                }
            }

            SuiErrorKind::InvalidSignature { .. }
            | SuiErrorKind::SignerSignatureAbsent { .. }
            | SuiErrorKind::SignerSignatureNumberMismatch { .. }
            | SuiErrorKind::IncorrectSigner { .. }
            | SuiErrorKind::UnknownSigner { .. }
            | SuiErrorKind::TransactionExpired => ErrorCategory::InvalidTransaction,

            SuiErrorKind::ObjectLockConflict { .. } => ErrorCategory::LockConflict,

            SuiErrorKind::Unknown { .. }
            | SuiErrorKind::GrpcMessageSerializeError { .. }
            | SuiErrorKind::GrpcMessageDeserializeError { .. }
            | SuiErrorKind::ByzantineAuthoritySuspicion { .. }
            | SuiErrorKind::InvalidTxKindInSoftBundle
            | SuiErrorKind::UnsupportedFeatureError { .. }
            | SuiErrorKind::InvalidRequest { .. } => ErrorCategory::Internal,

            SuiErrorKind::TooManyTransactionsPendingExecution { .. }
            | SuiErrorKind::TooManyTransactionsPendingOnObject { .. }
            | SuiErrorKind::TooOldTransactionPendingOnObject { .. }
            | SuiErrorKind::TooManyTransactionsPendingConsensus
            | SuiErrorKind::ValidatorOverloadedRetryAfter { .. } => {
                ErrorCategory::ValidatorOverloaded
            }

            SuiErrorKind::TimeoutError => ErrorCategory::Unavailable,

            // Other variants are assumed to be retriable with new transaction submissions.
            _ => ErrorCategory::Aborted,
        }
    }
}

impl Ord for SuiError {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Ord::cmp(self.as_ref(), other.as_ref())
    }
}

impl PartialOrd for SuiError {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Debug for SuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_inner().fmt(f)
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

pub fn command_argument_error(e: CommandArgumentError, arg_idx: usize) -> ExecutionError {
    ExecutionError::from_kind(ExecutionErrorKind::command_argument_error(
        e,
        arg_idx as u16,
    ))
}

/// Types of SuiError.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, IntoStaticStr)]
pub enum ErrorCategory {
    // A generic error that is retriable with new transaction resubmissions.
    Aborted,
    // Any validator or full node can check if a transaction is valid.
    InvalidTransaction,
    // Lock conflict on the transaction input.
    LockConflict,
    // Unexpected client error, for example generating invalid request or entering into invalid state.
    // And unexpected error from the remote peer. The validator may be malicious or there is a software bug.
    Internal,
    // Validator is overloaded.
    ValidatorOverloaded,
    // Target validator is down or there are network issues.
    Unavailable,
}

impl ErrorCategory {
    // Whether the failure is retriable with new transaction submission.
    pub fn is_submission_retriable(&self) -> bool {
        matches!(
            self,
            ErrorCategory::Aborted
                | ErrorCategory::ValidatorOverloaded
                | ErrorCategory::Unavailable
        )
    }
}

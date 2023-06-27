// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::ErrorObject;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::execution_status::ExecutionFailureStatus;
use sui_types::quorum_driver_types::QuorumDriverError;
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    AnyhowError(#[from] anyhow::Error),

    // General catchall variant for errors that we don't expect to happen. Maps to -32603 internal error
    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error(transparent)]
    RPCServerError(#[from] jsonrpsee::core::Error),

    #[error(transparent)]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),

    #[error(transparent)]
    EncodingError(#[from] eyre::Report),

    #[error(transparent)]
    TokioJoinError(#[from] JoinError),

    #[error(transparent)]
    QuorumDriverError(#[from] QuorumDriverError),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error(transparent)]
    SuiObjectResponseError(#[from] SuiObjectResponseError),

    #[error(transparent)]
    Client(#[from] SuiRpcInputError),

    // Errors that map to -32603 internal error
    #[error(transparent)]
    Server(#[from] ServerError),
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Serde error encountered: {0}")]
    Serde(String),

    // TODO(wlmyng): circle back to determine how we can better do this. Errors that rely on this indirection are due to client- or server-faults that can only be discerned on RPC
    #[error("{0}")]
    SuiError(SuiError),
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        e.to_rpc_error()
    }
}

impl Error {
    pub fn to_rpc_error(self) -> RpcError {
        match self {
            Error::SuiError(sui_error) => match_sui_error(sui_error),
            Error::AnyhowError(err) => to_internal_error(err),
            Error::UnexpectedError(err) => to_internal_error(err),
            Error::RPCServerError(err) => err, // return RPCServerError as is
            Error::UserInputError(user_input_error) => {
                RpcError::Call(CallError::InvalidParams(user_input_error.into()))
            }
            Error::EncodingError(err) => to_internal_error(err),
            Error::TokioJoinError(err) => to_internal_error(err),
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::QuorumDriverInternalError(sui_error) => {
                    match_sui_error(sui_error)
                }
                QuorumDriverError::InvalidUserSignature(_)
                | QuorumDriverError::ObjectsDoubleUsed { .. } => {
                    RpcError::Call(CallError::InvalidParams(err.into()))
                }
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    let error_object = ErrorObject::owned(
                        -32000,
                        "Transaction has non recoverable errors from at least 1/3 of validators",
                        Some(errors),
                    );
                    RpcError::Call(CallError::Custom(error_object))
                }
                QuorumDriverError::SystemOverload { .. } => to_internal_error(err),
                _ => RpcError::Call(CallError::Failed(err.into())),
            },
            Error::SuiObjectResponseError(sui_object_response_error) => {
                match sui_object_response_error {
                    SuiObjectResponseError::Unknown { .. } => {
                        RpcError::Call(CallError::Failed(sui_object_response_error.into()))
                    }
                    _ => RpcError::Call(CallError::InvalidParams(sui_object_response_error.into())),
                }
            }
            Error::Client(sui_json_rpc_input_error) => {
                RpcError::Call(CallError::InvalidParams(sui_json_rpc_input_error.into()))
            }
            Error::FastCryptoError(err) => to_internal_error(err),
            Error::Server(err) => match err {
                ServerError::SuiError(err) => match err {
                    SuiError::ModuleDeserializationFailure { .. }
                    | SuiError::DeserializationError { .. } => to_internal_error(err),
                    _ => match_sui_error(err),
                },
                _ => to_internal_error(err),
            },
            _ => RpcError::Call(CallError::Failed(self.into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum SuiRpcInputError {
    #[error("Input contains duplicates")]
    ContainsDuplicates,

    #[error("Input exceeds limit of {0}")]
    SizeLimitExceeded(String),

    #[error("{0}")]
    GenericNotFound(String),

    #[error("{0}")]
    GenericInvalid(String),

    #[error("request_type` must set to `None` or `WaitForLocalExecution` if effects is required in the response")]
    InvalidExecuteTransactionRequestType,

    #[error("Unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),

    // Generic error variant for any kind of serde
    #[error("Failed to serialize {input}: {}", error)]
    Serialization { input: String, error: String },
    #[error("Failed to deserialize {input}: {}", error)]
    Deserialization { input: String, error: String },
}

pub fn match_sui_error(sui_error: SuiError) -> RpcError {
    match sui_error {
        SuiError::SuiObjectResponseError { error } => match error {
            SuiObjectResponseError::Unknown { .. } => {
                RpcError::Call(CallError::Failed(error.into()))
            }
            _ => RpcError::Call(CallError::InvalidParams(error.into())),
        },
        SuiError::ExecutionError(_err, status) => match status {
            ExecutionFailureStatus::VMInvariantViolation => to_internal_error(status),
            _ => RpcError::Call(CallError::InvalidParams(status.into())),
        },
        SuiError::UserInputError { .. }
        | SuiError::UnexpectedOwnerType
        | SuiError::TooManyTransactionsPendingExecution { .. }
        | SuiError::TooManyTransactionsPendingConsensus
        | SuiError::TooManyTransactionsPendingOnObject { .. }
        | SuiError::InvalidSignature { .. }
        | SuiError::SignerSignatureAbsent { .. }
        | SuiError::SignerSignatureNumberMismatch { .. }
        | SuiError::IncorrectSigner { .. }
        | SuiError::UnknownSigner { .. }
        | SuiError::StakeAggregatorRepeatedSigner { .. }
        | SuiError::PotentiallyTemporarilyInvalidSignature { .. }
        | SuiError::WrongEpoch { .. }
        | SuiError::CertificateRequiresQuorum
        | SuiError::ErrorWhileProcessingCertificate { .. }
        | SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. }
        | SuiError::InvalidSystemTransaction
        | SuiError::InvalidAuthenticator
        | SuiError::InvalidAddress
        | SuiError::InvalidTransactionDigest
        | SuiError::FunctionNotFound { .. }
        | SuiError::ModuleNotFound { .. }
        | SuiError::ObjectLockAlreadyInitialized { .. }
        | SuiError::ObjectLockConflict { .. }
        | SuiError::ObjectLockedAtFutureEpoch { .. }
        | SuiError::TransactionNotFound { .. }
        | SuiError::TransactionsNotFound { .. }
        | SuiError::TransactionEventsNotFound { .. }
        | SuiError::TransactionAlreadyExecuted { .. }
        | SuiError::InvalidChildObjectAccess { .. }
        | SuiError::UnexpectedVersion { .. }
        | SuiError::WrongMessageVersion { .. }
        | SuiError::FullNodeCantHandleCertificate
        | SuiError::CircularObjectOwnership
        | SuiError::TooManyIncorrectAuthorities { .. }
        | SuiError::IndexStoreNotAvailable
        | SuiError::UnsupportedFeatureError { .. }
        | SuiError::InvalidCommittee(_)
        | SuiError::ByzantineAuthoritySuspicion { .. }
        | SuiError::TypeError { .. }
        | SuiError::TransactionExpired => {
            RpcError::Call(CallError::InvalidParams(sui_error.into()))
        }
        SuiError::StorageError { .. }
        | SuiError::GenericStorageError { .. }
        | SuiError::StorageMissingFieldError { .. }
        | SuiError::StorageCorruptedFieldError { .. }
        | SuiError::GenericAuthorityError { .. }
        | SuiError::MissingCommitteeAtEpoch { .. }
        | SuiError::FileIOError { .. }
        | SuiError::JWKRetrievalError
        | SuiError::ExecutionInvariantViolation
        | SuiError::SuiSystemStateReadError(_)
        | SuiError::FailObjectLayout { .. }
        | SuiError::Unknown { .. } => to_internal_error(sui_error),
        _ => RpcError::Call(CallError::Failed(sui_error.into())),
    }
}

fn to_internal_error(err: impl ToString) -> RpcError {
    let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
    RpcError::Call(CallError::Custom(error_object))
}

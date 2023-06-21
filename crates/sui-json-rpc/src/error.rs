// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::ErrorObject;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::quorum_driver_types::QuorumDriverError;
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),

    #[error("Deserialization error: {0}")]
    BcsError(#[from] bcs::Error),

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
    SuiRpcInputError(#[from] SuiRpcInputError),
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        e.to_rpc_error()
    }
}

impl Error {
    pub fn to_rpc_error(self) -> RpcError {
        // Unfortunately Rust doesn't understand using enum variants as types
        match self {
            Error::SuiError(sui_error) => match sui_error {
                SuiError::SuiObjectResponseError { error } => match error {
                    SuiObjectResponseError::Unknown { .. } => {
                        RpcError::Call(CallError::Failed(error.into()))
                    }
                    _ => RpcError::Call(CallError::InvalidParams(error.into())),
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
                | SuiError::TransactionSerializationError { .. }
                | SuiError::ObjectSerializationError { .. }
                | SuiError::UnexpectedVersion { .. }
                | SuiError::WrongMessageVersion { .. }
                | SuiError::FullNodeCantHandleCertificate
                | SuiError::CircularObjectOwnership
                | SuiError::TooManyIncorrectAuthorities { .. }
                | SuiError::IndexStoreNotAvailable
                | SuiError::UnsupportedFeatureError { .. }
                | SuiError::InvalidCommittee(_)
                | SuiError::ByzantineAuthoritySuspicion { .. }
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
                | SuiError::Unknown { .. } => {
                    let error_object =
                        ErrorObject::owned(-32603, sui_error.to_string(), None::<String>);
                    RpcError::Call(CallError::Custom(error_object))
                }
                _ => RpcError::Call(CallError::Failed(sui_error.into())),
            },
            Error::InternalError(err) => {
                let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
                RpcError::Call(CallError::Custom(error_object))
            }
            Error::BcsError(err) => {
                let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
                RpcError::Call(CallError::Custom(error_object))
            }
            Error::UnexpectedError(err) => {
                let error_object = ErrorObject::owned(-32603, err, None::<String>);
                RpcError::Call(CallError::Custom(error_object))
            }
            Error::RPCServerError(err) => err, // return RPCServerError as is
            Error::UserInputError(user_input_error) => {
                RpcError::Call(CallError::InvalidParams(user_input_error.into()))
            }
            Error::EncodingError(err) => {
                let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
                RpcError::Call(CallError::Custom(error_object))
            }
            Error::TokioJoinError(err) => {
                let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
                RpcError::Call(CallError::Custom(error_object))
            }
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    let error_object = ErrorObject::owned(
                        -32000,
                        "Transaction has non recoverable errors from at least 1/3 of validators",
                        Some(errors),
                    );
                    RpcError::Call(CallError::Custom(error_object))
                }
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
            Error::SuiRpcInputError(sui_json_rpc_input_error) => {
                RpcError::Call(CallError::InvalidParams(sui_json_rpc_input_error.into()))
            }
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

    #[error("Unable to serialize: {0}")]
    CannotSerialize(#[from] bcs::Error),
}

/*
* ClientDeserializationError
* TypeError
* ModuleDeserializationFailure
* FailObjectLayout
* DynamicFieldReadError
* SuiSystemStateReadError
* ObjectDeserializationError

*/

/*
 * Depends on where this happens. This error enum to be used when error stems from us trying to deserialize something
 * ServerDeserializationError
 * ModuleDeserializationFailure
 * FailObjectLayout
 * DynamicFieldReadError
 * SuiSystemStateReadError
 * ObjectDeserializationError
 */

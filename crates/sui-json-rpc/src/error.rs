// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::types::error::{ErrorCode, CALL_EXECUTION_FAILED_CODE};
use jsonrpsee::types::{ErrorObject, ErrorObjectOwned};
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

impl From<Error> for ErrorObjectOwned {
    fn from(e: Error) -> Self {
        e.to_rpc_error()
    }
}

impl Error {
    pub fn to_rpc_error(self) -> ErrorObjectOwned {
        match self {
            Error::UserInputError(_) => {
                ErrorObject::owned(ErrorCode::InvalidParams.code(), self.to_string(), None::<()>)
            },
            Error::SuiRpcInputError(err) => match err {
                SuiRpcInputError::ContainsDuplicates => {
                    ErrorObject::owned(-54321, err.to_string(), None::<()>)
                }
                _ => ErrorObject::owned(ErrorCode::InvalidParams.code(), err.to_string(), None::<()>)
            },
            Error::SuiError(err) => match err {
                SuiError::TransactionNotFound { .. } | SuiError::TransactionsNotFound { .. } | SuiError::UserInputError { .. } => {
                    ErrorObject::owned(ErrorCode::InvalidParams.code(), err.to_string(), None::<()>)
                }
                _ => ErrorObject::owned(CALL_EXECUTION_FAILED_CODE, err.to_string(), None::<()>)
            },
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    ErrorObject::owned(
                        CALL_EXECUTION_FAILED_CODE,
                        "Transaction has non recoverable errors from at least 1/3 of validators",
                        Some(errors),
                    )
                }
                _ => ErrorObject::owned(CALL_EXECUTION_FAILED_CODE, err.to_string(), None::<()>)
            },
            _ => ErrorObject::owned(CALL_EXECUTION_FAILED_CODE, self.to_string(), None::<()>)
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::ErrorObject;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::quorum_driver_types::{QuorumDriverError, NON_RECOVERABLE_ERROR_MSG};
use thiserror::Error;
use tokio::task::JoinError;

pub type RpcInterimResult<T = ()> = Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SuiError(SuiError),

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

impl From<SuiError> for Error {
    fn from(e: SuiError) -> Self {
        match e {
            SuiError::UserInputError { error } => Self::UserInputError(error),
            SuiError::SuiObjectResponseError { error } => Self::SuiObjectResponseError(error),
            other => Self::SuiError(other),
        }
    }
}

impl Error {
    /// `InvalidParams`/`INVALID_PARAMS_CODE` for client errors.
    pub fn to_rpc_error(self) -> RpcError {
        match self {
            Error::UserInputError(_) => RpcError::Call(CallError::InvalidParams(self.into())),
            Error::SuiObjectResponseError(err) => match err {
                SuiObjectResponseError::NotExists { .. }
                | SuiObjectResponseError::DynamicFieldNotFound { .. }
                | SuiObjectResponseError::Deleted { .. }
                | SuiObjectResponseError::DisplayError { .. } => {
                    RpcError::Call(CallError::InvalidParams(err.into()))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
            },
            Error::SuiRpcInputError(err) => err.into(),
            Error::SuiError(sui_error) => match sui_error {
                SuiError::TransactionNotFound { .. }
                | SuiError::TransactionsNotFound { .. }
                | SuiError::TransactionEventsNotFound { .. } => {
                    RpcError::Call(CallError::InvalidParams(sui_error.into()))
                }
                _ => RpcError::Call(CallError::Failed(sui_error.into())),
            },
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    // Note: we probably want a more precise error than `INVALID_PARAMS_CODE`
                    // but to keep the error code consistent we still use `INVALID_PARAMS_CODE`
                    let error_object = ErrorObject::owned(
                        jsonrpsee::types::error::INVALID_PARAMS_CODE,
                        NON_RECOVERABLE_ERROR_MSG,
                        Some(errors),
                    );
                    RpcError::Call(CallError::Custom(error_object))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
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

    #[error("{0}")]
    CannotParseSuiStructTag(String),

    #[error(transparent)]
    Base64(#[from] eyre::Report),

    #[error("Deserialization error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),
}

impl From<SuiRpcInputError> for RpcError {
    fn from(e: SuiRpcInputError) -> Self {
        RpcError::Call(CallError::InvalidParams(e.into()))
    }
}

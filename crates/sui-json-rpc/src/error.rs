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

use crate::name_service::DomainParseError;

pub type RpcInterimResult<T = ()> = Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),

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

    #[error(transparent)]
    ClientError(#[from] ClientError),

    #[error(transparent)]
    ServerError(#[from] ServerError),
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        e.to_rpc_error()
    }
}

impl Error {
    pub fn to_rpc_error(self) -> RpcError {
        match self {
            Error::UserInputError(user_input_error) => {
                RpcError::Call(CallError::InvalidParams(user_input_error.into()))
            }
            Error::SuiRpcInputError(sui_json_rpc_input_error) => {
                RpcError::Call(CallError::InvalidParams(sui_json_rpc_input_error.into()))
            }
            Error::SuiError(sui_error) => match sui_error {
                SuiError::TransactionNotFound { .. } | SuiError::TransactionsNotFound { .. } => {
                    RpcError::Call(CallError::InvalidParams(sui_error.into()))
                }
                _ => RpcError::Call(CallError::Failed(sui_error.into())),
            },
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
            Error::ServerError(err) => err.into(),
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

    #[error("Unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),

    #[error("Unable to serialize: {0}")]
    CannotSerialize(#[from] bcs::Error),

    #[error("{0}")]
    CannotParseSuiStructTag(String),
}

#[derive(Debug, Error)]
pub enum ServerError {
    // do we really need these variants for server-side errors?
    #[error("Serde error")]
    Serde,

    #[error("Unexpected error")]
    Unexpected,
}

impl From<ServerError> for RpcError {
    fn from(e: ServerError) -> Self {
        to_internal_error("Internal server error, please try again later")
    }
}

fn to_internal_error(err: impl ToString) -> RpcError {
    let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
    RpcError::Call(CallError::Custom(error_object))
}

#[derive(Debug, Error)]
pub enum ClientError {
    // Any kind of serialization or deserialization error
    #[error("Invalid {param}: {reason}")]
    Serde { param: String, reason: String },

    #[error(transparent)]
    Domain(#[from] DomainParseError),

    #[error("Invalid {param}: {reason}")]
    InvalidParam { param: String, reason: String },
    // maybe InvalidParamMulti or something. param, value, reason
    #[error("`request_type` must set to `None` or `WaitForLocalExecution` if effects is required in the response")]
    InvalidExecuteTransactionRequestType,
}

impl From<ClientError> for RpcError {
    fn from(e: ClientError) -> Self {
        // TODO(wlmyng): Please check your input and try again text
        RpcError::Call(CallError::InvalidParams(e.into()))
    }
}

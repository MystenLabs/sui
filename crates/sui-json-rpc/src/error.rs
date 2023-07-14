// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use jsonrpsee::types::ErrorObject;
use std::fmt;
use strum_macros::{AsRefStr, EnumString};
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::quorum_driver_types::{QuorumDriverError, NON_RECOVERABLE_ERROR_MSG};
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
            Error::SuiError(sui_error) => match sui_error {
                SuiError::TransactionNotFound { .. } | SuiError::TransactionsNotFound { .. } => {
                    RpcError::Call(CallError::InvalidParams(sui_error.into()))
                }
                _ => RpcError::Call(CallError::Failed(sui_error.into())),
            },
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    let error_object =
                        ErrorObject::owned(-32000, NON_RECOVERABLE_ERROR_MSG, Some(errors));
                    RpcError::Call(CallError::Custom(error_object))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
            },
            Error::ClientError(err) => err.into(),
            Error::ServerError(err) => err.into(),
            _ => RpcError::Call(CallError::Failed(self.into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Serde error")]
    Serde(SuiError),

    #[error("Bcs error")]
    Bcs(bcs::Error),
}

impl From<ServerError> for RpcError {
    fn from(_e: ServerError) -> Self {
        to_internal_error("Internal server error, please try again later")
    }
}

fn to_internal_error(err: impl ToString) -> RpcError {
    let error_object = ErrorObject::owned(-32603, err.to_string(), None::<String>);
    RpcError::Call(CallError::Custom(error_object))
}

// One-to-one of the return type of the RPC method where applicable
#[derive(Debug, Clone, Copy, AsRefStr, EnumString)]
pub enum EntityType {
    DelegatedStake,
    SuiMoveNormalizedStruct,
    SuiMoveNormalizedFunction,
    Object,
    Package,
    Product,
    SuiMoveNormalizedModule,
    Checkpoint,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    // Any kind of serialization or deserialization error
    #[error("Serialization error on '{param}': {reason}")]
    Serde { param: &'static str, reason: String },

    #[error("Base64 error on '{param}': {reason}")]
    Base64 { param: &'static str, reason: String },

    #[error("BCS error on '{param}': {reason}")]
    Bcs {
        param: &'static str,
        reason: bcs::Error,
    },

    #[error("FastCrypto error on '{param}': {reason}")]
    FastCrypto {
        param: &'static str,
        reason: FastCryptoError,
    },

    #[error("Invalid combination of type and value: {0}")]
    SerdeWithLayout(String),

    #[error(transparent)]
    Domain(#[from] DomainParseError),

    #[error("Invalid '{param}': {reason}")]
    InvalidParam { param: &'static str, reason: String },
    // maybe InvalidParamMulti or something. param, value, reason
    #[error("`request_type` must set to `None` or `WaitForLocalExecution` if effects is required in the response")]
    InvalidExecuteTransactionRequestType,

    #[error("{entity} '{id}' not found")]
    NotFound { entity: EntityType, id: String },

    #[error("{entity} '{id}' not found in {in_entity} '{in_id}'")]
    NotFoundIn {
        entity: EntityType,
        id: String,
        in_entity: EntityType,
        in_id: String,
    },

    #[error("{entity} '{id}' not found most likely due to pruning")]
    Pruned { entity: EntityType, id: String },

    // For error scenarios that don't fit cleanly into NotFound
    #[error("{0}")]
    NotFoundCustom(String),

    #[error("`{param}` exceeds limit of `{limit}`")]
    LimitExceeded { param: &'static str, limit: usize },

    #[error("`{param}` must be at least `{limit}`")]
    LimitTooSmall { param: &'static str, limit: usize },

    #[error("Invalid 'version': unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),
}

impl From<ClientError> for RpcError {
    fn from(e: ClientError) -> Self {
        RpcError::Call(CallError::InvalidParams(e.into()))
    }
}

#[derive(Debug, Error)]
pub enum ObjectDisplayError {
    #[error("Not a move struct")]
    NotMoveStruct,

    #[error("Failed to extract layout")]
    Layout,

    #[error("Failed to extract Move object")]
    MoveObject,

    #[error(transparent)]
    Deserialization(#[from] SuiError),

    #[error(transparent)] // Failed to deserialize 'VersionUpdatedEvent': {e}
    Bcs(#[from] bcs::Error),
}

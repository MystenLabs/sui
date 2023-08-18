// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::types::error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE};
use jsonrpsee::types::ErrorObjectOwned;
use sui_json_rpc::error::{TRANSACTION_EXECUTION_CLIENT_ERROR_CODE, TRANSIENT_ERROR_CODE};
use sui_types::base_types::{SuiAddress, TransactionDigest};
use sui_types::error::UserInputError;
use thiserror::Error;

pub type SuiRpcResult<T = ()> = Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    RpcError(#[from] jsonrpsee::core::Error),
    #[error(transparent)]
    BcsSerialisationError(#[from] bcs::Error),
    #[error(transparent)]
    UserInputError(#[from] UserInputError),
    #[error("Subscription error : {0}")]
    Subscription(String),
    #[error("Encountered error when confirming tx status for {0:?}, err: {1:?}")]
    TransactionConfirmationError(TransactionDigest, jsonrpsee::core::Error),
    #[error("Failed to confirm tx status for {0:?} within {1} seconds.")]
    FailToConfirmTransactionStatus(TransactionDigest, u64),
    #[error("Data error: {0}")]
    DataError(String),
    #[error("Client/Server api version mismatch, client api version : {client_version}, server api version : {server_version}")]
    ServerVersionMismatch {
        client_version: String,
        server_version: String,
    },
    #[error("Insufficient fund for address [{address}], requested amount: {amount}")]
    InsufficientFund { address: SuiAddress, amount: u128 },
}

#[derive(Debug, Clone)]
pub struct RpcErrorObject {
    code: ErrorCode,
    message: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidParams,
    InternalError,
    TransientError,
    TransactionExecutionClientError,
    Custom(i32),
}

impl ErrorCode {
    pub fn code(&self) -> i32 {
        match self {
            ErrorCode::InvalidParams => INVALID_PARAMS_CODE,
            ErrorCode::InternalError => INTERNAL_ERROR_CODE,
            ErrorCode::TransientError => TRANSIENT_ERROR_CODE,
            ErrorCode::TransactionExecutionClientError => TRANSACTION_EXECUTION_CLIENT_ERROR_CODE,
            ErrorCode::Custom(code) => *code,
        }
    }
}

impl From<i32> for ErrorCode {
    fn from(code: i32) -> Self {
        match code {
            INVALID_PARAMS_CODE => ErrorCode::InvalidParams,
            INTERNAL_ERROR_CODE => ErrorCode::InternalError,
            TRANSIENT_ERROR_CODE => ErrorCode::TransientError,
            TRANSACTION_EXECUTION_CLIENT_ERROR_CODE => ErrorCode::TransactionExecutionClientError,
            _ => ErrorCode::Custom(code),
        }
    }
}

impl From<jsonrpsee::core::Error> for RpcErrorObject {
    fn from(err: jsonrpsee::core::Error) -> Self {
        let error_object_owned: ErrorObjectOwned = err.into();
        RpcErrorObject {
            code: ErrorCode::from(error_object_owned.code()),
            message: error_object_owned.message().to_string(),
        }
    }
}

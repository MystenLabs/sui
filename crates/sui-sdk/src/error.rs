// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use crate::json_rpc_error::Error as JsonRpcError;
use sui_types::base_types::{SuiAddress, TransactionDigest};
use sui_types::error::UserInputError;
use thiserror::Error;

pub type SuiRpcResult<T = ()> = Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    RpcError(#[from] jsonrpsee::core::ClientError),
    #[error(transparent)]
    JsonRpcError(JsonRpcError),
    #[error(transparent)]
    BcsSerialisationError(#[from] bcs::Error),
    #[error(transparent)]
    JsonSerializationError(#[from] serde_json::Error),
    #[error(transparent)]
    UserInputError(#[from] UserInputError),
    #[error("Subscription error : {0}")]
    Subscription(String),
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

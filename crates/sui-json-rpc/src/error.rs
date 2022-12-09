// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use sui_types::error::SuiError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Sui error: {0}")]
    SuiError(#[from] SuiError),

    #[error("{0}")]
    InternalError(#[from] anyhow::Error),

    #[error("Deserialization error: {0}")]
    BcsError(#[from] bcs::Error),

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        RpcError::Call(CallError::Failed(e.into()))
    }
}

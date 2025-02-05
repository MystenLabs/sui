// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod execution;
pub use execution::SimulateTransaction;
pub use execution::SimulateTransactionQueryParameters;
pub use execution::TransactionSimulationResponse;

mod resolve;
pub use resolve::ResolveTransaction;
pub use resolve::ResolveTransactionQueryParameters;
pub use resolve::ResolveTransactionResponse;

use super::{ApiEndpoint, RouteHandler};
use sui_sdk_types::TransactionDigest;

#[derive(Debug)]
pub struct TransactionNotFoundError(pub TransactionDigest);

impl std::fmt::Display for TransactionNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transaction {} not found", self.0)
    }
}

impl std::error::Error for TransactionNotFoundError {}

impl From<TransactionNotFoundError> for crate::RpcServiceError {
    fn from(value: TransactionNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}

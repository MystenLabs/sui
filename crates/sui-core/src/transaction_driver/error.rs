// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::error::SuiError;
use thiserror::Error;

/// Client facing errors regarding transaction submission via Transaction Driver.
/// Every invariant needs detailed content to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash)]
pub enum TransactionDriverError {
    #[error("Serialization error: {0}")]
    SerializationError(SuiError),
    #[error("Deserialization error: {0}")]
    DeserializationError(SuiError),
    #[error("Transaction timed out getting consensus position")]
    TimeoutSubmittingTransaction,
    #[error("Transaction timed out while acknowledging effects")]
    TimeoutAcknowledgingEffects,
    #[error("Transaction timed out while getting full effects")]
    TimeoutGettingFullEffects,
    #[error("Failed to call validator {0}: {1}")]
    RpcFailure(String, String),
    #[error("Failed to find execution data: {0}")]
    ExecutionDataNotFound(String),
    #[error("Transaction rejected with reason: {0}")]
    TransactionRejected(String),
    #[error("Transaction expired at round: {0}")]
    TransactionExpired(String),
    #[error("Transaction rejected with reason: {0} & expired with reason: {1}")]
    TransactionRejectedOrExpired(String, String),
    #[error("Forked execution results: total_responses_weight {total_responses_weight}, executed_weight {executed_weight}, rejected_weight {rejected_weight}, expired_weight {expired_weight}, Errors: {errors:?}")]
    ForkedExecution {
        total_responses_weight: u64,
        executed_weight: u64,
        rejected_weight: u64,
        expired_weight: u64,
        errors: Vec<String>,
    },
}

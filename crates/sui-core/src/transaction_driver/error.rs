// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::{committee::EpochId, error::SuiError};
use thiserror::Error;

/// Client facing errors regarding transaction submission via Transaction Driver.
/// Every invariant needs detailed content to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash)]
pub enum TransactionDriverError {
    // Errors against individual validators.
    #[error("Transaction timed out getting consensus position")]
    TimedOutSubmittingTransaction,
    #[error("Transaction timed out while getting full effects")]
    TimedOutGettingFullEffectsAtValidator,
    #[error("Failed to find execution data: {0}")]
    ExecutionDataNotFound(String),
    #[error("Transaction rejected at peer validator")]
    TransactionRejectedAtValidator(String),
    #[error("Transaction status expired at peer validator, currently at epoch {0} round {1}")]
    TransactionStatusExpired(EpochId, u32),
    #[error("Validator internal error: {0}")]
    ValidatorInternalError(SuiError),

    // TODO(fastpath): after proper error aggregation, there errors should not exist.
    #[error("No more targets to retry")]
    NoMoreTargets,
    #[error("Transaction driver failed after retries. See logs for details.")]
    TransactionDriverFailure,

    // TODO(fastpath): Move these aggregated errors to a different status.
    #[error("Transaction rejected with stake {0} & reasons: {1}")]
    TransactionRejected(u64, String),
    #[error("Transaction rejected with stake {0} and expired with stake {1}, reasons: {2}")]
    TransactionRejectedOrExpired(u64, u64, String),
    #[error("Forked execution results: total_responses_weight {total_responses_weight}, executed_weight {executed_weight}, rejected_weight {rejected_weight}, expired_weight {expired_weight}, Errors: {errors:?}")]
    ForkedExecution {
        total_responses_weight: u64,
        executed_weight: u64,
        rejected_weight: u64,
        expired_weight: u64,
        errors: Vec<String>,
    },
}

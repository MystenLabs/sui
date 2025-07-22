// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use itertools::Itertools as _;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::{EpochId, StakeUnit},
    digests::TransactionEffectsDigest,
    error::SuiError,
};
use thiserror::Error;

/// Errors emitted from individual validators during transaction driver operations.
///
/// These errors are associated with the transaction and authority externally, so it is unnecessary
/// to include those information in these messages.
///
/// NOTE: these errors will be aggregated across authorities by status and reported to the caller.
/// So the error messages should not contain authority specific information, such as authority name.
#[derive(Eq, PartialEq, Clone, Debug, Error)]
pub(crate) enum TransactionRequestError {
    #[error("Request timed out submitting transaction")]
    TimedOutSubmittingTransaction,
    #[error("Request timed out getting full effects")]
    TimedOutGettingFullEffectsAtValidator,
    #[error("Failed to find execution data")]
    ExecutionDataNotFound,

    // Rejected by the validator when voting on the transaction.
    #[error("{0}")]
    RejectedAtValidator(SuiError),
    // Transaction status has been dropped from cache at the validator.
    #[error("Transaction status expired")]
    StatusExpired(EpochId, u32),
    // Request to submit transaction or get full effects failed.
    #[error("{0}")]
    Aborted(SuiError),
}

impl TransactionRequestError {
    pub fn is_submission_retriable(&self) -> bool {
        match self {
            TransactionRequestError::RejectedAtValidator(error) => {
                error.is_transaction_submission_retriable()
            }
            TransactionRequestError::Aborted(error) => error.is_transaction_submission_retriable(),
            _ => true,
        }
    }
}

/// Client facing errors on transaction processing via Transaction Driver.
///
/// NOTE: every error should indicate if it is retriable.
#[derive(Eq, PartialEq, Clone, Debug, Error)]
pub enum TransactionDriverError {
    #[error("Transaction is rejected by more than 1/3 of validators by stake: non-retriable errors: {submission_non_retriable_errors}, retriable errors: {submission_retriable_errors}")]
    Rejected {
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
        submission_retriable: bool,
    },
    #[error("Transaction execution observed forked outputs: {observed_effects_digests}, non-retriable errors: {submission_non_retriable_errors}, retriable errors: {submission_retriable_errors}")]
    ForkedExecution {
        observed_effects_digests: AggregatedEffectsDigests,
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
        submission_retriable: bool,
    },
}

impl TransactionDriverError {
    pub fn is_retriable(&self) -> bool {
        match self {
            TransactionDriverError::Rejected {
                submission_retriable,
                ..
            } => *submission_retriable,
            TransactionDriverError::ForkedExecution {
                submission_retriable,
                ..
            } => *submission_retriable,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct AggregatedRequestErrors {
    pub errors: Vec<(String, Vec<AuthorityName>, StakeUnit)>,
    pub total_stake: StakeUnit,
}

impl std::fmt::Display for AggregatedRequestErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self
            .errors
            .iter()
            .map(|(error, names, stake)| {
                format!(
                    "{} {{ {} }} with {} stake",
                    error,
                    names.iter().map(|n| n.concise_owned()).join(", "),
                    stake
                )
            })
            .join("; ");
        write!(f, "{}", msg)?;
        Ok(())
    }
}

pub(crate) fn aggregate_request_errors(
    errors: Vec<(AuthorityName, StakeUnit, TransactionRequestError)>,
) -> AggregatedRequestErrors {
    let mut total_stake = 0;
    let mut aggregated_errors = BTreeMap::<String, (Vec<AuthorityName>, StakeUnit)>::new();

    for (name, stake, error) in errors {
        total_stake += stake;
        let key = error.to_string();
        let entry = aggregated_errors.entry(key).or_default();
        entry.0.push(name);
        entry.1 += stake;
    }

    let mut errors: Vec<_> = aggregated_errors
        .into_iter()
        .map(|(error, (names, stake))| (error, names, stake))
        .collect();
    errors.sort_by_key(|(_, _, stake)| std::cmp::Reverse(*stake));

    AggregatedRequestErrors {
        errors,
        total_stake,
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct AggregatedEffectsDigests {
    pub digests: Vec<(TransactionEffectsDigest, Vec<AuthorityName>, StakeUnit)>,
}

impl std::fmt::Display for AggregatedEffectsDigests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self
            .digests
            .iter()
            .map(|(digest, names, stake)| {
                format!(
                    "{} {{ {} }} with {} stake",
                    digest,
                    names.iter().map(|n| n.concise_owned()).join(", "),
                    stake
                )
            })
            .join("; ");
        write!(f, "{}", msg)?;
        Ok(())
    }
}

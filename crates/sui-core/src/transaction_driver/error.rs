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
    #[error("Transaction rejected by consensus")]
    RejectedByConsensus,
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
#[derive(Eq, PartialEq, Clone)]
pub enum TransactionDriverError {
    /// Transient failure during transaction processing that prevents the transaction from finalization.
    /// Retriable with new transaction submission / call to TransactionDriver.
    Aborted {
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
        observed_effects_digests: AggregatedEffectsDigests,
    },
    /// Over validity threshold of validators rejected the transaction as invalid.
    /// Non-retriable.
    InvalidTransaction {
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
    },
    /// Transaction execution observed multiple effects digests, and it is no longer possible to
    /// certify any of them.
    /// Non-retriable.
    ForkedExecution {
        observed_effects_digests: AggregatedEffectsDigests,
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
    },
}

impl TransactionDriverError {
    pub fn is_retriable(&self) -> bool {
        match self {
            TransactionDriverError::Aborted { .. } => true,
            TransactionDriverError::InvalidTransaction { .. } => false,
            TransactionDriverError::ForkedExecution { .. } => false,
        }
    }

    fn display_aborted(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionDriverError::Aborted {
            submission_non_retriable_errors,
            submission_retriable_errors,
            observed_effects_digests,
        } = self
        else {
            return Ok(());
        };
        let mut msgs = vec![
            "Transaction processing aborted (retriable with the same transaction).".to_string(),
        ];
        if submission_retriable_errors.total_stake > 0 {
            msgs.push(format!(
                "Retriable errors: [{submission_retriable_errors}]."
            ));
        }
        if submission_non_retriable_errors.total_stake > 0 {
            msgs.push(format!(
                "Non-retriable errors: [{submission_non_retriable_errors}]."
            ));
        }
        if !observed_effects_digests.digests.is_empty() {
            msgs.push(format!(
                "Observed effects digests: [{observed_effects_digests}]."
            ));
        }
        write!(f, "{}", msgs.join(" "))
    }

    fn display_invalid_transaction(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionDriverError::InvalidTransaction {
            submission_non_retriable_errors,
            submission_retriable_errors,
        } = self
        else {
            return Ok(());
        };
        let mut msgs = vec!["Transaction is rejected as invalid by more than 1/3 of validators by stake (non-retriable).".to_string()];
        msgs.push(format!(
            "Non-retriable errors: [{submission_non_retriable_errors}]."
        ));
        if submission_retriable_errors.total_stake > 0 {
            msgs.push(format!(
                "Retriable errors: [{submission_retriable_errors}]."
            ));
        }
        write!(f, "{}", msgs.join(" "))
    }

    fn display_forked_execution(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionDriverError::ForkedExecution {
            observed_effects_digests,
            submission_non_retriable_errors,
            submission_retriable_errors,
        } = self
        else {
            return Ok(());
        };
        let mut msgs =
            vec!["Transaction execution observed forked outputs (non-retriable).".to_string()];
        msgs.push(format!(
            "Observed effects digests: [{observed_effects_digests}]."
        ));
        if submission_non_retriable_errors.total_stake > 0 {
            msgs.push(format!(
                "Non-retriable errors: [{submission_non_retriable_errors}]."
            ));
        }
        if submission_retriable_errors.total_stake > 0 {
            msgs.push(format!(
                "Retriable errors: [{submission_retriable_errors}]."
            ));
        }
        write!(f, "{}", msgs.join(" "))
    }
}

impl std::fmt::Display for TransactionDriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionDriverError::Aborted { .. } => self.display_aborted(f),
            TransactionDriverError::InvalidTransaction { .. } => {
                self.display_invalid_transaction(f)
            }
            TransactionDriverError::ForkedExecution { .. } => self.display_forked_execution(f),
        }
    }
}

impl std::fmt::Debug for TransactionDriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::error::Error for TransactionDriverError {}

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

impl AggregatedEffectsDigests {
    #[cfg(test)]
    pub fn total_stake(&self) -> StakeUnit {
        self.digests.iter().map(|(_, _, stake)| stake).sum()
    }
}

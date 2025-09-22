// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use itertools::Itertools as _;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::{EpochId, StakeUnit},
    digests::TransactionEffectsDigest,
    error::{ErrorCategory, SuiError},
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
    #[error("{0}")]
    ValidatorInternal(String),

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
    pub(crate) fn categorize(&self) -> ErrorCategory {
        match self {
            TransactionRequestError::TimedOutSubmittingTransaction => ErrorCategory::Unavailable,
            TransactionRequestError::TimedOutGettingFullEffectsAtValidator => {
                ErrorCategory::Unavailable
            }
            TransactionRequestError::ValidatorInternal(_) => ErrorCategory::Internal,

            TransactionRequestError::RejectedAtValidator(error) => error.categorize(),
            TransactionRequestError::RejectedByConsensus => ErrorCategory::Aborted,
            TransactionRequestError::StatusExpired(_, _) => ErrorCategory::Aborted,
            TransactionRequestError::Aborted(error) => error.categorize(),
        }
    }

    pub(crate) fn is_submission_retriable(&self) -> bool {
        self.categorize().is_submission_retriable()
    }
}

/// Client facing errors on transaction processing via Transaction Driver.
///
/// NOTE: every error should indicate if it is retriable.
#[derive(Eq, PartialEq, Clone)]
pub enum TransactionDriverError {
    /// TransactionDriver encountered an internal error.
    /// Non-retriable.
    ClientInternal { error: String },
    /// The transaction failed validation from local state.
    /// Non-retriable.
    ValidationFailed { error: String },
    /// Transient failure during transaction processing that prevents the transaction from finalization.
    /// Retriable with new transaction submission.
    Aborted {
        submission_non_retriable_errors: AggregatedRequestErrors,
        submission_retriable_errors: AggregatedRequestErrors,
        observed_effects_digests: AggregatedEffectsDigests,
    },
    /// Over validity threshold of validators rejected the transaction as invalid.
    /// Non-retriable.
    RejectedByValidators {
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
    /// Transaction timed out but we return last retriable error if it exists.
    /// Non-retriable.
    TimeoutWithLastRetriableError {
        last_error: Option<Box<TransactionDriverError>>,
        attempts: u32,
        timeout: Duration,
    },
}

impl TransactionDriverError {
    pub(crate) fn is_submission_retriable(&self) -> bool {
        self.categorize().is_submission_retriable()
    }

    pub fn categorize(&self) -> ErrorCategory {
        match self {
            TransactionDriverError::ClientInternal { .. } => ErrorCategory::Internal,
            TransactionDriverError::ValidationFailed { .. } => ErrorCategory::InvalidTransaction,
            TransactionDriverError::Aborted {
                submission_retriable_errors,
                submission_non_retriable_errors,
                ..
            } => {
                if let Some((_, _, _, category)) = submission_retriable_errors.errors.first() {
                    *category
                } else if let Some((_, _, _, category)) =
                    submission_non_retriable_errors.errors.first()
                {
                    *category
                } else {
                    ErrorCategory::Aborted
                }
            }
            TransactionDriverError::RejectedByValidators {
                submission_non_retriable_errors,
                submission_retriable_errors,
                ..
            } => {
                if let Some((_, _, _, category)) = submission_non_retriable_errors.errors.first() {
                    *category
                } else if let Some((_, _, _, category)) = submission_retriable_errors.errors.first()
                {
                    *category
                } else {
                    // There should be at least one error.
                    ErrorCategory::Internal
                }
            }
            TransactionDriverError::ForkedExecution { .. } => ErrorCategory::Internal,
            TransactionDriverError::TimeoutWithLastRetriableError { .. } => {
                ErrorCategory::Unavailable
            }
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
        let mut msgs =
            vec!["Transaction processing aborted (retriable with another submission).".to_string()];
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

    fn display_validation_failed(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionDriverError::ValidationFailed { error } = self else {
            return Ok(());
        };
        write!(f, "Transaction failed validation: {}", error)
    }

    fn display_invalid_transaction(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionDriverError::RejectedByValidators {
            submission_non_retriable_errors,
            submission_retriable_errors,
        } = self
        else {
            return Ok(());
        };
        let mut msgs = vec!["Transaction is rejected as invalid by more than 1/3 of validators by stake (non-retriable).".to_string()];
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
            TransactionDriverError::ClientInternal { error } => {
                write!(f, "TransactionDriver internal error: {}", error)
            }
            TransactionDriverError::Aborted { .. } => self.display_aborted(f),
            TransactionDriverError::ValidationFailed { .. } => self.display_validation_failed(f),
            TransactionDriverError::RejectedByValidators { .. } => {
                self.display_invalid_transaction(f)
            }
            TransactionDriverError::ForkedExecution { .. } => self.display_forked_execution(f),
            TransactionDriverError::TimeoutWithLastRetriableError {
                last_error,
                attempts,
                timeout,
            } => {
                write!(
                    f,
                    "Transaction timed out after {} attempts. Timeout: {:?}. Last error: {}",
                    attempts,
                    timeout,
                    last_error
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_default()
                )
            }
        }
    }
}

impl std::fmt::Debug for TransactionDriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::error::Error for TransactionDriverError {}

#[derive(Eq, PartialEq, Clone, Debug, Default)]
pub struct AggregatedRequestErrors {
    pub errors: Vec<(String, Vec<AuthorityName>, StakeUnit, ErrorCategory)>,
    // The total stake of all errors.
    pub total_stake: StakeUnit,
}

impl std::fmt::Display for AggregatedRequestErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self
            .errors
            .iter()
            .map(|(error, names, stake, _category)| {
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

// TODO(fastpath): This is a temporary fix to unify the error message between QD and TD.
// Match special handling of UserInputError in sui-json-rpc/src/error.rs NonRecoverableTransactionError
fn format_transaction_request_error(error: &TransactionRequestError) -> String {
    match error {
        TransactionRequestError::RejectedAtValidator(sui_error) => match sui_error {
            SuiError::UserInputError { error: user_error } => user_error.to_string(),
            _ => sui_error.to_string(),
        },
        _ => error.to_string(),
    }
}

pub(crate) fn aggregate_request_errors(
    errors: Vec<(AuthorityName, StakeUnit, TransactionRequestError)>,
) -> AggregatedRequestErrors {
    let mut total_stake = 0;
    let mut aggregated_errors =
        BTreeMap::<String, (Vec<AuthorityName>, StakeUnit, ErrorCategory)>::new();

    for (name, stake, error) in errors {
        total_stake += stake;
        let key = format_transaction_request_error(&error);
        let entry = aggregated_errors
            .entry(key)
            .or_insert_with(|| (vec![], 0, error.categorize()));
        entry.0.push(name);
        entry.1 += stake;
    }

    let mut errors: Vec<_> = aggregated_errors
        .into_iter()
        .map(|(error, (names, stake, category))| (error, names, stake, category))
        .collect();
    errors.sort_by_key(|(_, _, stake, _)| std::cmp::Reverse(*stake));

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

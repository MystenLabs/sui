// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, sync::Arc};

use sui_types::base_types::AuthorityName;

use crate::{
    authority_aggregator::AuthorityAggregator,
    safe_client::SafeClient,
    status_aggregator::StatusAggregator,
    transaction_driver::error::{
        aggregate_request_errors, AggregatedEffectsDigests, TransactionDriverError,
        TransactionRequestError,
    },
    validator_client_monitor::{TxType, ValidatorClientMonitor},
};

/// Provides the next target validator to retry operations,
/// and gathers the errors along with the operations.
///
/// In TransactionDriver, submitting a transaction and getting full effects follow the same pattern:
/// 1. Retry against all validators until the operation succeeds.
/// 2. If non‑retriable errors from a quorum of validators are returned, the operation should fail permanently.
///
/// This component helps to manager this retry pattern.
pub(crate) struct RequestRetrier<A: Clone> {
    remaining_clients: VecDeque<(AuthorityName, Arc<SafeClient<A>>)>,
    pub(crate) non_retriable_errors_aggregator: StatusAggregator<TransactionRequestError>,
    pub(crate) retriable_errors_aggregator: StatusAggregator<TransactionRequestError>,
}

impl<A: Clone> RequestRetrier<A> {
    pub(crate) fn new(
        auth_agg: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        tx_type: TxType,
    ) -> Self {
        let selected_validators = client_monitor.select_shuffled_preferred_validators(
            &auth_agg.committee,
            auth_agg.committee.num_members() / 3,
            tx_type,
        );
        let remaining_clients = selected_validators
            .into_iter()
            .map(|name| (name, auth_agg.authority_clients[&name].clone()))
            .collect::<VecDeque<_>>();
        let non_retriable_errors_aggregator = StatusAggregator::new(auth_agg.committee.clone());
        let retriable_errors_aggregator = StatusAggregator::new(auth_agg.committee.clone());
        Self {
            remaining_clients,
            non_retriable_errors_aggregator,
            retriable_errors_aggregator,
        }
    }

    // Selects the next target validator to attempt an operation.
    pub(crate) fn next_target(
        &mut self,
    ) -> Result<(AuthorityName, Arc<SafeClient<A>>), TransactionDriverError> {
        if let Some((name, client)) = self.remaining_clients.pop_front() {
            return Ok((name, client));
        };

        if self
            .non_retriable_errors_aggregator
            .reached_validity_threshold()
        {
            Err(TransactionDriverError::InvalidTransaction {
                submission_non_retriable_errors: aggregate_request_errors(
                    self.non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    self.retriable_errors_aggregator.status_by_authority(),
                ),
            })
        } else {
            Err(TransactionDriverError::Aborted {
                submission_non_retriable_errors: aggregate_request_errors(
                    self.non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    self.retriable_errors_aggregator.status_by_authority(),
                ),
                observed_effects_digests: AggregatedEffectsDigests {
                    digests: Vec::new(),
                },
            })
        }
    }

    // Adds an error associated with the operation against the authority.
    //
    // Returns an error if it has aggregated >= f+1 submission non-retriable errors.
    // In this case, the transaction cannot finalize unless there is a software bug
    // or > f malicious validators.
    pub(crate) fn add_error(
        &mut self,
        name: AuthorityName,
        error: TransactionRequestError,
    ) -> Result<(), TransactionDriverError> {
        if error.is_submission_retriable() {
            self.retriable_errors_aggregator.insert(name, error);
        } else {
            self.non_retriable_errors_aggregator.insert(name, error);
            if self
                .non_retriable_errors_aggregator
                .reached_validity_threshold()
            {
                return Err(TransactionDriverError::InvalidTransaction {
                    submission_non_retriable_errors: aggregate_request_errors(
                        self.non_retriable_errors_aggregator.status_by_authority(),
                    ),
                    submission_retriable_errors: aggregate_request_errors(
                        self.retriable_errors_aggregator.status_by_authority(),
                    ),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sui_types::error::{SuiError, UserInputError};

    use crate::{
        authority_aggregator::{AuthorityAggregatorBuilder, TimeoutConfig},
        test_authority_clients::MockAuthorityApi,
    };

    use super::*;

    pub(crate) fn get_authority_aggregator(
        committee_size: usize,
    ) -> AuthorityAggregator<MockAuthorityApi> {
        let timeouts_config = TimeoutConfig::default();
        AuthorityAggregatorBuilder::from_committee_size(committee_size)
            .with_timeouts_config(timeouts_config)
            .build_mock_authority_aggregator()
    }

    #[tokio::test]
    async fn test_next_target() {
        let auth_agg = Arc::new(get_authority_aggregator(4));
        let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(auth_agg.clone()));
        let mut retrier = RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter);

        for _ in 0..4 {
            retrier.next_target().unwrap();
        }

        let Err(error) = retrier.next_target() else {
            panic!("Expected an error");
        };
        assert!(error.is_retriable());
    }

    #[tokio::test]
    async fn test_add_error() {
        let auth_agg = Arc::new(get_authority_aggregator(4));
        let authorities: Vec<_> = auth_agg.committee.names().copied().collect();

        // Add retriable errors.
        {
            let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(auth_agg.clone()));
            let mut retrier = RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter);

            // 25% stake.
            retrier
                .add_error(
                    authorities[0],
                    TransactionRequestError::TimedOutSubmittingTransaction,
                )
                .unwrap();
            // 50% stake.
            retrier
                .add_error(
                    authorities[1],
                    TransactionRequestError::TimedOutSubmittingTransaction,
                )
                .unwrap();
            // 75% stake.
            retrier
                .add_error(
                    authorities[1],
                    TransactionRequestError::TimedOutSubmittingTransaction,
                )
                .unwrap();
            // 100% stake.
            retrier
                .add_error(
                    authorities[1],
                    TransactionRequestError::TimedOutSubmittingTransaction,
                )
                .unwrap();
            // Still there is no aggregated error.
        }

        // Add mix of retriable and non-retriable errors.
        {
            let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(auth_agg.clone()));
            let mut retrier = RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter);

            // 25% stake retriable error.
            retrier
                .add_error(
                    authorities[0],
                    TransactionRequestError::TimedOutSubmittingTransaction,
                )
                .unwrap();
            // 25% stake non-retriable error.
            retrier
                .add_error(
                    authorities[1],
                    TransactionRequestError::RejectedAtValidator(SuiError::UserInputError {
                        error: UserInputError::EmptyCommandInput,
                    }),
                )
                .unwrap();
            // 50% stake non-retriable error. Above validity threshold.
            let aggregated_error = retrier
                .add_error(
                    authorities[2],
                    TransactionRequestError::RejectedAtValidator(SuiError::UserInputError {
                        error: UserInputError::EmptyCommandInput,
                    }),
                )
                .unwrap_err();
            // The aggregated error is non-retriable.
            assert!(!aggregated_error.is_retriable());
        }
    }
}

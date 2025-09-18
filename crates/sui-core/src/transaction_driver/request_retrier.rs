// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, sync::Arc};

use sui_types::{base_types::AuthorityName, messages_grpc::TxType};

use crate::{
    authority_aggregator::AuthorityAggregator,
    safe_client::SafeClient,
    status_aggregator::StatusAggregator,
    transaction_driver::error::{
        aggregate_request_errors, AggregatedEffectsDigests, TransactionDriverError,
        TransactionRequestError,
    },
    validator_client_monitor::ValidatorClientMonitor,
};

pub(crate) const TOP_K_VALIDATORS_DENOMINATOR: usize = 3;

/// Provides the next target validator to retry operations,
/// and gathers the errors along with the operations.
///
/// In TransactionDriver, submitting a transaction and getting full effects follow the same pattern:
/// 1. Retry against all validators until the operation succeeds.
/// 2. If non‑retriable errors from a quorum of validators are returned, the operation should fail permanently.
///
/// When an `allowed_validators` is provided, only the validators in the list will be used to submit the transaction to.
/// When the allowed validator list is empty, any validator can be used an then the validators are selected based on their scores.
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
        allowed_validators: Vec<AuthorityName>,
    ) -> Self {
        let selected_validators = if !allowed_validators.is_empty() {
            allowed_validators
        } else {
            client_monitor.select_shuffled_preferred_validators(
                &auth_agg.committee,
                auth_agg.committee.num_members() / TOP_K_VALIDATORS_DENOMINATOR,
                tx_type,
            )
        };
        let remaining_clients = selected_validators
            .into_iter()
            .filter_map(|name| {
                // There is not guarantee that the `selected_validators` are in the `auth_agg.authority_clients` if those are coming from the list
                // of `allowed_validators`, as the provided `auth_agg` might have been updated with a new committee that doesn't contain the validator in question.
                auth_agg
                    .authority_clients
                    .get(&name)
                    .map(|client| (name, client.clone()))
            })
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
        let mut retrier =
            RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter, vec![]);

        for _ in 0..4 {
            retrier.next_target().unwrap();
        }

        let Err(error) = retrier.next_target() else {
            panic!("Expected an error");
        };
        assert!(error.is_retriable());
    }

    #[tokio::test]
    async fn test_allowed_validators() {
        use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};

        let auth_agg = Arc::new(get_authority_aggregator(4));
        let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(auth_agg.clone()));

        // Create validators that don't exist in the auth_agg
        let (_, key_pair1): (_, AuthorityKeyPair) = get_key_pair();
        let (_, key_pair2): (_, AuthorityKeyPair) = get_key_pair();
        let unknown_validator1: AuthorityName = key_pair1.public().into();
        let unknown_validator2: AuthorityName = key_pair2.public().into();

        // Mix of unknown validators and one known validator
        let authorities: Vec<_> = auth_agg.committee.names().copied().collect();

        println!("Case 1. Mix of unknown validators and one known validator");
        {
            let allowed_validators = vec![
                unknown_validator1,
                unknown_validator2,
                authorities[0], // This one exists in auth_agg
            ];

            let retrier = RequestRetrier::new(
                &auth_agg,
                &client_monitor,
                TxType::SingleWriter,
                allowed_validators,
            );

            // Should only have 1 remaining client (the known validator)
            assert_eq!(retrier.remaining_clients.len(), 1);
            assert_eq!(retrier.remaining_clients[0].0, authorities[0]);
        }

        println!("Case 2. Only unknown validators are provided");
        {
            let allowed_validators = vec![unknown_validator1, unknown_validator2];

            let retrier = RequestRetrier::new(
                &auth_agg,
                &client_monitor,
                TxType::SingleWriter,
                allowed_validators,
            );

            // Should have no remaining clients since none of the allowed validators exist
            assert_eq!(retrier.remaining_clients.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_add_error() {
        let auth_agg = Arc::new(get_authority_aggregator(4));
        let authorities: Vec<_> = auth_agg.committee.names().copied().collect();

        // Add retriable errors.
        {
            let client_monitor = Arc::new(ValidatorClientMonitor::new_for_test(auth_agg.clone()));
            let mut retrier =
                RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter, vec![]);

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
            let mut retrier =
                RequestRetrier::new(&auth_agg, &client_monitor, TxType::SingleWriter, vec![]);

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

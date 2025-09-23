// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::stream::{FuturesUnordered, StreamExt};
use sui_types::{
    base_types::AuthorityName,
    error::ErrorCategory,
    messages_grpc::{SubmitTxRequest, SubmitTxResult, TxType},
};
use tokio::time::timeout;
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
    transaction_driver::{
        error::{
            aggregate_request_errors, AggregatedEffectsDigests, TransactionDriverError,
            TransactionRequestError,
        },
        request_retrier::RequestRetrier,
        SubmitTransactionOptions, TransactionDriverMetrics,
    },
    validator_client_monitor::{OperationFeedback, OperationType, ValidatorClientMonitor},
};

#[cfg(test)]
#[path = "unit_tests/transaction_submitter_tests.rs"]
mod transaction_submitter_tests;

// Using a long timeout for transaction submission is ok, because good performing validators
// are chosen first.
const SUBMIT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct TransactionSubmitter {
    metrics: Arc<TransactionDriverMetrics>,
}

impl TransactionSubmitter {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "debug", skip_all, err(level = "debug"))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        tx_type: TxType,
        amplification_factor: u64,
        request: SubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<(AuthorityName, SubmitTxResult), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let start_time = Instant::now();

        self.metrics
            .submit_amplification_factor
            .observe(amplification_factor as f64);

        let mut retrier = RequestRetrier::new(
            authority_aggregator,
            client_monitor,
            tx_type,
            options.allowed_validators.clone(),
        );
        let mut retries = 0;
        let mut request_rpcs = FuturesUnordered::new();

        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            // Try to fill up to amplification_factor concurrent requests
            while request_rpcs.len() < amplification_factor as usize {
                match retrier.next_target() {
                    Ok((name, client)) => {
                        let display_name = authority_aggregator.get_display_name(&name);
                        self.metrics
                            .validator_selections
                            .with_label_values(&[&display_name])
                            .inc();

                        // Create a future that returns the name and display_name along with the result
                        let submit_fut = self.submit_transaction_once(
                            client,
                            &request,
                            options,
                            client_monitor,
                            name,
                            display_name.clone(),
                        );

                        let wrapped_fut = async move {
                            let result = submit_fut.await;
                            (name, display_name, result)
                        };

                        request_rpcs.push(wrapped_fut);
                    }
                    Err(_) if request_rpcs.is_empty() => {
                        // No more targets and no requests in flight
                        return Err(TransactionDriverError::Aborted {
                            submission_non_retriable_errors: aggregate_request_errors(
                                retrier
                                    .non_retriable_errors_aggregator
                                    .status_by_authority(),
                            ),
                            submission_retriable_errors: aggregate_request_errors(
                                retrier.retriable_errors_aggregator.status_by_authority(),
                            ),
                            observed_effects_digests: AggregatedEffectsDigests {
                                digests: Vec::new(),
                            },
                        });
                    }
                    Err(_) => {
                        // No more targets but still have requests in flight
                        break;
                    }
                }
            }

            match request_rpcs.next().await {
                Some((name, display_name, Ok(result))) => {
                    self.metrics
                        .validator_submit_transaction_successes
                        .with_label_values(&[&display_name])
                        .inc();
                    self.metrics
                        .submit_transaction_retries
                        .observe(retries as f64);
                    let elapsed = start_time.elapsed().as_secs_f64();
                    self.metrics.submit_transaction_latency.observe(elapsed);

                    return Ok((name, result));
                }
                Some((name, display_name, Err(e))) => {
                    let error_type = if e.is_submission_retriable() {
                        "retriable"
                    } else {
                        "non_retriable"
                    };
                    self.metrics
                        .validator_submit_transaction_errors
                        .with_label_values(&[&display_name, error_type])
                        .inc();

                    retries += 1;
                    retrier.add_error(name, e)?;
                }
                None => {
                    // All requests have been processed.
                    return Err(TransactionDriverError::Aborted {
                        submission_non_retriable_errors: aggregate_request_errors(
                            retrier
                                .non_retriable_errors_aggregator
                                .status_by_authority(),
                        ),
                        submission_retriable_errors: aggregate_request_errors(
                            retrier.retriable_errors_aggregator.status_by_authority(),
                        ),
                        observed_effects_digests: AggregatedEffectsDigests {
                            digests: Vec::new(),
                        },
                    });
                }
            };

            // Yield to prevent this retry loop from starving other tasks.
            tokio::task::yield_now().await;
        }
    }

    #[instrument(level = "debug", skip_all, err(level = "debug"), ret, fields(validator_display_name = ?display_name))]
    pub(crate) async fn submit_transaction_once<A>(
        &self,
        client: Arc<SafeClient<A>>,
        request: &SubmitTxRequest,
        options: &SubmitTransactionOptions,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        validator: AuthorityName,
        display_name: String,
    ) -> Result<SubmitTxResult, TransactionRequestError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let submit_start = Instant::now();

        let resp = timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(request.clone(), options.forwarded_client_addr),
        )
        .await
        .map_err(|_| {
            client_monitor.record_interaction_result(OperationFeedback {
                authority_name: validator,
                display_name: display_name.clone(),
                operation: OperationType::Submit,
                result: Err(()),
            });
            TransactionRequestError::TimedOutSubmittingTransaction
        })?
        .map_err(|error| {
            if is_validator_error(error.categorize()) {
                client_monitor.record_interaction_result(OperationFeedback {
                    authority_name: validator,
                    display_name: display_name.clone(),
                    operation: OperationType::Submit,
                    result: Err(()),
                });
            }
            TransactionRequestError::RejectedAtValidator(error)
        })?;

        if resp.results.len() != 1 {
            return Err(TransactionRequestError::ValidatorInternal(format!(
                "Expected exactly 1 result, got {}",
                resp.results.len()
            )));
        }
        let result = resp.results.into_iter().next().unwrap();

        // Since only one transaction is submitted, it is ok to return error when the submission is rejected.
        if let SubmitTxResult::Rejected { error } = &result {
            if is_validator_error(error.categorize()) {
                client_monitor.record_interaction_result(OperationFeedback {
                    authority_name: validator,
                    display_name,
                    operation: OperationType::Submit,
                    result: Err(()),
                });
            }
            return Err(TransactionRequestError::RejectedAtValidator(error.clone()));
        }

        let latency = submit_start.elapsed();
        client_monitor.record_interaction_result(OperationFeedback {
            authority_name: validator,
            display_name,
            operation: OperationType::Submit,
            result: Ok(latency),
        });
        Ok(result)
    }
}

// Whether the failure is caused by the peer validator, as opposed to the user or this node.
fn is_validator_error(category: ErrorCategory) -> bool {
    matches!(
        category,
        ErrorCategory::Aborted
            | ErrorCategory::Internal
            | ErrorCategory::ValidatorOverloaded
            | ErrorCategory::Unavailable
    )
}

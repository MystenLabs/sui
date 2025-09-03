// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::join_all;
use sui_types::{
    base_types::AuthorityName, digests::TransactionDigest, error::SuiError,
    messages_grpc::RawSubmitTxRequest,
};
use tokio::time::timeout;
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
    transaction_driver::{
        error::{TransactionDriverError, TransactionRequestError},
        request_retrier::RequestRetrier,
        SubmitTransactionOptions, SubmitTxResult, TransactionDriverMetrics,
    },
    validator_client_monitor::{OperationFeedback, OperationType, ValidatorClientMonitor},
};

const SUBMIT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct TransactionSubmitter {
    metrics: Arc<TransactionDriverMetrics>,
}

impl TransactionSubmitter {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "debug", skip_all, err(level = "debug"), fields(tx_digest = ?_tx_digest))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        _tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<(AuthorityName, SubmitTxResult), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let start_time = Instant::now();

        // Check if concurrent submissions are requested
        if let Some(num_validators) = options.concurrent_validator_submissions {
            if num_validators > 0 {
                return self
                    .submit_transaction_concurrent(
                        authority_aggregator,
                        client_monitor,
                        _tx_digest,
                        raw_request,
                        options,
                        num_validators,
                        start_time,
                    )
                    .await;
            }
        }

        // Original single-validator submission logic
        let mut retrier = RequestRetrier::new(authority_aggregator, client_monitor);
        let mut retries = 0;

        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            let (name, client) = retrier.next_target()?;
            let display_name = authority_aggregator.get_display_name(&name);
            self.metrics
                .validator_selections
                .with_label_values(&[&display_name])
                .inc();
            match self
                .submit_transaction_once(
                    client,
                    &raw_request,
                    options,
                    client_monitor,
                    name,
                    display_name.clone(),
                )
                .await
            {
                Ok(result) => {
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
                Err(e) => {
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
            };
            // Yield to prevent this retry loop from starving other tasks under heavy load
            tokio::task::yield_now().await;
        }
    }

    #[instrument(level = "debug", skip_all, err(level = "debug"), ret, fields(validator_display_name = ?display_name))]
    async fn submit_transaction_once<A>(
        &self,
        client: Arc<SafeClient<A>>,
        raw_request: &RawSubmitTxRequest,
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
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
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
        // TODO: Note that we do not record this error in the client monitor
        // because it may be due to invalid transactions.
        // To fully utilize this error, we need to either pre-check the transaction
        // on the fullnode, or be able to categorize the error.
        .map_err(TransactionRequestError::RejectedAtValidator)?;

        let result = resp.results.into_iter().next().ok_or_else(|| {
            TransactionRequestError::Aborted(SuiError::GenericAuthorityError {
                error: "No result in SubmitTxResponse".to_string(),
            })
        })?;

        // Since only one transaction is submitted, it is ok to return error when the submission is rejected.
        if let SubmitTxResult::Rejected { error } = result {
            return Err(TransactionRequestError::RejectedAtValidator(error));
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

    /// Submit transaction to multiple validators concurrently for DOS protection testing.
    /// One submission is blocking to get consensus position, others are non-blocking.
    async fn submit_transaction_concurrent<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        _tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
        num_validators: usize,
        start_time: Instant,
    ) -> Result<(AuthorityName, SubmitTxResult), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        // Get multiple validators to submit to
        let selected_validators = client_monitor
            .select_shuffled_preferred_validators(&authority_aggregator.committee, num_validators);

        if selected_validators.is_empty() {
            return Err(TransactionDriverError::Aborted {
                submission_non_retriable_errors:
                    crate::transaction_driver::error::AggregatedRequestErrors {
                        errors: Vec::new(),
                        total_stake: 0,
                    },
                submission_retriable_errors:
                    crate::transaction_driver::error::AggregatedRequestErrors {
                        errors: Vec::new(),
                        total_stake: 0,
                    },
                observed_effects_digests:
                    crate::transaction_driver::error::AggregatedEffectsDigests {
                        digests: Vec::new(),
                    },
            });
        }

        // Create futures for all submissions
        let mut submission_futures = Vec::new();

        for (i, &validator_name) in selected_validators.iter().enumerate() {
            let client = authority_aggregator.authority_clients[&validator_name].clone();
            let display_name = authority_aggregator.get_display_name(&validator_name);
            let raw_request_clone = raw_request.clone();
            let options_clone = options.clone();
            let client_monitor_clone = client_monitor.clone();
            let metrics_clone = self.metrics.clone();

            // Record validator selection
            metrics_clone
                .validator_selections
                .with_label_values(&[&display_name])
                .inc();

            let future = async move {
                let result = Self::submit_transaction_once_internal(
                    client,
                    &raw_request_clone,
                    &options_clone,
                    &client_monitor_clone,
                    validator_name,
                    display_name.clone(),
                    &metrics_clone,
                )
                .await;

                (i, validator_name, display_name, result)
            };

            submission_futures.push(future);
        }

        // Submit to the first validator blocking to get consensus position
        let blocking_future = submission_futures.remove(0);
        let (_, blocking_validator, blocking_display_name, blocking_result) = blocking_future.await;

        // Handle the blocking result first
        match blocking_result {
            Ok(result) => {
                self.metrics
                    .validator_submit_transaction_successes
                    .with_label_values(&[&blocking_display_name])
                    .inc();
                let elapsed = start_time.elapsed().as_secs_f64();
                self.metrics.submit_transaction_latency.observe(elapsed);

                // Fire off non-blocking submissions to remaining validators
                if !submission_futures.is_empty() {
                    tokio::spawn(async move {
                        let _ = join_all(submission_futures).await;
                    });
                }

                return Ok((blocking_validator, result));
            }
            Err(e) => {
                let error_type = if e.is_submission_retriable() {
                    "retriable"
                } else {
                    "non_retriable"
                };
                self.metrics
                    .validator_submit_transaction_errors
                    .with_label_values(&[&blocking_display_name, error_type])
                    .inc();

                // If the blocking submission failed, we need to handle the error
                // For now, we'll return the error, but in a more sophisticated implementation
                // we might want to try the other submissions
                return Err(TransactionDriverError::Aborted {
                    submission_non_retriable_errors:
                        crate::transaction_driver::error::AggregatedRequestErrors {
                            errors: Vec::new(),
                            total_stake: 0,
                        },
                    submission_retriable_errors:
                        crate::transaction_driver::error::AggregatedRequestErrors {
                            errors: Vec::new(),
                            total_stake: 0,
                        },
                    observed_effects_digests:
                        crate::transaction_driver::error::AggregatedEffectsDigests {
                            digests: Vec::new(),
                        },
                });
            }
        }
    }

    /// Internal method for submitting transaction once, used by both single and concurrent submission.
    async fn submit_transaction_once_internal<A>(
        client: Arc<SafeClient<A>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        validator: AuthorityName,
        display_name: String,
        _metrics: &Arc<TransactionDriverMetrics>,
    ) -> Result<SubmitTxResult, TransactionRequestError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let submit_start = Instant::now();

        let resp = timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
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
        // TODO: Note that we do not record this error in the client monitor
        // because it may be due to invalid transactions.
        // To fully utilize this error, we need to either pre-check the transaction
        // on the fullnode, or be able to categorize the error.
        .map_err(TransactionRequestError::RejectedAtValidator)?;

        let result = resp.results.into_iter().next().ok_or_else(|| {
            TransactionRequestError::Aborted(SuiError::GenericAuthorityError {
                error: "No result in SubmitTxResponse".to_string(),
            })
        })?;

        // Since only one transaction is submitted, it is ok to return error when the submission is rejected.
        if let SubmitTxResult::Rejected { error } = result {
            return Err(TransactionRequestError::RejectedAtValidator(error));
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

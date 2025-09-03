// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

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

    #[instrument(level = "debug", skip_all, err(level = "debug"), fields(tx_digest = ?tx_digest))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<(AuthorityName, SubmitTxResult), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let start_time = Instant::now();
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
}

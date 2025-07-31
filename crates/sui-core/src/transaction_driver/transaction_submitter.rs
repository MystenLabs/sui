// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    digests::TransactionDigest,
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
        SubmitTransactionOptions, SubmitTxResponse, TransactionDriverMetrics,
    },
};

const SUBMIT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct TransactionSubmitter {
    metrics: Arc<TransactionDriverMetrics>,
}

impl TransactionSubmitter {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<(AuthorityName, SubmitTxResponse), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let start_time = Instant::now();
        let mut retrier = RequestRetrier::new(authority_aggregator);
        let mut retries = 0;

        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            let (name, client) = retrier.next_target()?;
            match self
                .submit_transaction_once(client, &raw_request, options)
                .await
            {
                Ok(resp) => {
                    // Track successful submission metrics
                    let display_name = authority_aggregator
                        .validator_display_names
                        .get(&name)
                        .unwrap_or(&name.concise().to_string())
                        .clone();
                    self.metrics
                        .validator_submit_transaction_successes
                        .with_label_values(&[&display_name])
                        .inc();

                    // Track retries needed for success
                    self.metrics
                        .submit_transaction_retries
                        .observe(retries as f64);

                    // Track latency
                    let elapsed = start_time.elapsed().as_secs_f64();
                    self.metrics.submit_transaction_latency.observe(elapsed);

                    return Ok((name, resp));
                }
                Err(e) => {
                    // Track error metrics with validator and error type
                    let error_type = if e.is_submission_retriable() {
                        "retriable"
                    } else {
                        "non_retriable"
                    };
                    let display_name = authority_aggregator
                        .validator_display_names
                        .get(&name)
                        .unwrap_or(&name.concise().to_string())
                        .clone();
                    self.metrics
                        .validator_submit_transaction_errors
                        .with_label_values(&[&display_name, error_type])
                        .inc();

                    retries += 1;
                    retrier.add_error(name, e)?;
                }
            };
            tokio::task::yield_now().await;
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn submit_transaction_once<A>(
        &self,
        client: Arc<SafeClient<A>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<SubmitTxResponse, TransactionRequestError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let resp = timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionRequestError::TimedOutSubmittingTransaction)?
        .map_err(TransactionRequestError::RejectedAtValidator)?;
        Ok(resp)
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use sui_types::{
    base_types::AuthorityName, digests::TransactionDigest, messages_grpc::RawSubmitTxRequest,
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

    #[instrument(level = "error", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<(AuthorityName, SubmitTxResponse), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut retrier = RequestRetrier::new(authority_aggregator, client_monitor);
        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            let (name, client) = retrier.next_target()?;
            match self
                .submit_transaction_once(client, &raw_request, options, client_monitor, name)
                .await
            {
                Ok(resp) => {
                    self.metrics.submit_transaction_success.inc();
                    return Ok((name, resp));
                }
                Err(e) => {
                    self.metrics.submit_transaction_error.inc();
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
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        validator: AuthorityName,
    ) -> Result<SubmitTxResponse, TransactionRequestError>
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
                validator,
                operation: OperationType::Submit,
                latency: None,
                success: false,
            });
            TransactionRequestError::TimedOutSubmittingTransaction
        })?
        // TODO: Note that we do not record this error in the client monitor
        // because it may be due to invalid transactions.
        // To fully utilize this error, we need to either pre-check the transaction
        // on the fullnode, or be able to categrize the error.
        .map_err(TransactionRequestError::RejectedAtValidator)?;

        let latency = submit_start.elapsed();
        client_monitor.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(latency),
            success: true,
        });
        Ok(resp)
    }
}

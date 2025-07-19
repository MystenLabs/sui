// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

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
        error::TransactionDriverError, transaction_retrier::TransactionRetrier,
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

    // TODO(fastpath): this should return an aggregated error from submission retries.
    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
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
        let mut retrier = TransactionRetrier::new(authority_aggregator);
        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            let (name, client) = retrier.next_target()?;
            match self
                .submit_transaction_once(client, &raw_request, options)
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

    #[instrument(level = "trace", skip_all)]
    async fn submit_transaction_once<A>(
        &self,
        client: Arc<SafeClient<A>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<SubmitTxResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let resp = timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionDriverError::TimedOutSubmittingTransaction)?
        .map_err(TransactionDriverError::ValidatorInternalError)?;
        Ok(resp)
    }
}

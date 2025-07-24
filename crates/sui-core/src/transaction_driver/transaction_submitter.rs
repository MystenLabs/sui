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
        error::{TransactionDriverError, TransactionRequestError},
        request_retrier::RequestRetrier,
        SubmitTransactionOptions, SubmitTxResponse, TransactionContext, TransactionDriverMetrics,
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
        txn_context: &mut TransactionContext,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
    ) -> Result<(AuthorityName, SubmitTxResponse), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut retrier = RequestRetrier::new(authority_aggregator);
        // This loop terminates when there are enough (f+1) non-retriable errors when submitting the transaction,
        // or all feasible targets returned errors or timed out.
        loop {
            let (name, client) = retrier.next_target(txn_context)?;
            match self
                .submit_transaction_once(client, &raw_request, &txn_context.options)
                .await
            {
                Ok(resp) => {
                    self.metrics.submit_transaction_success.inc();
                    return Ok((name, resp));
                }
                Err(e) => {
                    self.metrics.submit_transaction_error.inc();
                    retrier.add_error(txn_context, name, e)?;
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
        .map_err(TransactionRequestError::Rejected)?;
        Ok(resp)
    }
}

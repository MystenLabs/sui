// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::ConciseableName, digests::TransactionDigest, messages_consensus::ConsensusPosition,
    messages_grpc::RawSubmitTxRequest,
};
use tokio::time::timeout;
use tracing::{debug, instrument};

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    transaction_driver::{
        error::TransactionDriverError, SubmitTransactionOptions, TransactionDriverMetrics,
    },
};

const SUBMIT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct TransactionSubmitter {
    metrics: Arc<TransactionDriverMetrics>,
}

impl TransactionSubmitter {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut attempts = 0;
        // TODO(fastpath): Remove MAX_ATTEMPTS. Retry until f+1 permanent failures or cancellation.
        const MAX_ATTEMPTS: usize = 10;

        loop {
            attempts += 1;
            match self
                .submit_transaction_once(authority_aggregator, &raw_request, options)
                .await
            {
                Ok(consensus_position) => {
                    debug!(
                        "Transaction {tx_digest} submitted to consensus at position: {consensus_position:?}",
                    );
                    self.metrics.submit_transaction_success.inc();
                    return Ok(consensus_position);
                }
                Err(e) => {
                    self.metrics.submit_transaction_error.inc();
                    if attempts >= MAX_ATTEMPTS {
                        return Err(e);
                    }
                    tracing::warn!(
                        "Failed to submit transaction {tx_digest} (attempt {attempts}/{MAX_ATTEMPTS}): {e}",
                    );
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all)]
    async fn submit_transaction_once<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();
        let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

        let consensus_position = timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionDriverError::TimeoutSubmittingTransaction)?
        .map_err(|e| {
            TransactionDriverError::RpcFailure(name.concise().to_string(), e.to_string())
        })?;

        Ok(consensus_position)
    }
}

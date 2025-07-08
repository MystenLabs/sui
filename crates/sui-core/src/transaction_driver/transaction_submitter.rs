// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

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
    validator_client_monitor::{OperationFeedback, OperationType, ValidatorClientMonitor},
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
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
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
                .submit_transaction_once(
                    authority_aggregator,
                    client_monitor,
                    &raw_request,
                    options,
                )
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
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        // Select validator based on performance
        let selected_validator =
            client_monitor.select_preferred_validator(&authority_aggregator.committee);

        // Record selection in metrics
        self.metrics
            .validator_selections
            .with_label_values(&[&selected_validator.concise().to_string()])
            .inc();

        let client = authority_aggregator
            .authority_clients
            .get(&selected_validator)
            .expect("Selected validator not in authority clients")
            .as_ref();

        let submit_start = Instant::now();
        let consensus_position = match timeout(
            SUBMIT_TRANSACTION_TIMEOUT,
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        {
            Ok(Ok(pos)) => {
                let latency = submit_start.elapsed();
                client_monitor.record_interaction_result(OperationFeedback {
                    validator: selected_validator,
                    operation: OperationType::Submit,
                    latency: Some(latency),
                    success: true,
                });
                pos
            }
            Ok(Err(e)) => {
                let latency = submit_start.elapsed();
                client_monitor.record_interaction_result(OperationFeedback {
                    validator: selected_validator,
                    operation: OperationType::Submit,
                    latency: Some(latency),
                    // submit_transaction may return errors if the transaction is invalid,
                    // but we still want to record the feedback.
                    // TODO(mysticeti-fastpath): If we check transaction validity
                    // on the fullnode first, we can then utilize the error info.
                    success: true,
                });
                return Err(TransactionDriverError::RpcFailure(
                    selected_validator.concise().to_string(),
                    e.to_string(),
                ));
            }
            Err(_) => {
                // Timeout - don't include latency as it would pollute the numbers
                client_monitor.record_interaction_result(OperationFeedback {
                    validator: selected_validator,
                    operation: OperationType::Submit,
                    latency: None,
                    success: false,
                });
                return Err(TransactionDriverError::TimeoutSubmittingTransaction);
            }
        };

        Ok(consensus_position)
    }
}

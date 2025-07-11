// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    digests::TransactionDigest,
    error::SuiError,
    messages_consensus::ConsensusPosition,
    messages_grpc::RawSubmitTxRequest,
};
use tokio::time::timeout;
use tracing::{debug, instrument};

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    transaction_driver::{
        error::TransactionDriverError,
        error_categorizer::{ErrorCategorizationResult, ErrorCategorizer},
        SubmitTransactionOptions, TransactionDriverMetrics,
    },
};

const SUBMIT_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(2);

pub struct TransactionSubmitter {
    metrics: Arc<TransactionDriverMetrics>,
}

impl TransactionSubmitter {
    pub fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub async fn submit_transaction<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let committee = authority_aggregator.committee.clone();
        let mut error_categorizer = ErrorCategorizer::new(committee);
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();

        loop {
            // Try to submit to a random validator
            let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

            match self
                .submit_transaction_once(client, name, &raw_request, options)
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

                    // Convert TransactionDriverError to SuiError for categorization
                    let sui_error = match &e {
                        TransactionDriverError::RpcFailure(_, error_msg) => SuiError::RpcError {
                            error: error_msg.clone(),
                        },
                        TransactionDriverError::TimeoutSubmittingTransaction => {
                            SuiError::TimeoutError
                        }
                        _ => {
                            // For other errors, we'll treat them as non-retryable
                            tracing::warn!(
                                "Non-retryable error submitting transaction {tx_digest}: {e}",
                            );
                            return Err(e);
                        }
                    };

                    // Record the error in the categorizer
                    let categorization_result = error_categorizer.record_error(*name, sui_error);

                    match categorization_result {
                        ErrorCategorizationResult::Continue => {
                            // Continue retrying
                            tracing::debug!(
                                "Retryable error submitting transaction {tx_digest}, continuing...",
                            );
                        }
                        ErrorCategorizationResult::RetryableQuorumReached => {
                            // Retryable errors reached quorum, but we can still try
                            tracing::warn!(
                                "Retryable errors reached quorum for transaction {tx_digest}, but continuing...",
                            );
                        }
                        ErrorCategorizationResult::FatalQuorumReached => {
                            // Fatal condition reached, stop retrying
                            let error_state = error_categorizer.get_error_state();
                            tracing::error!(
                                "Fatal errors reached quorum for transaction {tx_digest}: {}",
                                error_state.summary()
                            );
                            return Err(TransactionDriverError::RpcFailure(
                                "Multiple validators".to_string(),
                                "Fatal errors reached quorum threshold".to_string(),
                            ));
                        }
                    }

                    // Check if we should continue retrying
                    if !error_categorizer.should_continue_retrying() {
                        let error_state = error_categorizer.get_error_state();
                        tracing::error!(
                            "Cannot reach quorum for transaction {tx_digest}: {}",
                            error_state.summary()
                        );
                        return Err(TransactionDriverError::RpcFailure(
                            "Multiple validators".to_string(),
                            "Cannot reach quorum due to errors".to_string(),
                        ));
                    }
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all)]
    async fn submit_transaction_once<A>(
        &self,
        client: &Arc<A>,
        name: &AuthorityName,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
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

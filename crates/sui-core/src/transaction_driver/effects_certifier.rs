// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{join, stream::FuturesUnordered, StreamExt as _};
use mysten_common::debug_fatal;
use rand::{seq::SliceRandom as _, Rng as _};
use sui_types::{
    base_types::ConciseableName,
    committee::EpochId,
    digests::{TransactionDigest, TransactionEffectsDigest},
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::{sleep, timeout};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    stake_aggregator::{InsertResult, StakeAggregator},
    transaction_driver::{
        error::TransactionDriverError, metrics::TransactionDriverMetrics,
        QuorumTransactionResponse, SubmitTransactionOptions,
    },
    transaction_driver::{ExecutedData, WaitForEffectsRequest, WaitForEffectsResponse},
};

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn get_certified_finalized_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let (acknowledgments_result, mut full_effects_result) = join!(
            self.wait_for_acknowledgments_with_retry(
                authority_aggregator,
                tx_digest,
                consensus_position,
                options,
            ),
            self.get_full_effects_with_retry(
                authority_aggregator,
                tx_digest,
                consensus_position,
                options,
            ),
        );
        let certified_digest = acknowledgments_result?;

        // Retry until full effects digest matches the certified digest.
        // TODO(fastpath): send backup requests to get full effects before timeout or failure.
        loop {
            match full_effects_result {
                Ok((effects_digest, executed_data)) => {
                    if effects_digest != certified_digest {
                        tracing::warn!(
                            "Full effects digest mismatch ({} vs certified {})",
                            effects_digest,
                            certified_digest
                        );
                    } else {
                        return Ok(self.get_effects_response(
                            effects_digest,
                            executed_data,
                            consensus_position.epoch,
                            tx_digest,
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get full effects: {e}");
                }
            };
            full_effects_result = self
                .get_full_effects_with_retry(
                    authority_aggregator,
                    tx_digest,
                    consensus_position,
                    options,
                )
                .await;
        }
    }

    async fn get_full_effects_with_retry<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, ExecutedData), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut attempts = 0;
        // TODO(fastpath): Remove MAX_ATTEMPTS. Retry until unretriable error.
        const MAX_ATTEMPTS: usize = 10;
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();

        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: *tx_digest,
            consensus_position,
            include_details: true,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        // TODO(fastpath): only retry transient (RPC) errors. aggregate permanent errors on a higher level.
        loop {
            attempts += 1;
            // TODO(fastpath): pick target with performance metrics.
            let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

            match timeout(
                WAIT_FOR_EFFECTS_TIMEOUT,
                client.wait_for_effects(raw_request.clone(), options.forwarded_client_addr),
            )
            .await
            {
                Ok(Ok(response)) => match response {
                    WaitForEffectsResponse::Executed {
                        effects_digest,
                        details,
                    } => {
                        // All error cases are retryable until max attempt due to the chance
                        // of the status being returned from a byzantine validator.
                        if let Some(details) = details {
                            return Ok((effects_digest, *details));
                        } else {
                            if attempts >= MAX_ATTEMPTS {
                                return Err(TransactionDriverError::ExecutionDataNotFound(
                                    tx_digest.to_string(),
                                ));
                            }
                            tracing::debug!("Execution data not found, retrying...");
                        }
                    }
                    WaitForEffectsResponse::Rejected { ref reason } => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionRejected(
                                reason.to_string(),
                            ));
                        }
                        tracing::debug!("Transaction rejected, retrying... Reason: {}", reason);
                    }
                    WaitForEffectsResponse::Expired { epoch, round } => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionStatusExpired);
                        }
                        tracing::debug!(
                            "Transaction status expired at epoch {}, round {}, retrying...",
                            epoch,
                            round.unwrap_or(0),
                        );
                    }
                },
                Ok(Err(e)) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::RpcFailure(
                            name.concise().to_string(),
                            e.to_string(),
                        ));
                    }
                    tracing::debug!(
                        "Full effects request failed from {}: {}, retrying...",
                        name.concise(),
                        e
                    );
                }
                Err(_) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::TimeoutGettingFullEffects);
                    }
                    tracing::debug!("Full effects request timed out, retrying...");
                }
            }
        }
    }

    async fn wait_for_acknowledgments_with_retry<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        options: &SubmitTransactionOptions,
    ) -> Result<TransactionEffectsDigest, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();
        let committee = authority_aggregator.committee.clone();
        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: *tx_digest,
            consensus_position,
            include_details: false,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        // Create futures for all validators (digest-only requests)
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let raw_request = raw_request.clone();
            let future = async move {
                // Keep retrying transient errors until cancellation.
                loop {
                    let result = timeout(
                        WAIT_FOR_EFFECTS_TIMEOUT,
                        client.wait_for_effects(raw_request.clone(), options.forwarded_client_addr),
                    )
                    .await;
                    match result {
                        Ok(Ok(response)) => {
                            return (name, response);
                        }
                        Ok(Err(e)) => {
                            tracing::trace!("Wait for effects acknowledgement: error: {:?}", e);
                        }
                        Err(_) => {
                            tracing::trace!("Wait for effects acknowledgement: timeout");
                        }
                    };
                    let delay_ms = rand::thread_rng().gen_range(1000..2000);
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            };

            futures.push(future);
        }

        let mut effects_digest_aggregators: HashMap<
            TransactionEffectsDigest,
            StakeAggregator<(), true>,
        > = HashMap::new();
        let mut rejected_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut expired_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut rejected_errors = Vec::new();
        let mut expired_errors = Vec::new();

        // Every validator returns at most one WaitForEffectsResponse.
        while let Some((name, response)) = futures.next().await {
            match response {
                WaitForEffectsResponse::Executed {
                    effects_digest,
                    details: _,
                } => {
                    let aggregator = effects_digest_aggregators
                        .entry(effects_digest)
                        .or_insert_with(|| StakeAggregator::<(), true>::new(committee.clone()));

                    match aggregator.insert_generic(name, ()) {
                        InsertResult::QuorumReached(_) => {
                            let quorum_weight = aggregator.total_votes();
                            for (other_digest, other_aggregator) in effects_digest_aggregators {
                                if other_digest != effects_digest
                                    && other_aggregator.total_votes() > 0
                                {
                                    tracing::warn!(
                                        "Effects digest inconsistency detected: quorum digest {effects_digest:?} (weight {quorum_weight}), other digest {other_digest:?} (weight {})",
                                        other_aggregator.total_votes()
                                    );
                                    self.metrics.effects_digest_mismatches.inc();
                                }
                            }
                            return Ok(effects_digest);
                        }
                        InsertResult::NotEnoughVotes { .. } => {}
                        InsertResult::Failed { error } => {
                            debug_fatal!(
                                "Failed to insert vote for digest {}: {:?}",
                                effects_digest,
                                error
                            );
                        }
                    }
                }
                WaitForEffectsResponse::Rejected { reason } => {
                    rejected_errors.push(format!("{}: {}", name.concise(), reason));
                    self.metrics.rejection_acks.inc();
                    if let InsertResult::Failed { error } =
                        rejected_aggregator.insert_generic(name, ())
                    {
                        debug_fatal!("Failed to insert rejection vote: {:?}", error);
                    }
                }
                WaitForEffectsResponse::Expired { epoch, round } => {
                    expired_errors.push(format!(
                        "{} at epoch {}, round {}",
                        name.concise(),
                        epoch,
                        round.unwrap_or(0),
                    ));
                    self.metrics.expiration_acks.inc();
                    if let InsertResult::Failed { error } =
                        expired_aggregator.insert_generic(name, ())
                    {
                        debug_fatal!("Failed to insert expiration vote: {:?}", error);
                    }
                }
            };

            let executed_weight: u64 = effects_digest_aggregators
                .values()
                .map(|agg| agg.total_votes())
                .sum();
            let rejected_weight = rejected_aggregator.total_votes();
            let expired_weight = expired_aggregator.total_votes();
            let total_weight = executed_weight + rejected_weight + expired_weight;

            if total_weight >= committee.quorum_threshold() {
                // Abort as early as possible because there is no guarantee that another response will be received.
                if rejected_weight + expired_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejectedOrExpired(
                        rejected_errors.join(", "),
                        expired_errors.join(", "),
                    ));
                }
                // Check if quorum can still be reached with remaining responses.
                let remaining_weight = committee.total_votes().saturating_sub(total_weight);
                let quorum_feasible = effects_digest_aggregators.values().any(|agg| {
                    agg.total_votes() + remaining_weight >= committee.quorum_threshold()
                });
                if !quorum_feasible {
                    break;
                }
            } else {
                // Abort less eagerly for clearer error message.
                // More responses are available when the network is live.
                if rejected_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejected(
                        rejected_errors.join(", "),
                    ));
                }
            }
        }

        // No quorum is reached or can be reached for any effects digest.
        let executed_weight: u64 = effects_digest_aggregators
            .values()
            .map(|agg| agg.total_votes())
            .sum();
        let rejected_weight = rejected_aggregator.total_votes();
        let expired_weight = expired_aggregator.total_votes();

        Err(TransactionDriverError::ForkedExecution {
            total_responses_weight: executed_weight + rejected_weight + expired_weight,
            executed_weight,
            rejected_weight,
            expired_weight,
            // TODO(fastpath): Aggregate and summarize forked effects and errors.
            errors: vec![],
        })
    }

    /// Creates the final full response.
    fn get_effects_response(
        &self,
        effects_digest: TransactionEffectsDigest,
        executed_data: ExecutedData,
        epoch: EpochId,
        tx_digest: &TransactionDigest,
    ) -> QuorumTransactionResponse {
        self.metrics.executed_transactions.inc();

        tracing::debug!("Transaction {tx_digest} executed with effects digest: {effects_digest}",);

        let details = FinalizedEffects {
            effects: executed_data.effects,
            finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
        };

        QuorumTransactionResponse {
            effects: details,
            events: executed_data.events,
            input_objects: if !executed_data.input_objects.is_empty() {
                Some(executed_data.input_objects)
            } else {
                None
            },
            output_objects: if !executed_data.output_objects.is_empty() {
                Some(executed_data.output_objects)
            } else {
                None
            },
            auxiliary_data: None,
        }
    }
}

// TODO(fastpath): Add tests for EffectsCertifier

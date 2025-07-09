// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::ConciseableName,
    committee::EpochId,
    digests::{TransactionDigest, TransactionEffectsDigest},
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::timeout;
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    stake_aggregator::{InsertResult, StakeAggregator},
    transaction_driver::{
        error::TransactionDriverError, metrics::TransactionDriverMetrics,
        QuorumSubmitTransactionResponse, SubmitTransactionOptions,
    },
    wait_for_effects_request::{ExecutedData, WaitForEffectsRequest, WaitForEffectsResponse},
};

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(2);

pub struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub async fn wait_for_quorum_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumSubmitTransactionResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut full_effects_future = Box::pin(self.get_full_effects_with_retry(
            authority_aggregator,
            tx_digest,
            consensus_position,
            epoch,
            options,
        ));

        let mut acknowledgments_future = Box::pin(self.wait_for_acknowledgments_with_retry(
            authority_aggregator,
            tx_digest,
            consensus_position,
            epoch,
            options,
        ));

        tokio::select! {
            full_effects_result = &mut full_effects_future => {
                match full_effects_result {
                    Ok((effects_digest, executed_data)) => {
                        // Full effects succeeded, now wait for acknowledgments
                        match acknowledgments_future.await {
                            Ok(confirmed_digest) => {
                                self.validate_effects_match(
                                    effects_digest,
                                    confirmed_digest,
                                    executed_data,
                                    epoch,
                                    tx_digest,
                                )
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => {
                        // Full effects failed, no need to wait for acknowledgments
                        Err(e)
                    }
                }
            }
            acknowledgments_result = &mut acknowledgments_future => {
                match acknowledgments_result {
                    Ok(confirmed_digest) => {
                        // Acknowledgments succeeded, now wait for full effects to get the digest
                        match full_effects_future.await {
                            Ok((effects_digest, executed_data)) => {
                                self.validate_effects_match(
                                    effects_digest,
                                    confirmed_digest,
                                    executed_data,
                                    epoch,
                                    tx_digest,
                                )
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => {
                        // Acknowledgments failed, no need to wait for full effects
                        Err(e)
                    }
                }
            }
        }
    }

    async fn get_full_effects_with_retry<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, ExecutedData), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;
        let clients = authority_aggregator
            .authority_clients
            .iter()
            .collect::<Vec<_>>();

        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            epoch,
            transaction_digest: *tx_digest,
            transaction_position: consensus_position,
            include_details: true,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        loop {
            attempts += 1;
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
                    WaitForEffectsResponse::Expired(round) => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionExpired(
                                round.to_string(),
                            ));
                        }
                        tracing::debug!("Transaction expired at round {}, retrying...", round);
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
        epoch: EpochId,
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
            epoch,
            transaction_digest: *tx_digest,
            transaction_position: consensus_position,
            include_details: false,
        })
        .map_err(TransactionDriverError::SerializationError)?;

        // Create futures for all validators (digest-only requests)
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client_clone = client.clone();
            let name_clone = *name;

            let raw_request_clone = raw_request.clone();
            let future = async move {
                match timeout(
                    WAIT_FOR_EFFECTS_TIMEOUT,
                    client_clone.wait_for_effects(raw_request_clone, options.forwarded_client_addr),
                )
                .await
                {
                    Ok(Ok(response)) => Ok((name_clone, response)),
                    Ok(Err(e)) => Err(TransactionDriverError::RpcFailure(
                        name_clone.concise().to_string(),
                        e.to_string(),
                    )),
                    Err(_) => Err(TransactionDriverError::TimeoutWaitingForEffects),
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
        let mut errors = Vec::new();

        while let Some(result) = futures.next().await {
            match result {
                Ok((name, response)) => match response {
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
                                tracing::warn!(
                                    "Failed to insert vote for digest {}: {:?}",
                                    effects_digest,
                                    error
                                );
                            }
                        }
                    }
                    WaitForEffectsResponse::Rejected { reason } => {
                        rejected_errors.push(format!("{}: {}", name.concise(), reason));

                        match rejected_aggregator.insert_generic(name, ()) {
                            InsertResult::QuorumReached(_) => {
                                self.metrics.rejected_transactions.inc();

                                return Err(TransactionDriverError::TransactionRejected(
                                    rejected_errors.join(", "),
                                ));
                            }
                            InsertResult::NotEnoughVotes { .. } => {
                                let expired_weight = expired_aggregator.total_votes();
                                let rejected_weight = rejected_aggregator.total_votes();
                                if expired_weight + rejected_weight >= committee.quorum_threshold()
                                {
                                    return Err(
                                        TransactionDriverError::TransactionRejectedAndExpired(
                                            rejected_errors.join(", "),
                                            expired_errors.join(", "),
                                        ),
                                    );
                                }
                            }
                            InsertResult::Failed { error } => {
                                tracing::warn!("Failed to insert rejection vote: {:?}", error);
                            }
                        }
                    }
                    WaitForEffectsResponse::Expired(round) => {
                        expired_errors.push(format!(
                            "{}: expired at round {}",
                            name.concise(),
                            round
                        ));

                        match expired_aggregator.insert_generic(name, ()) {
                            InsertResult::QuorumReached(_) => {
                                self.metrics.expired_transactions.inc();

                                return Err(TransactionDriverError::TransactionExpired(
                                    expired_errors.join(", "),
                                ));
                            }
                            InsertResult::NotEnoughVotes { .. } => {
                                let expired_weight = expired_aggregator.total_votes();
                                let rejected_weight = rejected_aggregator.total_votes();
                                if expired_weight + rejected_weight >= committee.quorum_threshold()
                                {
                                    return Err(
                                        TransactionDriverError::TransactionRejectedAndExpired(
                                            rejected_errors.join(", "),
                                            expired_errors.join(", "),
                                        ),
                                    );
                                }
                            }
                            InsertResult::Failed { error } => {
                                tracing::warn!("Failed to insert expiration vote: {:?}", error);
                            }
                        }
                    }
                },
                Err(e) => {
                    // TODO(fastpath): Categorize retryable errors and push to retry queue, store premanent failures. Exit on f+1 errors
                    errors.push(e);
                }
            }
        }

        // If we get here, we didn't reach quorum for any digest
        let executed_weight: u64 = effects_digest_aggregators
            .values()
            .map(|agg| agg.total_votes())
            .sum();
        let rejected_weight = rejected_aggregator.total_votes();
        let expired_weight = expired_aggregator.total_votes();

        Err(TransactionDriverError::InsufficientResponses {
            total_responses_weight: executed_weight + rejected_weight + expired_weight,
            executed_weight,
            rejected_weight,
            expired_weight,
            // TODO(fastpath): Aggregate and summarize errors
            errors: errors.into_iter().map(|e| e.to_string()).collect(),
        })
    }

    /// Create the final response after validating effects digest match
    fn validate_effects_match(
        &self,
        effects_digest: TransactionEffectsDigest,
        confirmed_digest: TransactionEffectsDigest,
        executed_data: ExecutedData,
        epoch: EpochId,
        tx_digest: &TransactionDigest,
    ) -> Result<QuorumSubmitTransactionResponse, TransactionDriverError> {
        if effects_digest == confirmed_digest {
            self.metrics.executed_transactions.inc();

            tracing::info!(
                "Transaction {tx_digest} executed with effects digest: {effects_digest}",
            );

            let details = FinalizedEffects {
                effects: executed_data.effects,
                finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
            };

            Ok(QuorumSubmitTransactionResponse {
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
            })
        } else {
            Err(TransactionDriverError::EffectsDigestMismatch {
                quorum_expected: effects_digest.to_string(),
                actual: confirmed_digest.to_string(),
            })
        }
    }
}

// TODO(fastpath): Add tests for EffectsCertifier

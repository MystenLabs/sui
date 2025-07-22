// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures::{join, stream::FuturesUnordered, StreamExt as _};
use itertools::Itertools as _;
use rand::Rng as _;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::EpochId,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::TransactionEffectsAPI as _,
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::{sleep, timeout};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
    status_aggregator::StatusAggregator,
    transaction_driver::{
        error::TransactionDriverError, metrics::TransactionDriverMetrics,
        transaction_retrier::TransactionRetrier, ExecutedData, QuorumTransactionResponse,
        RejectReason, SubmitTransactionOptions, SubmitTxResponse, WaitForEffectsRequest,
        WaitForEffectsResponse,
    },
};

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    // TODO(fastpath): this should return an aggregated error from effects certification or
    // retries to get full effects.
    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn get_certified_finalized_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        // This keeps track of the current target for getting full effects.
        mut current_target: AuthorityName,
        submit_txn_resp: SubmitTxResponse,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        // When consensus position is provided, wait for finalized and fastpath outputs at the validators' side.
        // Otherwise, only wait for finalized effects.
        // Skip the first attempt to get full effects if it is already provided.
        let (consensus_position, full_effects) = match submit_txn_resp {
            SubmitTxResponse::Submitted { consensus_position } => (Some(consensus_position), None),
            SubmitTxResponse::Executed {
                effects_digest,
                details,
            } => match details {
                Some(details) => (None, Some((effects_digest, details))),
                // Details should always be set in correct responses.
                // But if it is not set, continuing to get full effects and certify the digest are still correct.
                None => (None, None),
            },
        };

        let mut retrier = TransactionRetrier::new(authority_aggregator);

        let (acknowledgments_result, mut full_effects_result) = join!(
            self.wait_for_acknowledgments(
                authority_aggregator,
                tx_digest,
                consensus_position,
                options,
            ),
            async {
                // No need to send a full effects request if it is already provided.
                if let Some(full_effects) = full_effects {
                    // In this branch, current_target is the authority providing the full effects,
                    // so it is consistent. This is not used though because current_target is
                    // only used with failed full effects query.
                    return Ok(full_effects);
                }
                let (name, client) = retrier.next_target()?;
                current_target = name;
                self.get_full_effects(client, tx_digest, consensus_position, options)
                    .await
            },
        );

        // If the consensus position got rejected, effects certification will see the failure and gather
        // error messages to explain the rejection.
        let certified_digest = acknowledgments_result?;

        // Retry until there is a valid full effects that matches the certified digest, or all targets
        // have been attempted.
        //
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
                        return Ok(self.get_quorum_transaction_response(
                            effects_digest,
                            executed_data,
                            tx_digest,
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get full effects: {e}");
                    // This emits an error when retrier gathers enough (f+1) non-retriable effects errors,
                    // but the error should not happen after effects certification unless there are software bugs
                    // or > f malicious validators.
                    retrier.add_error(current_target, e)?;
                }
            };

            tokio::task::yield_now().await;

            // Retry getting full effects from the next target.

            // This emits an error when retrier has no targets available.
            let (name, client) = retrier.next_target()?;
            current_target = name;
            full_effects_result = self
                .get_full_effects(client, tx_digest, consensus_position, options)
                .await;
        }
    }

    async fn get_full_effects<A>(
        &self,
        client: Arc<SafeClient<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: Option<ConsensusPosition>,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, Box<ExecutedData>), TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
            transaction_digest: *tx_digest,
            consensus_position,
            include_details: true,
        })
        .unwrap();

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
                    if let Some(details) = details {
                        Ok((effects_digest, details))
                    } else {
                        tracing::debug!("Execution data not found, retrying...");
                        Err(TransactionDriverError::ExecutionDataNotFound(
                            tx_digest.to_string(),
                        ))
                    }
                }
                WaitForEffectsResponse::Rejected { ref reason } => Err(
                    TransactionDriverError::TransactionRejectedAtValidator(reason.to_string()),
                ),
                WaitForEffectsResponse::Expired { epoch, round } => Err(
                    TransactionDriverError::TransactionStatusExpired(epoch, round.unwrap_or(0)),
                ),
            },
            Ok(Err(e)) => Err(TransactionDriverError::ValidatorInternalError(e)),
            Err(_) => Err(TransactionDriverError::TimedOutGettingFullEffectsAtValidator),
        }
    }

    async fn wait_for_acknowledgments<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: Option<ConsensusPosition>,
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
        .unwrap();

        // Create futures for all validators (digest-only requests)
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let raw_request = raw_request.clone();
            let future = async move {
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

        let mut effects_digest_aggregators: BTreeMap<
            TransactionEffectsDigest,
            StatusAggregator<()>,
        > = BTreeMap::new();
        let mut rejected_aggregator = StatusAggregator::<RejectReason>::new(committee.clone());
        let mut expired_aggregator = StatusAggregator::<(EpochId, u32)>::new(committee.clone());

        // Every validator returns at most one WaitForEffectsResponse.
        while let Some((name, response)) = futures.next().await {
            match response {
                WaitForEffectsResponse::Executed {
                    effects_digest,
                    details: _,
                } => {
                    let aggregator = effects_digest_aggregators
                        .entry(effects_digest)
                        .or_insert_with(|| StatusAggregator::<()>::new(committee.clone()));
                    aggregator.insert(name, ());
                    if aggregator.reached_quorum_threshold() {
                        let quorum_weight = aggregator.total_votes();
                        for (other_digest, other_aggregator) in effects_digest_aggregators {
                            if other_digest != effects_digest && other_aggregator.total_votes() > 0
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
                }
                WaitForEffectsResponse::Rejected { reason } => {
                    rejected_aggregator.insert(name, reason);
                    self.metrics.rejection_acks.inc();
                }
                WaitForEffectsResponse::Expired { epoch, round } => {
                    expired_aggregator.insert(name, (epoch, round.unwrap_or(0)));
                    self.metrics.expiration_acks.inc();
                }
            };

            // Adding vote up between different StatusAggregators without de-duplication is ok,
            // because each authority only returns one response.
            let executed_weight: u64 = effects_digest_aggregators
                .values()
                .map(|agg| agg.total_votes())
                .sum();
            let rejected_weight = rejected_aggregator.total_votes();
            let expired_weight = expired_aggregator.total_votes();
            let total_weight = executed_weight + rejected_weight + expired_weight;

            // Quorum threshold is used here to gather as many responses as possible for summary,
            // while making sure the loop can exit with f malicious peers.
            if total_weight >= committee.quorum_threshold() {
                // TransactionRejected status may or may not be submission retriable.
                if rejected_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejected(
                        rejected_weight,
                        rejected_aggregator
                            .statuses()
                            .iter()
                            .map(|(n, s)| format!("{}: {:?}", n.concise(), s))
                            .join(", "),
                    ));
                }
                // TransactionRejectedOrExpired status is always submission retriable.
                if rejected_weight + expired_weight >= committee.validity_threshold() {
                    return Err(TransactionDriverError::TransactionRejectedOrExpired(
                        rejected_weight,
                        expired_weight,
                        rejected_aggregator
                            .statuses()
                            .iter()
                            .map(|(n, s)| format!("{}: {:?}", n.concise(), s))
                            .join(", "),
                    ));
                }
            }
        }

        // At this point, no effects digest has reached quorum. But there is not a validity threshold
        // of failed responses either.
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
    fn get_quorum_transaction_response(
        &self,
        effects_digest: TransactionEffectsDigest,
        executed_data: Box<ExecutedData>,
        tx_digest: &TransactionDigest,
    ) -> QuorumTransactionResponse {
        self.metrics.executed_transactions.inc();

        tracing::debug!("Transaction {tx_digest} executed with effects digest: {effects_digest}",);

        let epoch = executed_data.effects.executed_epoch();
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

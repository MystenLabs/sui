// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{join, stream::FuturesUnordered, StreamExt as _};
use mysten_common::debug_fatal;
use sui_types::{
    base_types::AuthorityName,
    committee::StakeUnit,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::TransactionEffectsAPI as _,
    error::SuiError,
    messages_consensus::ConsensusPosition,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::time::{sleep, timeout};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    safe_client::SafeClient,
    status_aggregator::StatusAggregator,
    transaction_driver::{
        error::{
            aggregate_request_errors, AggregatedEffectsDigests, TransactionDriverError,
            TransactionRequestError,
        },
        metrics::TransactionDriverMetrics,
        request_retrier::RequestRetrier,
        ExecutedData, QuorumTransactionResponse, SubmitTransactionOptions, SubmitTxResponse,
        WaitForEffectsRequest, WaitForEffectsResponse,
    },
    validator_client_monitor::{OperationFeedback, OperationType, ValidatorClientMonitor},
};

#[cfg(test)]
#[path = "unit_tests/effects_certifier_tests.rs"]
mod effects_certifier_tests;

const WAIT_FOR_EFFECTS_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct EffectsCertifier {
    metrics: Arc<TransactionDriverMetrics>,
}

impl EffectsCertifier {
    pub(crate) fn new(metrics: Arc<TransactionDriverMetrics>) -> Self {
        Self { metrics }
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?tx_digest))]
    pub(crate) async fn get_certified_finalized_effects<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
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

        let mut retrier = RequestRetrier::new(authority_aggregator, client_monitor);

        // Setting this to None at first because if the full effects are already provided,
        // we do not need to record the latency. We track the time in this function instead of inside
        // get_full_effects so that we could record differently depending on whether the result is byzantine.
        let mut full_effects_start_time = None;
        let (acknowledgments_result, mut full_effects_result) = join!(
            self.wait_for_acknowledgments(
                authority_aggregator,
                client_monitor,
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
                let (name, client) = retrier
                    .next_target()
                    .expect("there should be at least 1 target");
                current_target = name;
                full_effects_start_time = Some(Instant::now());
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
            let display_name = authority_aggregator.get_display_name(&current_target);
            match full_effects_result {
                Ok((effects_digest, executed_data)) => {
                    if effects_digest != certified_digest {
                        tracing::warn!(
                            ?current_target,
                            "Full effects digest mismatch ({} vs certified {})",
                            effects_digest,
                            certified_digest
                        );
                        // This validator is byzantine, record the error.
                        client_monitor.record_interaction_result(OperationFeedback {
                            authority_name: current_target,
                            display_name,
                            operation: OperationType::Effects,
                            result: Err(()),
                        });
                    } else {
                        if let Some(start_time) = full_effects_start_time {
                            let latency = start_time.elapsed();
                            client_monitor.record_interaction_result(OperationFeedback {
                                authority_name: current_target,
                                display_name,
                                operation: OperationType::Effects,
                                result: Ok(latency),
                            });
                        }
                        return Ok(
                            self.get_quorum_transaction_response(effects_digest, executed_data)
                        );
                    }
                }
                Err(e) => {
                    tracing::debug!(?current_target, "Failed to get full effects: {e}");
                    client_monitor.record_interaction_result(OperationFeedback {
                        authority_name: current_target,
                        display_name,
                        operation: OperationType::Effects,
                        result: Err(()),
                    });
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
            full_effects_start_time = Some(Instant::now());
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
    ) -> Result<(TransactionEffectsDigest, Box<ExecutedData>), TransactionRequestError>
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
                        Err(TransactionRequestError::ExecutionDataNotFound)
                    }
                }
                WaitForEffectsResponse::Rejected { error } => match error {
                    Some(e) => Err(TransactionRequestError::RejectedAtValidator(e)),
                    // Even though this response is not an error, returning an error which is required
                    // by the function signature. This will get ignored by the caller as a retriable error.
                    None => Err(TransactionRequestError::RejectedByConsensus),
                },
                WaitForEffectsResponse::Expired { epoch, round } => Err(
                    TransactionRequestError::StatusExpired(epoch, round.unwrap_or(0)),
                ),
            },
            Ok(Err(e)) => Err(TransactionRequestError::Aborted(e)),
            Err(_) => Err(TransactionRequestError::TimedOutGettingFullEffectsAtValidator),
        }
    }

    async fn wait_for_acknowledgments<A>(
        &self,
        authority_aggregator: &Arc<AuthorityAggregator<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        tx_digest: &TransactionDigest,
        consensus_position: Option<ConsensusPosition>,
        options: &SubmitTransactionOptions,
    ) -> Result<TransactionEffectsDigest, TransactionDriverError>
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        self.metrics.certified_effects_ack_attempts.inc();
        let timer = tokio::time::Instant::now();
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

        // Broadcast requests for digest acknowledgments against all validators.
        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let display_name = authority_aggregator.get_display_name(&name);

            let raw_request = raw_request.clone();
            let future = async move {
                match timeout(
                    WAIT_FOR_EFFECTS_TIMEOUT,
                    self.wait_for_acknowledgment_rpc(
                        name,
                        display_name.clone(),
                        &client,
                        client_monitor,
                        &raw_request,
                        options,
                    ),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        client_monitor.record_interaction_result(OperationFeedback {
                            authority_name: name,
                            display_name,
                            operation: OperationType::Effects,
                            result: Err(()),
                        });
                        (name, Err(SuiError::TimeoutError))
                    }
                }
            };

            futures.push(future);
        }

        let mut effects_digest_aggregators: BTreeMap<
            TransactionEffectsDigest,
            StatusAggregator<()>,
        > = BTreeMap::new();
        // Collect errors non-retriable even with new transaction submission.
        let mut non_retriable_errors_aggregator =
            StatusAggregator::<TransactionRequestError>::new(committee.clone());
        // Collect errors retriable with new transaction submission.
        let mut retriable_errors_aggregator =
            StatusAggregator::<TransactionRequestError>::new(committee.clone());
        // Collect validators that voted to accept the transaction, even though these validators believe
        // consensus has rejected it.
        let mut accept_aggregator = StatusAggregator::<()>::new(committee.clone());

        // Every validator returns at most one WaitForEffectsResponse.
        while let Some((name, response)) = futures.next().await {
            match response {
                Ok(WaitForEffectsResponse::Executed {
                    effects_digest,
                    details: _,
                }) => {
                    let aggregator = effects_digest_aggregators
                        .entry(effects_digest)
                        .or_insert_with(|| StatusAggregator::<()>::new(committee.clone()));
                    aggregator.insert(name, ());
                    if aggregator.reached_quorum_threshold() {
                        let quorum_weight = aggregator.total_votes();
                        for (other_digest, other_aggregator) in effects_digest_aggregators {
                            if other_digest != effects_digest && other_aggregator.total_votes() > 0
                            {
                                tracing::warn!(?name,
                                    "Effects digest inconsistency detected: quorum digest {effects_digest:?} (weight {quorum_weight}), other digest {other_digest:?} (weight {})",
                                    other_aggregator.total_votes()
                                );
                                self.metrics.effects_digest_mismatches.inc();
                            }
                        }
                        // Record success and latency
                        self.metrics.certified_effects_ack_successes.inc();
                        self.metrics
                            .certified_effects_ack_latency
                            .observe(timer.elapsed().as_secs_f64());
                        return Ok(effects_digest);
                    }
                }
                Ok(WaitForEffectsResponse::Rejected { error }) => {
                    if let Some(e) = error {
                        let error = TransactionRequestError::RejectedAtValidator(e);
                        if error.is_submission_retriable() {
                            retriable_errors_aggregator.insert(name, error);
                        } else {
                            non_retriable_errors_aggregator.insert(name, error);
                        }
                    } else {
                        accept_aggregator.insert(name, ());
                    }
                    self.metrics.rejection_acks.inc();
                }
                Ok(WaitForEffectsResponse::Expired { epoch, round }) => {
                    let error = TransactionRequestError::StatusExpired(epoch, round.unwrap_or(0));
                    // Expired status is submission retriable.
                    retriable_errors_aggregator.insert(name, error);
                    self.metrics.expiration_acks.inc();
                }
                Err(error) => {
                    let error = TransactionRequestError::Aborted(error);
                    if error.is_submission_retriable() {
                        retriable_errors_aggregator.insert(name, error);
                    } else {
                        non_retriable_errors_aggregator.insert(name, error);
                    }
                }
            };

            // Adding vote up between different StatusAggregators without de-duplication is ok,
            // because each authority only returns one response.
            let executed_weight: u64 = effects_digest_aggregators
                .values()
                .map(|agg| agg.total_votes())
                .sum();
            let total_weight = executed_weight
                + accept_aggregator.total_votes()
                + non_retriable_errors_aggregator.total_votes()
                + retriable_errors_aggregator.total_votes();

            // Wait for a quorum of responses, to not summarize the responses too early.
            if total_weight >= committee.quorum_threshold() {
                // If neither of the branches below can exit the loop, the loop will eventually terminate
                // when responses are gathered from all validators. The time is bounded by WAIT_FOR_EFFECTS_TIMEOUT.
                if non_retriable_errors_aggregator.total_votes() >= committee.validity_threshold() {
                    return Err(TransactionDriverError::InvalidTransaction {
                        submission_non_retriable_errors: aggregate_request_errors(
                            non_retriable_errors_aggregator.status_by_authority(),
                        ),
                        submission_retriable_errors: aggregate_request_errors(
                            retriable_errors_aggregator.status_by_authority(),
                        ),
                    });
                } else if non_retriable_errors_aggregator.total_votes()
                    + retriable_errors_aggregator.total_votes()
                    >= committee.validity_threshold()
                {
                    let mut observed_effects_digests =
                        Vec::<(TransactionEffectsDigest, Vec<AuthorityName>, StakeUnit)>::new();
                    for (effects_digest, aggregator) in effects_digest_aggregators {
                        observed_effects_digests.push((
                            effects_digest,
                            aggregator.authorities(),
                            aggregator.total_votes(),
                        ));
                    }
                    return Err(TransactionDriverError::Aborted {
                        submission_non_retriable_errors: aggregate_request_errors(
                            non_retriable_errors_aggregator.status_by_authority(),
                        ),
                        submission_retriable_errors: aggregate_request_errors(
                            retriable_errors_aggregator.status_by_authority(),
                        ),
                        observed_effects_digests: AggregatedEffectsDigests {
                            digests: observed_effects_digests,
                        },
                    });
                }
            }
        }

        // At this point, no effects digest has reached quorum. But failed responses do not reach
        // validity threshold either.
        let retriable_and_accept_weight =
            retriable_errors_aggregator.total_votes() + accept_aggregator.total_votes();
        // Whether the transaction is retriable regardless of known effects.
        let mut submission_retriable = retriable_and_accept_weight >= committee.quorum_threshold();
        let mut observed_effects_digests =
            Vec::<(TransactionEffectsDigest, Vec<AuthorityName>, StakeUnit)>::new();
        for (effects_digest, aggregator) in effects_digest_aggregators {
            // This effects digest can still get certified, so the transaction is retriable.
            if aggregator.total_votes() + retriable_and_accept_weight
                >= committee.quorum_threshold()
            {
                submission_retriable = true;
            }
            observed_effects_digests.push((
                effects_digest,
                aggregator.authorities(),
                aggregator.total_votes(),
            ));
        }
        if submission_retriable {
            Err(TransactionDriverError::Aborted {
                submission_non_retriable_errors: aggregate_request_errors(
                    non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    retriable_errors_aggregator.status_by_authority(),
                ),
                observed_effects_digests: AggregatedEffectsDigests {
                    digests: observed_effects_digests,
                },
            })
        } else {
            if observed_effects_digests.len() <= 1 {
                debug_fatal!(
                    "Expect at least 2 effects digests, but got {:?}",
                    observed_effects_digests
                );
            }
            Err(TransactionDriverError::ForkedExecution {
                observed_effects_digests: AggregatedEffectsDigests {
                    digests: observed_effects_digests,
                },
                submission_non_retriable_errors: aggregate_request_errors(
                    non_retriable_errors_aggregator.status_by_authority(),
                ),
                submission_retriable_errors: aggregate_request_errors(
                    retriable_errors_aggregator.status_by_authority(),
                ),
            })
        }
    }

    async fn wait_for_acknowledgment_rpc<A>(
        &self,
        name: AuthorityName,
        display_name: String,
        client: &Arc<SafeClient<A>>,
        client_monitor: &Arc<ValidatorClientMonitor<A>>,
        raw_request: &RawWaitForEffectsRequest,
        options: &SubmitTransactionOptions,
    ) -> (AuthorityName, Result<WaitForEffectsResponse, SuiError>)
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let effects_start = Instant::now();
        let backoff = ExponentialBackoff::from_millis(100)
            .max_delay(Duration::from_secs(2))
            .map(jitter);
        // This loop should only retry errors that are retriable without new submission.
        for (attempt, delay) in backoff.enumerate() {
            let result = client
                .wait_for_effects(raw_request.clone(), options.forwarded_client_addr)
                .await;
            match result {
                Ok(response) => {
                    let latency = effects_start.elapsed();
                    client_monitor.record_interaction_result(OperationFeedback {
                        authority_name: name,
                        display_name: display_name.clone(),
                        operation: OperationType::Effects,
                        result: Ok(latency),
                    });
                    return (name, Ok(response));
                }
                Err(e) => {
                    client_monitor.record_interaction_result(OperationFeedback {
                        authority_name: name,
                        display_name: display_name.clone(),
                        operation: OperationType::Effects,
                        result: Err(()),
                    });
                    if !matches!(e, SuiError::RpcError(_, _)) {
                        return (name, Err(e));
                    }
                    tracing::trace!(
                        ?name,
                        "Wait for effects acknowledgement (attempt {attempt}): rpc error: {:?}",
                        e
                    );
                }
            };
            sleep(delay).await;
        }
        (name, Err(SuiError::TimeoutError))
    }

    /// Creates the final full response.
    fn get_quorum_transaction_response(
        &self,
        effects_digest: TransactionEffectsDigest,
        executed_data: Box<ExecutedData>,
    ) -> QuorumTransactionResponse {
        self.metrics.executed_transactions.inc();

        tracing::debug!("Transaction executed with effects digest: {effects_digest}",);

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

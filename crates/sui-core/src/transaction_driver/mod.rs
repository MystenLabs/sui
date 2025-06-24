// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod error;
mod message_types;
mod metrics;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::{ArcSwap, Guard};
pub use error::*;
pub use message_types::*;
pub use metrics::*;
use mysten_metrics::{monitored_future, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::ConciseableName,
    committee::{CommitteeTrait, EpochId},
    messages_consensus::ConsensusPosition,
    messages_grpc::{RawSubmitTxRequest, RawWaitForEffectsRequest},
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
    transaction::Transaction,
};
use tokio::{
    task::JoinSet,
    time::{sleep, timeout},
};
use tracing::{debug, instrument};

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    quorum_driver::{reconfig_observer::ReconfigObserver, AuthorityAggregatorUpdatable},
    wait_for_effects_request::{WaitForEffectsRequest, WaitForEffectsResponse},
};

/// Options for submitting a transaction.
#[derive(Clone, Default, Debug)]
pub struct SubmitTransactionOptions {
    /// When forwarding transactions on behalf of a client, this is the client's address
    /// specified for ddos protection.
    pub forwarded_client_addr: Option<SocketAddr>,
}

pub struct TransactionDriver<A: Clone> {
    authority_aggregator: ArcSwap<AuthorityAggregator<A>>,
    state: Mutex<State>,
    metrics: Arc<TransactionDriverMetrics>,
}

impl<A> TransactionDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        authority_aggregator: Arc<AuthorityAggregator<A>>,
        reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
        metrics: Arc<TransactionDriverMetrics>,
    ) -> Arc<Self> {
        let driver = Arc::new(Self {
            authority_aggregator: ArcSwap::new(authority_aggregator),
            state: Mutex::new(State::new()),
            metrics,
        });
        driver.enable_reconfig(reconfig_observer);
        driver
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    pub async fn submit_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
    ) -> Result<QuorumSubmitTransactionResponse, TransactionDriverError> {
        let tx_digest = request.transaction.digest();
        let is_single_writer_tx = !request.transaction.contains_shared_object();
        let timer = Instant::now();
        loop {
            match self.submit_transaction_once(&request, &options).await {
                Ok(resp) => {
                    let settlement_finality_latency = timer.elapsed().as_secs_f64();
                    self.metrics
                        .settlement_finality_latency
                        .with_label_values(&[if is_single_writer_tx {
                            TX_TYPE_SINGLE_WRITER_TX
                        } else {
                            TX_TYPE_SHARED_OBJ_TX
                        }])
                        .observe(settlement_finality_latency);
                    return Ok(resp);
                }
                Err(e) => {
                    tracing::warn!("Failed to submit transaction {tx_digest}: {}", e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    async fn submit_transaction_once(
        &self,
        request: &SubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumSubmitTransactionResponse, TransactionDriverError> {
        // Store the epoch number; we read it from the votes and use it later to create the certificate.
        let auth_agg = self.authority_aggregator.load();
        let committee = auth_agg.committee.clone();
        let epoch = committee.epoch();
        let transaction = request.transaction.clone();
        let raw_request = request
            .into_raw()
            .map_err(TransactionDriverError::SerializationError)?;

        // Step 1: Try to get a consensus position from a random validator
        let consensus_position = self
            .get_consensus_position(&auth_agg, &raw_request, options)
            .await?;

        debug!(
            "Transaction {} submitted to consensus at position: {consensus_position:?}",
            transaction.digest()
        );

        // Step 2: Wait for quorum of effects responses
        self.wait_for_quorum_effects(&auth_agg, &transaction, consensus_position, epoch, options)
            .await
    }

    async fn get_consensus_position(
        &self,
        auth_agg: &Guard<Arc<AuthorityAggregator<A>>>,
        raw_request: &RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<ConsensusPosition, TransactionDriverError> {
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();
        let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

        let consensus_position = timeout(
            Duration::from_secs(2),
            client.submit_transaction(raw_request.clone(), options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionDriverError::TimeoutGettingConsensusPosition)?
        .map_err(|e| {
            TransactionDriverError::RpcFailure(name.concise().to_string(), e.to_string())
        })?;

        Ok(consensus_position)
    }

    async fn wait_for_quorum_effects(
        &self,
        auth_agg: &Guard<Arc<AuthorityAggregator<A>>>,
        transaction: &Transaction,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumSubmitTransactionResponse, TransactionDriverError> {
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();
        let quorum_threshold = auth_agg.committee.quorum_threshold();

        // Create futures for all validators
        let mut futures = Vec::new();
        for (name, client) in clients {
            let client_clone = client.clone();
            let name_clone = name.clone();
            let transaction_digest = *transaction.digest();

            let future = async move {
                let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
                    epoch,
                    transaction_digest,
                    transaction_position: consensus_position,
                    include_details: true,
                })
                .map_err(|e| TransactionDriverError::SerializationError(e))?;

                match timeout(
                    Duration::from_secs(2),
                    client_clone.wait_for_effects(raw_request, options.forwarded_client_addr),
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

        // Wait for responses and collect them
        let mut responses = Vec::new();
        let mut total_response_weight = 0;
        let mut errors = Vec::new();

        // Wait for all futures to complete
        let results = futures::future::join_all(futures).await;

        // Process all results
        for result in results {
            match result {
                Ok((name, response)) => {
                    responses.push((name, response));
                    total_response_weight += auth_agg.committee.weight(&name);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        // Check if we have enough responses for quorum
        if total_response_weight < quorum_threshold {
            return Err(TransactionDriverError::InsufficientResponses {
                received: total_response_weight,
                required: quorum_threshold,
                errors: errors.into_iter().map(|e| e.to_string()).collect(),
            });
        }

        // Group responses by type and check for quorum
        let mut executed_responses = Vec::new();
        let mut executed_weight = 0;
        let mut rejected_responses = Vec::new();
        let mut rejected_weight = 0;
        let mut expired_responses = Vec::new();
        let mut expired_weight = 0;

        for (name, response) in responses {
            let weight = auth_agg.committee.weight(&name);
            match response {
                WaitForEffectsResponse::Executed {
                    effects_digest: _,
                    details,
                } => {
                    executed_responses.push((name, details));
                    executed_weight += weight;
                }
                WaitForEffectsResponse::Rejected { reason } => {
                    rejected_responses.push((name, reason));
                    rejected_weight += weight;
                }
                WaitForEffectsResponse::Expired(round) => {
                    expired_responses.push((name, round));
                    expired_weight += weight;
                }
            }
        }

        // Check for quorum of executed responses
        if executed_weight >= quorum_threshold {
            // All executed responses should have the same details, so we can use the first one
            let (_, details) = executed_responses.first().unwrap();
            if let Some(details) = details {
                return Ok(QuorumSubmitTransactionResponse {
                    effects: FinalizedEffects {
                        effects: details.effects.clone(),
                        finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
                    },
                    events: details.events.clone(),
                    input_objects: Some(details.input_objects.clone()),
                    output_objects: Some(details.output_objects.clone()),
                    auxiliary_data: None,
                });
            } else {
                return Err(TransactionDriverError::ExecutionDataNotFound(
                    transaction.digest().to_string(),
                ));
            }
        }

        // Check for quorum of rejected responses
        if rejected_weight >= quorum_threshold {
            // All rejected responses should have the same reason
            let (_, reason) = rejected_responses.first().unwrap();
            return Err(TransactionDriverError::TransactionRejected(
                reason.to_string(),
            ));
        }

        // Check for quorum of expired responses
        if expired_weight >= quorum_threshold {
            let (_, round) = expired_responses.first().unwrap();
            return Err(TransactionDriverError::TransactionExpired(
                round.to_string(),
            ));
        }

        // No quorum reached for any response type
        Err(TransactionDriverError::InsufficientResponses {
            received: executed_weight + rejected_weight + expired_weight,
            required: quorum_threshold,
            errors: errors.into_iter().map(|e| e.to_string()).collect(),
        })
    }

    fn enable_reconfig(
        self: &Arc<Self>,
        reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
    ) {
        let driver = self.clone();
        self.state.lock().tasks.spawn(monitored_future!(async move {
            let mut reconfig_observer = reconfig_observer.clone_boxed();
            reconfig_observer.run(driver).await;
        }));
    }
}

impl<A> AuthorityAggregatorUpdatable<A> for TransactionDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    fn epoch(&self) -> EpochId {
        self.authority_aggregator.load().committee.epoch
    }

    fn authority_aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.authority_aggregator.load_full()
    }

    fn update_authority_aggregator(&self, new_authorities: Arc<AuthorityAggregator<A>>) {
        tracing::info!(
            "Transaction Driver updating AuthorityAggregator with committee {}",
            new_authorities.committee
        );
        self.authority_aggregator.store(new_authorities);
    }
}

// Inner state of TransactionDriver.
struct State {
    tasks: JoinSet<()>,
}

impl State {
    fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
        }
    }
}

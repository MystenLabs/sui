// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod error;
mod message_types;
mod metrics;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::{ArcSwap, Guard};
pub use error::*;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
pub use message_types::*;
pub use metrics::*;
use mysten_metrics::{monitored_future, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::ConciseableName,
    committee::EpochId,
    digests::TransactionEffectsDigest,
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
    stake_aggregator::{InsertResult, StakeAggregator},
    wait_for_effects_request::{ExecutedData, WaitForEffectsRequest, WaitForEffectsResponse},
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

        // TODO: Add more sophisticated retry logic based on errors returned
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;

        loop {
            attempts += 1;
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
                    if attempts >= MAX_ATTEMPTS {
                        return Err(e);
                    }
                    tracing::warn!(
                        "Failed to submit transaction {tx_digest} (attempt {}/{}): {}",
                        attempts,
                        MAX_ATTEMPTS,
                        e
                    );
                    sleep(Duration::from_millis(500)).await;
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

        // First try to get a consensus position from a random validator
        let consensus_position = self
            .get_consensus_position(&auth_agg, &raw_request, options)
            .await?;

        debug!(
            "Transaction {} submitted to consensus at position: {consensus_position:?}",
            transaction.digest()
        );

        // Then wait for quorum of effects responses
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
        // Run full effects request and acknowledgments concurrently
        let (full_effects_result, acknowledgments_result) = tokio::join!(
            self.get_full_effects_with_retry(
                auth_agg,
                transaction,
                consensus_position,
                epoch,
                options
            ),
            self.wait_for_acknowledgments_with_retry(
                auth_agg,
                transaction,
                consensus_position,
                epoch,
                options
            )
        );

        match full_effects_result {
            Ok((effects_digest, executed_data)) => {
                let details = FinalizedEffects {
                    effects: executed_data.effects,
                    finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
                };

                match acknowledgments_result {
                    Ok(confirmed_digest) => {
                        if effects_digest == confirmed_digest {
                            self.metrics.executed_transactions.inc();

                            tracing::info!(
                                "Transaction {} executed with effects digest: {}",
                                transaction.digest(),
                                effects_digest
                            );

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
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn get_full_effects_with_retry(
        &self,
        auth_agg: &Guard<Arc<AuthorityAggregator<A>>>,
        transaction: &Transaction,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<(TransactionEffectsDigest, ExecutedData), TransactionDriverError> {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 3;
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();

        loop {
            attempts += 1;
            let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

            let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
                epoch,
                transaction_digest: *transaction.digest(),
                transaction_position: consensus_position,
                include_details: true,
            })
            .map_err(TransactionDriverError::SerializationError)?;

            match timeout(
                Duration::from_secs(2),
                client.wait_for_effects(raw_request, options.forwarded_client_addr),
            )
            .await
            {
                Ok(Ok(response)) => match response {
                    WaitForEffectsResponse::Executed {
                        effects_digest,
                        details,
                    } => {
                        if let Some(details) = details {
                            return Ok((effects_digest, *details));
                        } else {
                            return Err(TransactionDriverError::ExecutionDataNotFound(
                                transaction.digest().to_string(),
                            ));
                        }
                    }
                    WaitForEffectsResponse::Rejected { ref reason } => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionRejected(
                                reason.to_string(),
                            ));
                        }
                        tracing::warn!("Transaction rejected, retrying... Reason: {}", reason);
                    }
                    WaitForEffectsResponse::Expired(round) => {
                        if attempts >= MAX_ATTEMPTS {
                            return Err(TransactionDriverError::TransactionExpired(
                                round.to_string(),
                            ));
                        }
                        tracing::warn!("Transaction expired at round {}, retrying...", round);
                    }
                },
                Ok(Err(e)) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::RpcFailure(
                            name.concise().to_string(),
                            e.to_string(),
                        ));
                    }
                    tracing::warn!(
                        "Full effects request failed from {}: {}, retrying...",
                        name.concise(),
                        e
                    );
                }
                Err(_) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(TransactionDriverError::TimeoutWaitingForEffects);
                    }
                    tracing::warn!("Full effects request timed out, retrying...");
                }
            }
        }
    }

    async fn wait_for_acknowledgments_with_retry(
        &self,
        auth_agg: &Guard<Arc<AuthorityAggregator<A>>>,
        transaction: &Transaction,
        consensus_position: ConsensusPosition,
        epoch: EpochId,
        options: &SubmitTransactionOptions,
    ) -> Result<TransactionEffectsDigest, TransactionDriverError> {
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();
        let committee = auth_agg.committee.clone();

        // Create futures for all validators (digest-only requests)
        let mut futures = Vec::new();
        for (name, client) in clients {
            let client_clone = client.clone();
            let name_clone = *name;
            let transaction_digest = *transaction.digest();

            let future = async move {
                let raw_request = RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
                    epoch,
                    transaction_digest,
                    transaction_position: consensus_position,
                    include_details: false,
                })
                .map_err(TransactionDriverError::SerializationError)?;

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

        // Use StakeAggregator to track quorum for each effects digest
        let mut effects_digest_aggregators: HashMap<
            TransactionEffectsDigest,
            StakeAggregator<(), true>,
        > = HashMap::new();
        let mut rejected_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut expired_aggregator = StakeAggregator::<(), true>::new(committee.clone());
        let mut rejected_errors = Vec::new();
        let mut expired_errors = Vec::new();
        let mut errors = Vec::new();

        let mut futures = FuturesUnordered::from_iter(futures);

        while let Some(result) = futures.next().await {
            match result {
                Ok((name, response)) => {
                    match response {
                        WaitForEffectsResponse::Executed {
                            effects_digest,
                            details: _,
                        } => {
                            // Get or create aggregator for this digest
                            let aggregator = effects_digest_aggregators
                                .entry(effects_digest)
                                .or_insert_with(|| {
                                    StakeAggregator::<(), true>::new(committee.clone())
                                });

                            match aggregator.insert_generic(name, ()) {
                                InsertResult::QuorumReached(_) => {
                                    // Found quorum for this digest, check for inconsistencies
                                    let quorum_weight = aggregator.total_votes();
                                    let inconsistencies: Vec<_> = effects_digest_aggregators
                                        .iter()
                                        .filter_map(|(other_digest, other_aggregator)| {
                                            if other_digest != &effects_digest
                                                && other_aggregator.total_votes() > 0
                                            {
                                                Some((
                                                    *other_digest,
                                                    other_aggregator.total_votes(),
                                                ))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();

                                    for (other_digest, other_weight) in inconsistencies {
                                        tracing::warn!(
                                            "Effects digest inconsistency detected: quorum digest {:?} (weight {}), other digest {:?} (weight {})",
                                            effects_digest,
                                            quorum_weight,
                                            other_digest,
                                            other_weight
                                        );
                                        self.metrics.effects_digest_mismatches.inc();
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
                                        format!("Quorum rejected: {}", rejected_errors.join(", ")),
                                    ));
                                }
                                InsertResult::NotEnoughVotes { .. } => {}
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
                                        format!("Quorum expired: {}", expired_errors.join(", ")),
                                    ));
                                }
                                InsertResult::NotEnoughVotes { .. } => {}
                                InsertResult::Failed { error } => {
                                    tracing::warn!("Failed to insert expiration vote: {:?}", error);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
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

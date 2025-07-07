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

use arc_swap::ArcSwap;
pub use error::*;
pub use message_types::*;
pub use metrics::*;
use mysten_metrics::{monitored_future, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use rand::seq::SliceRandom as _;
use sui_types::{
    base_types::ConciseableName,
    committee::EpochId,
    messages_grpc::RawWaitForEffectsRequest,
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
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
        let is_single_writer_tx = !request.transaction.is_consensus_tx();
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

        // TODO(fastpath): Use validator performance metrics to choose who to submit with
        // Send the transaction to a random validator.
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();
        let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();

        let consensus_position = timeout(
            Duration::from_secs(2),
            client.submit_transaction(raw_request, options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionDriverError::TimeoutSubmittingTransaction)?
        .map_err(|e| {
            TransactionDriverError::RpcFailure(name.concise().to_string(), e.to_string())
        })?;

        debug!(
            "Transaction {} submitted to consensus at position: {consensus_position:?}",
            transaction.digest()
        );

        let response = match timeout(
            // TODO(fastpath): This will be removed when we change this to wait for effects from a quorum
            Duration::from_secs(20),
            client.wait_for_effects(
                RawWaitForEffectsRequest::try_from(WaitForEffectsRequest {
                    epoch,
                    transaction_digest: *transaction.digest(),
                    transaction_position: consensus_position,
                    include_details: true,
                })
                .map_err(TransactionDriverError::SerializationError)?,
                options.forwarded_client_addr,
            ),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => {
                return Err(TransactionDriverError::RpcFailure(
                    name.concise().to_string(),
                    e.to_string(),
                ));
            }
            Err(_) => {
                return Err(TransactionDriverError::TimeoutWaitingForEffects);
            }
        };

        match response {
            WaitForEffectsResponse::Executed {
                details,
                effects_digest: _,
            } => {
                if let Some(details) = details {
                    return Ok(QuorumSubmitTransactionResponse {
                        effects: FinalizedEffects {
                            effects: details.effects,
                            finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
                        },
                        events: details.events,
                        input_objects: Some(details.input_objects),
                        output_objects: Some(details.output_objects),
                        auxiliary_data: None,
                    });
                } else {
                    return Err(TransactionDriverError::ExecutionDataNotFound(
                        transaction.digest().to_string(),
                    ));
                }
            }
            WaitForEffectsResponse::Rejected { reason } => {
                return Err(TransactionDriverError::TransactionRejected(
                    reason.to_string(),
                ));
            }
            WaitForEffectsResponse::Expired(round) => {
                return Err(TransactionDriverError::TransactionExpired(
                    round.to_string(),
                ));
            }
        };
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

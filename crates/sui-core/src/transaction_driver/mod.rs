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
    quorum_driver_types::{EffectsFinalityInfo, FinalizedEffects},
};
use tokio::{
    task::JoinSet,
    time::{sleep, timeout},
};
use tracing::instrument;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    quorum_driver::{reconfig_observer::ReconfigObserver, AuthorityAggregatorUpdatable},
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
        let raw_request = request
            .into_raw()
            .map_err(TransactionDriverError::SerializationError)?;

        // TODO(fastpath): Use validator performance metrics to choose who to submit with
        // Send the transaction to a random validator.
        let clients = auth_agg.authority_clients.iter().collect::<Vec<_>>();
        let (name, client) = clients.choose(&mut rand::thread_rng()).unwrap();
        let response = timeout(
            Duration::from_secs(2),
            client.submit_transaction(raw_request, options.forwarded_client_addr),
        )
        .await
        .map_err(|_| TransactionDriverError::TimeoutBeforeFinality)?
        .map_err(|e| {
            TransactionDriverError::RpcFailure(name.concise().to_string(), e.to_string())
        })?;

        // TODO(fastpath): Aggregate quorum of responses before returning QuorumSubmitTransactionResponse
        Ok(QuorumSubmitTransactionResponse {
            effects: FinalizedEffects {
                effects: response.effects,
                // TODO(fastpath): return the epoch in response.
                finality_info: EffectsFinalityInfo::QuorumExecuted(epoch),
            },
            events: response.events,
            input_objects: response.input_objects,
            output_objects: response.output_objects,
            auxiliary_data: response.auxiliary_data,
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

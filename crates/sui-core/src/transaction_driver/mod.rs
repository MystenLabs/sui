// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod effects_certifier;
mod error;
mod message_types;
mod metrics;
mod transaction_submitter;

/// Exports
pub use message_types::*;
pub use metrics::*;

use std::{net::SocketAddr, sync::Arc, time::Instant};

use arc_swap::ArcSwap;
use effects_certifier::*;
use error::*;
use mysten_metrics::{monitored_future, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use sui_types::{
    committee::EpochId, digests::TransactionDigest, messages_grpc::RawSubmitTxRequest,
};
use tokio::task::JoinSet;
use tracing::instrument;
use transaction_submitter::*;

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
    submitter: TransactionSubmitter,
    certifier: EffectsCertifier,
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
            metrics: metrics.clone(),
            submitter: TransactionSubmitter::new(metrics.clone()),
            certifier: EffectsCertifier::new(metrics),
        });
        driver.enable_reconfig(reconfig_observer);
        driver
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    pub async fn drive_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let tx_digest = request.transaction.digest();
        let is_single_writer_tx = !request.transaction.is_consensus_tx();
        let raw_request = request
            .into_raw()
            .map_err(TransactionDriverError::SerializationError)?;
        let timer = Instant::now();

        let mut attempts = 0;
        loop {
            attempts += 1;
            // TODO(fastpath): Check local state before submitting transaction
            match self
                .drive_transaction_once(tx_digest, raw_request.clone(), &options)
                .await
            {
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
                    // TODO(fastpath): Break when the error is unretriable.
                    tracing::warn!(
                        "Failed to finalize transaction {tx_digest} (attempt {}): {}",
                        attempts,
                        e
                    );
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all, fields(tx_digest = ?tx_digest))]
    async fn drive_transaction_once(
        &self,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let auth_agg = self.authority_aggregator.load();

        // Get consensus position using TransactionSubmitter
        let submit_txn_resp = self
            .submitter
            .submit_transaction(&auth_agg, tx_digest, raw_request, options)
            .await?;

        // Wait for quorum effects using EffectsCertifier
        self.certifier
            .get_certified_finalized_effects(&auth_agg, tx_digest, submit_txn_resp, options)
            .await
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

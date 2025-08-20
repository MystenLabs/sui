// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod effects_certifier;
mod error;
mod message_types;
mod metrics;
mod request_retrier;
mod transaction_submitter;

/// Exports
pub use message_types::*;
pub use metrics::*;
use tokio_retry::strategy::{jitter, ExponentialBackoff};

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use effects_certifier::*;
use error::*;
use mysten_metrics::{monitored_future, TX_TYPE_SHARED_OBJ_TX, TX_TYPE_SINGLE_WRITER_TX};
use parking_lot::Mutex;
use sui_types::{
    committee::EpochId, digests::TransactionDigest, messages_grpc::RawSubmitTxRequest,
};
use tokio::{task::JoinSet, time::sleep};
use tracing::instrument;
use transaction_submitter::*;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    quorum_driver::{reconfig_observer::ReconfigObserver, AuthorityAggregatorUpdatable},
    validator_client_monitor::{ValidatorClientMetrics, ValidatorClientMonitor},
};
use sui_config::NodeConfig;

/// Options for submitting a transaction.
#[derive(Clone, Debug)]
pub struct SubmitTransactionOptions {
    /// When forwarding transactions on behalf of a client, this is the client's address
    /// specified for ddos protection.
    pub forwarded_client_addr: Option<SocketAddr>,

    /// Timeout for finalizing the transaction.
    pub timeout: Duration,
}



pub struct TransactionDriver<A: Clone> {
    authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    state: Mutex<State>,
    metrics: Arc<TransactionDriverMetrics>,
    submitter: TransactionSubmitter,
    certifier: EffectsCertifier,
    client_monitor: Arc<ValidatorClientMonitor<A>>,
}

impl<A> TransactionDriver<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        authority_aggregator: Arc<AuthorityAggregator<A>>,
        reconfig_observer: Arc<dyn ReconfigObserver<A> + Sync + Send>,
        metrics: Arc<TransactionDriverMetrics>,
        node_config: Option<&NodeConfig>,
        client_metrics: Arc<ValidatorClientMetrics>,
    ) -> Arc<Self> {
        let shared_swap = Arc::new(ArcSwap::new(authority_aggregator));

        // Extract validator client monitor config from NodeConfig or use default
        let monitor_config = node_config
            .and_then(|nc| nc.validator_client_monitor_config.clone())
            .unwrap_or_default();
        let client_monitor =
            ValidatorClientMonitor::new(monitor_config, client_metrics, shared_swap.clone());

        let driver = Arc::new(Self {
            authority_aggregator: shared_swap,
            state: Mutex::new(State::new()),
            metrics: metrics.clone(),
            submitter: TransactionSubmitter::new(metrics.clone()),
            certifier: EffectsCertifier::new(metrics),
            client_monitor,
        });

        driver.enable_reconfig(reconfig_observer);
        driver
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    pub async fn drive_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        // TODO: Make this configurable via NodeConfig or parameter
        const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(60);

        let tx_digest = request.transaction.digest();
        let is_single_writer_tx = !request.transaction.is_consensus_tx();
        let raw_request = request.into_raw().unwrap();
        let timer = Instant::now();

        self.metrics.total_transactions_submitted.inc();

        // Run the retry loop with timeout
        match tokio::time::timeout(
            TRANSACTION_TIMEOUT,
            self.drive_transaction_with_retry(
                *tx_digest,
                raw_request,
                options,
                is_single_writer_tx,
                timer,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_elapsed) => {
                // Timeout occurred - get the last error from state if available
                let state = self.state.lock();
                let last_error = state.last_transaction_error.get(tx_digest).cloned();
                let attempts = state
                    .last_attempt_count
                    .get(tx_digest)
                    .copied()
                    .unwrap_or(0);
                drop(state);

                self.metrics
                    .transaction_retries
                    .with_label_values(&["timeout"])
                    .observe(attempts as f64);

                Err(TransactionDriverError::Timeout {
                    elapsed: timer.elapsed(),
                    last_error: last_error.map(Box::new),
                    attempts,
                })
            }
        }
    }

    async fn drive_transaction_with_retry(
        &self,
        tx_digest: TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: SubmitTransactionOptions,
        is_single_writer_tx: bool,
        timer: Instant,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);
        // Exponential backoff with jitter to prevent thundering herd on retries
        let mut backoff = ExponentialBackoff::from_millis(100)
            .max_delay(MAX_RETRY_DELAY)
            .map(jitter);
        let mut attempts = 0;

        loop {
            // TODO(fastpath): Check local state before submitting transaction
            match self
                .drive_transaction_once(&tx_digest, raw_request.clone(), &options)
                .await
            {
                Ok(resp) => {
                    // Clear any stored error on success
                    {
                        let mut state = self.state.lock();
                        state.last_transaction_error.remove(&tx_digest);
                        state.last_attempt_count.remove(&tx_digest);
                    }

                    let settlement_finality_latency = timer.elapsed().as_secs_f64();
                    self.metrics
                        .settlement_finality_latency
                        .with_label_values(&[if is_single_writer_tx {
                            TX_TYPE_SINGLE_WRITER_TX
                        } else {
                            TX_TYPE_SHARED_OBJ_TX
                        }])
                        .observe(settlement_finality_latency);
                    // Record the number of retries for successful transaction
                    self.metrics
                        .transaction_retries
                        .with_label_values(&["success"])
                        .observe(attempts as f64);
                    return Ok(resp);
                }
                Err(e) => {
                    // Store the last error for timeout reporting
                    {
                        let mut state = self.state.lock();
                        state.last_transaction_error.insert(tx_digest, e.clone());
                        state.last_attempt_count.insert(tx_digest, attempts);
                    }

                    if !e.is_retriable() {
                        // Record the number of retries for failed transaction
                        self.metrics
                            .transaction_retries
                            .with_label_values(&["failure"])
                            .observe(attempts as f64);
                        return Err(e);
                    }
                    tracing::info!(
                        "Failed to finalize transaction (attempt {}): {}. Retrying ...",
                        attempts,
                        e
                    );
                }
            }

            sleep(backoff.next().unwrap_or(MAX_RETRY_DELAY)).await;
            attempts += 1;
        }
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?tx_digest))]
    async fn drive_transaction_once(
        &self,
        tx_digest: &TransactionDigest,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let auth_agg = self.authority_aggregator.load();

        let (name, submit_txn_resp) = self
            .submitter
            .submit_transaction(
                &auth_agg,
                &self.client_monitor,
                tx_digest,
                raw_request,
                options,
            )
            .await?;

        // Wait for quorum effects using EffectsCertifier
        self.certifier
            .get_certified_finalized_effects(
                &auth_agg,
                &self.client_monitor,
                tx_digest,
                name,
                submit_txn_resp,
                options,
            )
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

// Chooses the percentage of transactions to be driven by TransactionDriver.
pub fn choose_transaction_driver_percentage() -> u8 {
    // Currently, TD cannot work in mainnet.
    if let Ok(chain) =
        std::env::var(sui_types::digests::SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE_ENV_VAR_NAME)
    {
        if chain == "mainnet" {
            return 0;
        }
    }

    if let Ok(v) = std::env::var("TRANSACTION_DRIVER") {
        if let Ok(tx_driver_percentage) = v.parse::<u8>() {
            if tx_driver_percentage > 0 && tx_driver_percentage <= 100 {
                return tx_driver_percentage;
            }
        }
    }

    // Default to 50% in simtests.
    if cfg!(msim) {
        return 50;
    }

    0
}

// Inner state of TransactionDriver.
struct State {
    tasks: JoinSet<()>,
    last_transaction_error: HashMap<TransactionDigest, TransactionDriverError>,
    last_attempt_count: HashMap<TransactionDigest, u32>,
}

impl State {
    fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
            last_transaction_error: HashMap::new(),
            last_attempt_count: HashMap::new(),
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod effects_certifier;
mod error;
mod message_types;
mod metrics;
mod request_retrier;
mod transaction_submitter;

/// Exports
pub use error::TransactionDriverError;
pub use message_types::*;
pub use metrics::*;
use tokio_retry::strategy::{jitter, ExponentialBackoff};

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use effects_certifier::*;
use mysten_metrics::{monitored_future, TxType};
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
#[derive(Clone, Default, Debug)]
pub struct SubmitTransactionOptions {
    /// When forwarding transactions on behalf of a client, this is the client's address
    /// specified for ddos protection.
    pub forwarded_client_addr: Option<SocketAddr>,
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

    #[instrument(level = "error", skip_all, err(level = "info"), fields(tx_digest = ?request.transaction.digest()))]
    pub async fn drive_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        self.drive_transaction_with_timeout(request, options, None)
            .await
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    pub async fn drive_transaction_with_timeout(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
        timeout_duration: Option<Duration>,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let tx_digest = request.transaction.digest();
        let tx_type = if request.transaction.is_consensus_tx() {
            TxType::SharedObject
        } else {
            TxType::SingleWriter
        };
        let raw_request = request.into_raw().unwrap();
        let timer = Instant::now();

        self.metrics.total_transactions_submitted.inc();

        const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);
        // Exponential backoff with jitter to prevent thundering herd on retries
        let mut backoff = ExponentialBackoff::from_millis(100)
            .max_delay(MAX_RETRY_DELAY)
            .map(jitter);
        let mut attempts = 0;
        let mut latest_retriable_error = None;

        let retry_loop = async {
            loop {
                // TODO(fastpath): Check local state before submitting transaction
                match self
                    .drive_transaction_once(tx_digest, tx_type, raw_request.clone(), &options)
                    .await
                {
                    Ok(resp) => {
                        let settlement_finality_latency = timer.elapsed().as_secs_f64();
                        self.metrics
                            .settlement_finality_latency
                            .with_label_values(&[tx_type.as_str()])
                            .observe(settlement_finality_latency);
                        // Record the number of retries for successful transaction
                        self.metrics
                            .transaction_retries
                            .with_label_values(&["success"])
                            .observe(attempts as f64);
                        return Ok(resp);
                    }
                    Err(e) => {
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
                        // Buffer the latest retriable error to be returned in case of timeout
                        latest_retriable_error = Some(e);
                    }
                }

                sleep(backoff.next().unwrap_or(MAX_RETRY_DELAY)).await;
                attempts += 1;
            }
        };

        match timeout_duration {
            Some(duration) => {
                tokio::time::timeout(duration, retry_loop)
                    .await
                    .unwrap_or_else(|_| {
                        // Timeout occurred, return with latest retriable error if available
                        Err(TransactionDriverError::TimeOutWithLastRetriableError {
                            last_error: latest_retriable_error.map(Box::new),
                            attempts,
                            timeout: duration,
                        })
                    })
            }
            None => retry_loop.await,
        }
    }

    #[instrument(level = "error", skip_all, err)]
    async fn drive_transaction_once(
        &self,
        tx_digest: &TransactionDigest,
        tx_type: TxType,
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
                tx_type,
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
}

impl State {
    fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
        }
    }
}

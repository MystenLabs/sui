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
use mysten_metrics::{monitored_future, spawn_logged_monitored_task};
use parking_lot::Mutex;
use rand::Rng;
use sui_types::{
    committee::EpochId, digests::TransactionDigest, error::UserInputError,
    messages_grpc::RawSubmitTxRequest, transaction::TransactionDataAPI as _,
};
use tokio::{
    task::JoinSet,
    time::{interval, sleep},
};
use tracing::instrument;
use transaction_submitter::*;

use crate::{
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    quorum_driver::{reconfig_observer::ReconfigObserver, AuthorityAggregatorUpdatable},
    validator_client_monitor::{
        OperationFeedback, OperationType, TxType, ValidatorClientMetrics,
        ValidatorClientMonitorPool,
    },
};
use strum::IntoEnumIterator;
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
    client_monitor_pool: Arc<ValidatorClientMonitorPool<A>>,
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
        let client_monitor_pool =
            ValidatorClientMonitorPool::new(monitor_config, client_metrics, shared_swap.clone());

        let driver = Arc::new(Self {
            authority_aggregator: shared_swap,
            state: Mutex::new(State::new()),
            metrics: metrics.clone(),
            submitter: TransactionSubmitter::new(metrics.clone()),
            certifier: EffectsCertifier::new(metrics),
            client_monitor_pool,
        });

        let driver_clone = driver.clone();

        spawn_logged_monitored_task!(Self::run_latency_checks(driver_clone));

        driver.enable_reconfig(reconfig_observer);
        driver
    }

    // Runs a background task to send ping transactions to all validators to perform latency checks to test both the fast path and the consensus path.
    async fn run_latency_checks(self: Arc<Self>) {
        const MAX_DELAY_BETWEEN_REQUESTS_MS: u64 = 5_000;
        const INTERVAL_BETWEEN_RUNS_MS: u64 = 10_000;
        let mut interval = interval(Duration::from_millis(INTERVAL_BETWEEN_RUNS_MS));

        loop {
            interval.tick().await;

            // We are iterating over the single writer and shared object transaction types to test both the fast path and the consensus path.
            let mut tasks = JoinSet::new();
            for tx_type in TxType::iter() {
                // Send the latency requests to all validators.
                // TODO: do not send requests to all validators, but only to a subset of validators. For example order the validators by their
                // score ascending and then take a `K` of them.
                let self_clone = self.clone();
                let auth_agg = self_clone.authority_aggregator.load().clone();
                let clients: Vec<_> = auth_agg
                    .authority_clients
                    .iter()
                    .map(|(name, client)| (*name, client.clone()))
                    .collect();

                for (name, client) in clients {
                    let metrics_clone = self_clone.metrics.clone();
                    let client_monitor = self_clone.client_monitor_pool.get_monitor(tx_type);
                    let display_name = self_clone
                        .authority_aggregator
                        .load()
                        .get_display_name(&name);
                    let options = SubmitTransactionOptions::default();
                    let delay_ms = rand::thread_rng().gen_range(0..MAX_DELAY_BETWEEN_REQUESTS_MS);

                    // Clone values for each task
                    let self_task_clone = self_clone.clone();
                    let auth_agg_clone = auth_agg.clone();

                    tasks.spawn(async move {
                        // Add some random delay to the task to avoid all tasks running at the same time
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        let start_time = Instant::now();

                        // Submit the ping transaction to the validator.
                        let submit_txn_result = match self_task_clone
                            .submitter
                            .submit_transaction_once(
                                client.clone(),
                                &RawSubmitTxRequest {
                                    transactions: vec![],
                                    soft_bundle: false,
                                    ping: true,
                                },
                                &options,
                                &client_monitor,
                                name,
                                display_name.clone(),
                            )
                            .await
                        {
                            Ok(submit_txn_result) => submit_txn_result,
                            Err(e) => {
                                tracing::info!(
                                    "Failed to submit ping transaction to validator {}: {}",
                                    display_name.clone(),
                                    e
                                );
                                return Err(TransactionDriverError::Internal {
                                    error: e.to_string(),
                                });
                            }
                        };

                        // Wait for quorum effects using EffectsCertifier
                        let result = self_task_clone
                            .certifier
                            .get_certified_finalized_effects(
                                &auth_agg_clone,
                                &client_monitor,
                                &TransactionDigest::ZERO,
                                tx_type,
                                name,
                                submit_txn_result,
                                &options,
                                true,
                            )
                            .await?;

                        client_monitor.record_interaction_result(OperationFeedback {
                            authority_name: name,
                            display_name: auth_agg_clone.get_display_name(&name),
                            operation: OperationType::Finalization,
                            result: Ok(start_time.elapsed()),
                        });

                        metrics_clone
                            .ping_latency
                            .with_label_values(&[&display_name, tx_type.as_str()])
                            .observe(start_time.elapsed().as_secs_f64());

                        tracing::debug!(
                            "Ping transaction to validator {} completed end to end in {} seconds",
                            display_name,
                            start_time.elapsed().as_secs_f64()
                        );
                        Ok(result)
                    });
                }

                while let Some(result) = tasks.join_next().await {
                    if let Err(e) = result {
                        tracing::info!("Failed to drive ping transaction: {}", e);
                    }
                }
            }
        }
    }

    #[instrument(level = "error", skip_all, fields(tx_digest = ?request.transaction.digest()))]
    pub async fn drive_transaction(
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

        let gas_price = request.transaction.transaction_data().gas_price();
        let reference_gas_price = self.authority_aggregator.load().reference_gas_price;
        let amplification_factor = gas_price / reference_gas_price.max(1);
        if amplification_factor == 0 {
            return Err(TransactionDriverError::ValidationFailed {
                error: UserInputError::GasPriceUnderRGP {
                    gas_price,
                    reference_gas_price,
                }
                .to_string(),
            });
        }

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
                    .drive_transaction_once(
                        tx_digest,
                        tx_type,
                        amplification_factor,
                        raw_request.clone(),
                        &options,
                    )
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
                            tracing::info!("Failed to finalize transaction with non-retriable error after {} attempts: {}", attempts, e);
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
                        let e = TransactionDriverError::TimeoutWithLastRetriableError {
                            last_error: latest_retriable_error.map(Box::new),
                            attempts,
                            timeout: duration,
                        };
                        tracing::info!(
                            "Transaction timed out after {} attempts. Last error: {}",
                            attempts,
                            e
                        );
                        Err(e)
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
        amplification_factor: u64,
        raw_request: RawSubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let auth_agg = self.authority_aggregator.load();
        let amplification_factor =
            amplification_factor.min(auth_agg.committee.num_members() as u64);
        let start_time = Instant::now();
        let client_monitor = self.client_monitor_pool.get_monitor(tx_type);
        let ping = raw_request.ping;

        let (name, submit_txn_result) = self
            .submitter
            .submit_transaction(
                &auth_agg,
                &client_monitor,
                tx_digest,
                amplification_factor,
                raw_request,
                options,
            )
            .await?;

        // Wait for quorum effects using EffectsCertifier
        let result = self
            .certifier
            .get_certified_finalized_effects(
                &auth_agg,
                &client_monitor,
                tx_digest,
                tx_type,
                name,
                submit_txn_result,
                options,
                ping,
            )
            .await;

        client_monitor.record_interaction_result(OperationFeedback {
            authority_name: name,
            display_name: auth_agg.get_display_name(&name),
            operation: OperationType::Finalization,
            result: Ok(start_time.elapsed()),
        });
        result
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
pub fn choose_transaction_driver_percentage(
    chain_id: Option<sui_types::digests::ChainIdentifier>,
) -> u8 {
    // Currently, TD cannot work in mainnet.
    if let Some(chain_identifier) = chain_id {
        if chain_identifier.chain() == sui_protocol_config::Chain::Mainnet {
            return 0;
        }
    }

    // TODO(fastpath): Remove this once mfp hits mainnet
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

    // Default to 50% everywhere except mainnet
    50
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

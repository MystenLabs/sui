// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod effects_certifier;
mod error;
mod metrics;
mod request_retrier;
mod transaction_submitter;

/// Exports
pub use error::TransactionDriverError;
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
    base_types::AuthorityName,
    committee::EpochId,
    error::{ErrorCategory, UserInputError},
    messages_grpc::{PingType, SubmitTxRequest, SubmitTxResult, TxType},
    transaction::TransactionDataAPI as _,
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
        OperationFeedback, OperationType, ValidatorClientMetrics, ValidatorClientMonitor,
    },
};
use sui_config::NodeConfig;
/// Options for submitting a transaction.
#[derive(Clone, Default, Debug)]
pub struct SubmitTransactionOptions {
    /// When forwarding transactions on behalf of a client, this is the client's address
    /// specified for ddos protection.
    pub forwarded_client_addr: Option<SocketAddr>,

    /// When submitting a transaction, only the validators in the allowed validator list can be used to submit the transaction to.
    /// When the allowed validator list is empty, any validator can be used.
    pub allowed_validators: Vec<AuthorityName>,
}

#[derive(Clone, Debug)]
pub struct QuorumTransactionResponse {
    // TODO(fastpath): Stop using QD types
    pub effects: sui_types::quorum_driver_types::FinalizedEffects,

    pub events: Option<sui_types::effects::TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<sui_types::object::Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<sui_types::object::Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
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

        let driver_clone = driver.clone();

        spawn_logged_monitored_task!(Self::run_latency_checks(driver_clone));

        driver.enable_reconfig(reconfig_observer);
        driver
    }

    // Runs a background task to send ping transactions to all validators to perform latency checks to test both the fast path and the consensus path.
    async fn run_latency_checks(self: Arc<Self>) {
        const INTERVAL_BETWEEN_RUNS: Duration = Duration::from_millis(15_000);
        const MAX_DELAY_BETWEEN_REQUESTS_MS: u64 = 10_000;
        const PING_REQUEST_TIMEOUT: Duration = Duration::from_millis(5_000);

        let mut interval = interval(INTERVAL_BETWEEN_RUNS);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let mut tasks = JoinSet::new();

            for tx_type in [TxType::SingleWriter, TxType::SharedObject] {
                Self::execute_latency_for_tx_type(
                    self.clone(),
                    &mut tasks,
                    MAX_DELAY_BETWEEN_REQUESTS_MS,
                    PING_REQUEST_TIMEOUT,
                    tx_type,
                );
            }

            while let Some(result) = tasks.join_next().await {
                if let Err(e) = result {
                    tracing::info!("Error while driving ping transaction: {}", e);
                }
            }
        }
    }

    /// Executes a single round of latency checks for all validators and the provided transaction type.
    fn execute_latency_for_tx_type(
        self: Arc<Self>,
        tasks: &mut JoinSet<()>,
        max_delay_ms: u64,
        ping_timeout: Duration,
        tx_type: TxType,
    ) {
        // We are iterating over the single writer and shared object transaction types to test both the fast path and the consensus path.
        let auth_agg = self.authority_aggregator.load().clone();
        let validators = auth_agg.committee.names().cloned().collect::<Vec<_>>();

        self.metrics
            .latency_check_runs
            .with_label_values(&[tx_type.as_str()])
            .inc();

        for name in validators {
            let display_name = auth_agg.get_display_name(&name);
            let delay_ms = rand::thread_rng().gen_range(0..max_delay_ms);
            let self_clone = self.clone();

            let task = async move {
                // Add some random delay to the task to avoid all tasks running at the same time
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                let start_time = Instant::now();

                let ping = if tx_type == TxType::SingleWriter {
                    PingType::FastPath
                } else {
                    PingType::Consensus
                };

                // Now send a ping transaction to the chosen validator for the provided tx type
                match self_clone
                    .drive_transaction(
                        SubmitTxRequest::new_ping(ping),
                        SubmitTransactionOptions {
                            allowed_validators: vec![name],
                            ..Default::default()
                        },
                        Some(ping_timeout),
                    )
                    .await
                {
                    Ok(_) => {
                        tracing::debug!(
                            "Ping transaction to validator {} for tx type {} completed end to end in {} seconds",
                            display_name,
                            tx_type.as_str(),
                            start_time.elapsed().as_secs_f64()
                        );
                    }
                    Err(err) => {
                        tracing::info!("Failed to get certified finalized effects for tx type {}, for ping transaction to validator {}: {}", tx_type.as_str(), display_name, err);
                    }
                }
            };

            tasks.spawn(task);
        }
    }

    /// Drives transaction to submission and effects certification. If ping is provided, then the requested will be treated as a ping transaction.
    #[instrument(level = "error", skip_all, fields(tx_digest = ?request.transaction.as_ref().map(|t| t.digest()), ping = ?request.ping))]
    pub async fn drive_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
        timeout_duration: Option<Duration>,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        // For ping requests, the amplification factor is always 1.
        let amplification_factor = if request.ping.is_some() {
            1
        } else {
            let gas_price = request
                .transaction
                .as_ref()
                .unwrap()
                .transaction_data()
                .gas_price();
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
            amplification_factor
        };

        let tx_type = request.tx_type();
        let ping_label = if request.ping.is_some() {
            "true"
        } else {
            "false"
        };
        let timer = Instant::now();

        self.metrics
            .total_transactions_submitted
            .with_label_values(&[tx_type.as_str(), ping_label])
            .inc();

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
                    .drive_transaction_once(amplification_factor, request.clone(), &options)
                    .await
                {
                    Ok(resp) => {
                        let settlement_finality_latency = timer.elapsed().as_secs_f64();
                        self.metrics
                            .settlement_finality_latency
                            .with_label_values(&[tx_type.as_str(), ping_label])
                            .observe(settlement_finality_latency);
                        // Record the number of retries for successful transaction
                        self.metrics
                            .transaction_retries
                            .with_label_values(&["success", tx_type.as_str(), ping_label])
                            .observe(attempts as f64);
                        return Ok(resp);
                    }
                    Err(e) => {
                        if !e.is_submission_retriable() {
                            // Record the number of retries for failed transaction
                            self.metrics
                                .transaction_retries
                                .with_label_values(&["failure", tx_type.as_str(), ping_label])
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

                let overload = if let Some(e) = &latest_retriable_error {
                    e.categorize() == ErrorCategory::ValidatorOverloaded
                } else {
                    false
                };
                let delay = if overload {
                    // Increase delay during overload.
                    backoff.next().unwrap_or(MAX_RETRY_DELAY) + MAX_RETRY_DELAY
                } else {
                    backoff.next().unwrap_or(MAX_RETRY_DELAY)
                };
                sleep(delay).await;

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
        amplification_factor: u64,
        request: SubmitTxRequest,
        options: &SubmitTransactionOptions,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        let auth_agg = self.authority_aggregator.load();
        let amplification_factor =
            amplification_factor.min(auth_agg.committee.num_members() as u64);
        let start_time = Instant::now();
        let ping = request.ping;
        let tx_type = request.tx_type();
        let tx_digest = request.tx_digest();

        let (name, submit_txn_result) = self
            .submitter
            .submit_transaction(
                &auth_agg,
                &self.client_monitor,
                tx_type,
                amplification_factor,
                request,
                options,
            )
            .await?;
        if let SubmitTxResult::Rejected { error } = &submit_txn_result {
            return Err(TransactionDriverError::ClientInternal {
                error: format!("SubmitTxResult::Rejected should have been returned as an error in submit_transaction(): {}", error),
            });
        }

        // Wait for quorum effects using EffectsCertifier
        let result = self
            .certifier
            .get_certified_finalized_effects(
                &auth_agg,
                &self.client_monitor,
                tx_digest,
                tx_type,
                name,
                submit_txn_result,
                options,
                ping,
            )
            .await;

        if result.is_ok() {
            self.client_monitor
                .record_interaction_result(OperationFeedback {
                    authority_name: name,
                    display_name: auth_agg.get_display_name(&name),
                    operation: if tx_type == TxType::SingleWriter {
                        OperationType::FastPath
                    } else {
                        OperationType::Consensus
                    },
                    result: Ok(start_time.elapsed()),
                });
        }
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

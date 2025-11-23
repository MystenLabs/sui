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
use mysten_common::backoff::ExponentialBackoff;

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
    quorum_driver::{AuthorityAggregatorUpdatable, reconfig_observer::ReconfigObserver},
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
    pub allowed_validators: Vec<String>,

    /// When submitting a transaction, the validators in the blocked validator list cannot be used to submit the transaction to.
    /// When the blocked validator list is empty, no restrictions are applied.
    pub blocked_validators: Vec<String>,
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
    // TODO: accept a TransactionDriverConfig to set default allowed & blocked validators.
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

    /// Returns the authority aggregator wrapper which upgrades on epoch changes.
    pub fn authority_aggregator(&self) -> &Arc<ArcSwap<AuthorityAggregator<A>>> {
        &self.authority_aggregator
    }

    /// Drives transaction to finalization.
    ///
    /// Internally, retries the attempt to finalize a transaction until:
    /// - The transaction is finalized.
    /// - The transaction observes a non-retriable error.
    /// - Timeout is reached.
    #[instrument(level = "error", skip_all, fields(tx_digest = ?request.transaction.as_ref().map(|t| t.digest()), ping = %request.ping_type.is_some()))]
    pub async fn drive_transaction(
        &self,
        request: SubmitTxRequest,
        options: SubmitTransactionOptions,
        timeout_duration: Option<Duration>,
    ) -> Result<QuorumTransactionResponse, TransactionDriverError> {
        const MAX_DRIVE_TRANSACTION_RETRY_DELAY: Duration = Duration::from_secs(10);

        // For ping requests, the amplification factor is always 1.
        let amplification_factor = if request.ping_type.is_some() {
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
        let ping_label = if request.ping_type.is_some() {
            "true"
        } else {
            "false"
        };
        let timer = Instant::now();

        self.metrics
            .total_transactions_submitted
            .with_label_values(&[tx_type.as_str(), ping_label])
            .inc();

        let mut backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            MAX_DRIVE_TRANSACTION_RETRY_DELAY,
        );
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
                        self.metrics
                            .drive_transaction_errors
                            .with_label_values(&[
                                e.categorize().into(),
                                tx_type.as_str(),
                                ping_label,
                            ])
                            .inc();
                        if !e.is_submission_retriable() {
                            // Record the number of retries for failed transaction
                            self.metrics
                                .transaction_retries
                                .with_label_values(&["failure", tx_type.as_str(), ping_label])
                                .observe(attempts as f64);
                            if request.transaction.is_some() {
                                tracing::info!(
                                    "User transaction failed to finalize (attempt {}), with non-retriable error: {}",
                                    attempts,
                                    e
                                );
                            }
                            return Err(e);
                        }
                        if request.transaction.is_some() {
                            tracing::info!(
                                "User transaction failed to finalize (attempt {}): {}. Retrying ...",
                                attempts,
                                e
                            );
                        }
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
                    const OVERLOAD_ADDITIONAL_DELAY: Duration = Duration::from_secs(10);
                    backoff.next().unwrap() + OVERLOAD_ADDITIONAL_DELAY
                } else {
                    backoff.next().unwrap()
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
                        if request.transaction.is_some() {
                            tracing::info!(
                                "User transaction timed out after {} attempts. Last error: {}",
                                attempts,
                                e
                            );
                        }
                        Err(e)
                    })
            }
            None => retry_loop.await,
        }
    }

    #[instrument(level = "error", skip_all, err(level = "debug"))]
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
        let tx_type = request.tx_type();
        let tx_digest = request.tx_digest();
        let ping_type = request.ping_type;

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
                error: format!(
                    "SubmitTxResult::Rejected should have been returned as an error in submit_transaction(): {}",
                    error
                ),
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
                    ping_type,
                    result: Ok(start_time.elapsed()),
                });
        }
        result
    }

    // Runs a background task to send ping transactions to all validators to perform latency checks to test both the fast path and the consensus path.
    async fn run_latency_checks(self: Arc<Self>) {
        const INTERVAL_BETWEEN_RUNS: Duration = Duration::from_secs(15);
        const MAX_JITTER: Duration = Duration::from_secs(10);
        const PING_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

        let mut interval = interval(INTERVAL_BETWEEN_RUNS);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let mut tasks = JoinSet::new();

            for tx_type in [TxType::SingleWriter, TxType::SharedObject] {
                Self::ping_for_tx_type(
                    self.clone(),
                    &mut tasks,
                    tx_type,
                    MAX_JITTER,
                    PING_REQUEST_TIMEOUT,
                );
            }

            while let Some(result) = tasks.join_next().await {
                if let Err(e) = result {
                    tracing::debug!("Error while driving ping transaction: {}", e);
                }
            }
        }
    }

    /// Pings all validators for e2e latency with the provided transaction type.
    fn ping_for_tx_type(
        self: Arc<Self>,
        tasks: &mut JoinSet<()>,
        tx_type: TxType,
        max_jitter: Duration,
        ping_timeout: Duration,
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
            let delay_ms = rand::thread_rng().gen_range(0..max_jitter.as_millis()) as u64;
            let self_clone = self.clone();

            let task = async move {
                // Add some random delay to the task to avoid all tasks running at the same time
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                let start_time = Instant::now();

                let ping_type = if tx_type == TxType::SingleWriter {
                    PingType::FastPath
                } else {
                    PingType::Consensus
                };

                // Now send a ping transaction to the chosen validator for the provided tx type
                match self_clone
                    .drive_transaction(
                        SubmitTxRequest::new_ping(ping_type),
                        SubmitTransactionOptions {
                            allowed_validators: vec![display_name.clone()],
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
                        tracing::debug!(
                            "Failed to get certified finalized effects for tx type {}, for ping transaction to validator {}: {}",
                            tx_type.as_str(),
                            display_name,
                            err
                        );
                    }
                }
            };

            tasks.spawn(task);
        }
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
    if let Ok(v) = std::env::var("TRANSACTION_DRIVER")
        && let Ok(tx_driver_percentage) = v.parse::<u8>()
        && (0..=100).contains(&tx_driver_percentage)
    {
        return tx_driver_percentage;
    }

    if let Some(chain_identifier) = chain_id
        && chain_identifier.chain() == sui_protocol_config::Chain::Unknown
    {
        // Kep test coverage for QD.
        return 50;
    }

    // Default to 100% everywhere
    100
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::validator_client_monitor::stats::ClientObservedStats;
use crate::validator_client_monitor::{
    OperationFeedback, OperationType, metrics::ValidatorClientMetrics,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::committee::Committee;
use sui_types::messages_grpc::TxType;
use sui_types::{base_types::AuthorityName, messages_grpc::ValidatorHealthRequest};
use tokio::{
    task::JoinSet,
    time::{interval, timeout},
};
use tracing::{debug, info, warn};

/// Monitors validator interactions from the client's perspective.
///
/// This component:
/// - Collects client-side metrics from TransactionDriver operations
/// - Runs periodic health checks on all validators from the client
/// - Maintains client-observed statistics for reliability and latency
/// - Provides intelligent validator selection based on client-observed performance
/// - Handles epoch changes by cleaning up stale validator data
///
/// The monitor runs a background task for health checks and uses
/// moving averages to smooth client-side measurements.
pub struct ValidatorClientMonitor<A: Clone> {
    config: ValidatorClientMonitorConfig,
    metrics: Arc<ValidatorClientMetrics>,
    client_stats: RwLock<ClientObservedStats>,
    authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    cached_latencies: RwLock<HashMap<TxType, HashMap<AuthorityName, Duration>>>,
}

impl<A> ValidatorClientMonitor<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        config: ValidatorClientMonitorConfig,
        metrics: Arc<ValidatorClientMetrics>,
        authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    ) -> Arc<Self> {
        info!(
            "Validator client monitor starting with config: {:?}",
            config
        );

        let monitor = Arc::new(Self {
            config: config.clone(),
            metrics,
            client_stats: RwLock::new(ClientObservedStats::new(config)),
            authority_aggregator,
            cached_latencies: RwLock::new(HashMap::new()),
        });

        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            monitor_clone.run_health_checks().await;
        });

        monitor
    }

    #[cfg(test)]
    pub fn new_for_test(authority_aggregator: Arc<AuthorityAggregator<A>>) -> Arc<Self> {
        use prometheus::Registry;

        Self::new(
            ValidatorClientMonitorConfig::default(),
            Arc::new(ValidatorClientMetrics::new(&Registry::default())),
            Arc::new(ArcSwap::new(authority_aggregator)),
        )
    }

    /// Background task that runs periodic health checks on all validators.
    ///
    /// Sends health check requests to all validators in parallel and records
    /// the results (success/failure and latency). Timeouts are treated as
    /// failures without recording latency to avoid polluting latency statistics.
    async fn run_health_checks(self: Arc<Self>) {
        let mut interval = interval(self.config.health_check_interval);

        loop {
            interval.tick().await;

            let authority_agg = self.authority_aggregator.load();

            let current_validators: Vec<_> = authority_agg.committee.names().cloned().collect();
            self.client_stats
                .write()
                .retain_validators(&current_validators);

            let mut tasks = JoinSet::new();

            for (name, safe_client) in authority_agg.authority_clients.iter() {
                let name = *name;
                let display_name = authority_agg.get_display_name(&name);
                let client = safe_client.clone();
                let timeout_duration = self.config.health_check_timeout;
                let monitor = self.clone();

                tasks.spawn(async move {
                    let start = Instant::now();
                    match timeout(
                        timeout_duration,
                        client.validator_health(ValidatorHealthRequest {}),
                    )
                    .await
                    {
                        // TODO: Actually use the response details.
                        Ok(Ok(_response)) => {
                            let latency = start.elapsed();
                            monitor.record_interaction_result(OperationFeedback {
                                authority_name: name,
                                display_name: display_name.clone(),
                                operation: OperationType::HealthCheck,
                                ping_type: None,
                                result: Ok(latency),
                            });
                        }
                        Ok(Err(_)) => {
                            let _latency = start.elapsed();
                            monitor.record_interaction_result(OperationFeedback {
                                authority_name: name,
                                display_name: display_name.clone(),
                                operation: OperationType::HealthCheck,
                                ping_type: None,
                                result: Err(()),
                            });
                        }
                        Err(_) => {
                            monitor.record_interaction_result(OperationFeedback {
                                authority_name: name,
                                display_name,
                                operation: OperationType::HealthCheck,
                                ping_type: None,
                                result: Err(()),
                            });
                        }
                    }
                });
            }

            while let Some(result) = tasks.join_next().await {
                if let Err(e) = result {
                    warn!("Health check task failed: {}", e);
                }
            }

            self.update_cached_latencies(&authority_agg);
        }
    }
}

impl<A: Clone> ValidatorClientMonitor<A> {
    /// Calculate and cache latencies for all validators.
    ///
    /// This method is called periodically after health checks complete to update
    /// the cached validator latencies. Those are the end to end latencies as calculated for each validator
    /// taking into account the reliability of the validator.
    fn update_cached_latencies(&self, authority_agg: &AuthorityAggregator<A>) {
        let committee = &authority_agg.committee;
        let mut cached_latencies = self.cached_latencies.write();

        for tx_type in TxType::iter() {
            let latencies_map = self
                .client_stats
                .read()
                .get_all_validator_stats(committee, tx_type);

            for (validator, latency) in latencies_map.iter() {
                debug!(
                    "Validator {}, tx type {}: latency {}",
                    validator,
                    tx_type.as_str(),
                    latency.as_secs_f64()
                );
                let display_name = authority_agg.get_display_name(validator);
                self.metrics
                    .performance
                    .with_label_values(&[&display_name, tx_type.as_str()])
                    .set(latency.as_secs_f64());
            }

            cached_latencies.insert(tx_type, latencies_map);
        }
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Records operation results including success/failure status and latency
    /// from the client's perspective. Updates both Prometheus metrics and
    /// internal client statistics. This is the primary interface for the
    /// TransactionDriver to report client-observed validator interactions.
    /// TODO: Consider adding a byzantine flag to the feedback.
    pub fn record_interaction_result(&self, feedback: OperationFeedback) {
        let operation_str = match feedback.operation {
            OperationType::Submit => "submit",
            OperationType::Effects => "effects",
            OperationType::HealthCheck => "health_check",
            OperationType::FastPath => "fast_path",
            OperationType::Consensus => "consensus",
        };
        let ping_label = if feedback.ping_type.is_some() {
            "true"
        } else {
            "false"
        };

        match feedback.result {
            Ok(latency) => {
                self.metrics
                    .observed_latency
                    .with_label_values(&[&feedback.display_name, operation_str, ping_label])
                    .observe(latency.as_secs_f64());
                self.metrics
                    .operation_success
                    .with_label_values(&[&feedback.display_name, operation_str, ping_label])
                    .inc();
            }
            Err(()) => {
                self.metrics
                    .operation_failure
                    .with_label_values(&[&feedback.display_name, operation_str, ping_label])
                    .inc();
            }
        }

        let mut client_stats = self.client_stats.write();
        client_stats.record_interaction_result(feedback);
    }

    /// Select validators based on client-observed performance for the given transaction type.
    ///
    /// The current committee is passed in to ensure this function has the latest committee information.
    ///
    /// Also the tx type is passed in so that we can select validators based on their respective latencies
    /// for the transaction type.
    ///
    /// Validators with latencies within `delta` of the lowest latency in the given transaction type
    /// are shuffled, to balance the load among the fastest validators.
    ///
    /// Returns a vector containing all validators, where
    /// 1. Fast validators within `delta` of the lowest latency are shuffled.
    /// 2. Remaining slow validators are sorted by latency in ascending order.
    pub fn select_shuffled_preferred_validators(
        &self,
        committee: &Committee,
        tx_type: TxType,
        delta: f64,
    ) -> Vec<AuthorityName> {
        let mut rng = rand::thread_rng();

        let cached_latencies = self.cached_latencies.read();
        let Some(cached_latencies) = cached_latencies.get(&tx_type) else {
            let mut validators: Vec<_> = committee.names().cloned().collect();
            validators.shuffle(&mut rng);
            return validators;
        };

        // Since the cached latencies are updated periodically, it is possible that it was ran on
        // an out-of-date committee.
        let mut validator_with_latencies: Vec<_> = committee
            .names()
            .map(|v| {
                (
                    *v,
                    cached_latencies.get(v).cloned().unwrap_or(Duration::ZERO),
                )
            })
            .collect();
        if validator_with_latencies.is_empty() {
            return vec![];
        }
        // Shuffle the validators to balance the load among validators with the same latency.
        validator_with_latencies.shuffle(&mut rng);
        // Sort by latency in ascending order. We want to select the validators with the lowest latencies.
        validator_with_latencies.sort_by_key(|(_, latency)| *latency);

        // Shuffle the validators within delta of the lowest latency, for load balancing.
        let lowest_latency = validator_with_latencies[0].1;
        let threshold = lowest_latency.mul_f64(1.0 + delta);
        let k = validator_with_latencies
            .iter()
            .enumerate()
            .find(|(_, (_, latency))| *latency > threshold)
            .map(|(i, _)| i)
            .unwrap_or(validator_with_latencies.len());
        validator_with_latencies[..k].shuffle(&mut rng);
        self.metrics
            .shuffled_validators
            .with_label_values(&[tx_type.as_str()])
            .observe(k as f64);

        validator_with_latencies
            .into_iter()
            .map(|(v, _)| v)
            .collect()
    }

    #[cfg(test)]
    pub fn force_update_cached_latencies(&self, authority_agg: &AuthorityAggregator<A>) {
        self.update_cached_latencies(authority_agg);
    }

    #[cfg(test)]
    pub fn get_client_stats_len(&self) -> usize {
        self.client_stats.read().validator_stats.len()
    }

    #[cfg(test)]
    pub fn has_validator_stats(&self, validator: &AuthorityName) -> bool {
        self.client_stats
            .read()
            .validator_stats
            .contains_key(validator)
    }
}

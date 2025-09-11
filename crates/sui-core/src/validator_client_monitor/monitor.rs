// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::validator_client_monitor::stats::ClientObservedStats;
use crate::validator_client_monitor::TxType;
use crate::validator_client_monitor::{
    metrics::ValidatorClientMetrics, OperationFeedback, OperationType,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::{sync::Arc, time::Instant};
use strum::IntoEnumIterator;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::committee::Committee;
use sui_types::{base_types::AuthorityName, messages_grpc::ValidatorHealthRequest};
use tokio::{
    task::JoinSet,
    time::{interval, timeout},
};
use tracing::{debug, info, warn};

/// Pool of monitors for different transaction types. It allows us to both manage multiple monitors per specific
/// transaction type and also run health check operations updating all the monitors.
pub struct ValidatorClientMonitorPool<A: Clone> {
    config: ValidatorClientMonitorConfig,
    authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    monitors: HashMap<TxType, Arc<ValidatorClientMonitor>>,
}

impl<A> ValidatorClientMonitorPool<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        config: ValidatorClientMonitorConfig,
        metrics: Arc<ValidatorClientMetrics>,
        authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    ) -> Arc<Self> {
        let mut monitors = HashMap::new();
        for tx_type in TxType::iter() {
            monitors.insert(
                tx_type,
                ValidatorClientMonitor::new(config.clone(), metrics.clone(), tx_type),
            );
        }
        let pool = Arc::new(Self {
            config,
            authority_aggregator,
            monitors,
        });

        let pool_clone = pool.clone();
        tokio::spawn(async move {
            pool_clone.run_health_checks().await;
        });

        pool
    }

    // Gets a monitor for a specific transaction type.
    pub fn get_monitor(&self, tx_type: TxType) -> Arc<ValidatorClientMonitor> {
        self.monitors
            .get(&tx_type)
            .unwrap_or_else(|| panic!("Monitor for transaction type {:?} not found", tx_type))
            .clone()
    }

    // Records the interaction result to all monitors. This is mostly used to update records during health checks which don't necessary refer to a specific monitor.
    fn record_interaction_result_all_monitors(&self, feedback: OperationFeedback) {
        for monitor in self.monitors.values() {
            monitor.record_interaction_result(feedback.clone());
        }
    }

    fn update_cached_scores_all_monitors(&self) {
        let authority_agg = self.authority_aggregator.load();
        for monitor in self.monitors.values() {
            monitor.update_cached_scores(&authority_agg);
        }
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
            for monitor in self.monitors.values() {
                monitor.retain_validators(&current_validators);
            }

            let mut tasks = JoinSet::new();

            for (name, safe_client) in authority_agg.authority_clients.iter() {
                let name = *name;
                let display_name = authority_agg.get_display_name(&name);
                let client = safe_client.clone();
                let timeout_duration = self.config.health_check_timeout;
                let pool = self.clone();

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
                            pool.record_interaction_result_all_monitors(OperationFeedback {
                                authority_name: name,
                                display_name: display_name.clone(),
                                operation: OperationType::HealthCheck,
                                result: Ok(latency),
                            });
                        }
                        Ok(Err(_)) => {
                            let _latency = start.elapsed();
                            pool.record_interaction_result_all_monitors(OperationFeedback {
                                authority_name: name,
                                display_name: display_name.clone(),
                                operation: OperationType::HealthCheck,
                                result: Err(()),
                            });
                        }
                        Err(_) => {
                            pool.record_interaction_result_all_monitors(OperationFeedback {
                                authority_name: name,
                                display_name,
                                operation: OperationType::HealthCheck,
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

            self.update_cached_scores_all_monitors();
        }
    }
}

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
/// exponential moving averages to smooth client-side measurements.
pub struct ValidatorClientMonitor {
    metrics: Arc<ValidatorClientMetrics>,
    client_stats: RwLock<ClientObservedStats>,
    cached_scores: RwLock<Option<HashMap<AuthorityName, f64>>>,
    tx_type: TxType,
}

impl ValidatorClientMonitor {
    pub fn new(
        config: ValidatorClientMonitorConfig,
        metrics: Arc<ValidatorClientMetrics>,
        tx_type: TxType,
    ) -> Arc<Self> {
        info!("Validator client monitor starting for type: {:?}", tx_type);

        Arc::new(Self {
            metrics,
            client_stats: RwLock::new(ClientObservedStats::new(config)),
            cached_scores: RwLock::new(None),
            tx_type,
        })
    }

    #[cfg(test)]
    pub fn new_for_test(tx_type: TxType) -> Arc<Self> {
        use prometheus::Registry;

        Self::new(
            ValidatorClientMonitorConfig::default(),
            Arc::new(ValidatorClientMetrics::new(&Registry::default())),
            tx_type,
        )
    }
}

impl ValidatorClientMonitor {
    /// Calculate and cache scores for all validators.
    ///
    /// This method is called periodically after health checks complete to update
    /// the cached validator scores.
    fn update_cached_scores(&self, authority_agg: &AuthorityAggregator<impl Clone>) {
        let committee = &authority_agg.committee;

        let score_map = self.client_stats.read().get_all_validator_stats(committee);

        for (validator, score) in score_map.iter() {
            debug!("Validator {}: score {}", validator, score);
            let display_name = authority_agg.get_display_name(validator);
            self.metrics
                .performance_score
                .with_label_values(&[&display_name, self.tx_type.as_str()])
                .set(*score);
        }

        *self.cached_scores.write() = Some(score_map);
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Records operation results including success/failure status and latency
    /// from the client's perspective. Updates both Prometheus metrics and
    /// internal client statistics. This is the primary interface for the
    /// TransactionDriver to report client-observed validator interactions.
    /// TODO: Consider adding a byzantine flag to the feedback.
    pub fn record_interaction_result(&self, feedback: OperationFeedback) {
        let operation_str = feedback.operation.as_str();
        let tx_type_str = self.tx_type.as_str();

        match feedback.result {
            Ok(latency) => {
                self.metrics
                    .observed_latency
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type_str])
                    .observe(latency.as_secs_f64());
                self.metrics
                    .operation_success
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type_str])
                    .inc();
            }
            Err(()) => {
                self.metrics
                    .operation_failure
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type_str])
                    .inc();
            }
        }

        let mut client_stats = self.client_stats.write();
        client_stats.record_interaction_result(feedback, &self.metrics, self.tx_type);
    }

    /// Select validators based on client-observed performance with shuffled top k.
    ///
    /// We need to pass the current committee here because it is possible
    /// that the fullnode is in the middle of a committee change when this
    /// is called, and we need to maintain an invariant that the selected
    /// validators are always in the committee passed in.
    ///
    /// We shuffle the top k validators to avoid the same validator being selected
    /// too many times in a row and getting overloaded.
    ///
    /// Returns a vector containing:
    /// 1. The top `k` validators by score (shuffled)
    /// 2. The remaining validators ordered by score (not shuffled)
    pub fn select_shuffled_preferred_validators(
        &self,
        committee: &Committee,
        k: usize,
        // TODO: Pass in the operation type so that we can select validators based on the operation type.
    ) -> Vec<AuthorityName> {
        let mut rng = rand::thread_rng();

        let Some(cached_scores) = self.cached_scores.read().clone() else {
            let mut validators: Vec<_> = committee.names().cloned().collect();
            validators.shuffle(&mut rng);
            return validators;
        };

        // Since the cached scores are updated periodically, it is possible that it was ran on
        // an out-of-date committee.
        let mut validator_with_scores: Vec<_> = committee
            .names()
            .map(|v| (*v, cached_scores.get(v).copied().unwrap_or(0.0)))
            .collect();
        validator_with_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let k = k.min(validator_with_scores.len());
        validator_with_scores[..k].shuffle(&mut rng);

        validator_with_scores.into_iter().map(|(v, _)| v).collect()
    }

    pub fn retain_validators(&self, validators: &[AuthorityName]) {
        self.client_stats.write().retain_validators(validators);
    }

    #[cfg(test)]
    pub fn force_update_cached_scores(&self, authority_agg: &AuthorityAggregator<impl Clone>) {
        self.update_cached_scores(authority_agg);
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

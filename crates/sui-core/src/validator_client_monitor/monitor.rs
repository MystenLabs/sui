// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::validator_client_monitor::stats::ClientObservedStats;
use crate::validator_client_monitor::{
    metrics::ValidatorClientMetrics, OperationFeedback, OperationType,
};
use arc_swap::ArcSwap;
use mysten_metrics::TxType;
use parking_lot::RwLock;
use rand::{seq::SliceRandom, Rng};
use std::collections::HashMap;
use std::{sync::Arc, time::Instant};
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::committee::Committee;
use sui_types::messages_consensus::ConsensusPosition;
use sui_types::{base_types::AuthorityName, messages_grpc::ValidatorLatencyRequest};
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
/// exponential moving averages to smooth client-side measurements.
pub struct ValidatorClientMonitor<A: Clone> {
    config: ValidatorClientMonitorConfig,
    metrics: Arc<ValidatorClientMetrics>,
    client_stats: RwLock<ClientObservedStats>,
    authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
    cached_scores: RwLock<Option<HashMap<AuthorityName, f64>>>,
    tx_type: TxType,
}

impl<A> ValidatorClientMonitor<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        config: ValidatorClientMonitorConfig,
        metrics: Arc<ValidatorClientMetrics>,
        authority_aggregator: Arc<ArcSwap<AuthorityAggregator<A>>>,
        tx_type: TxType,
    ) -> Arc<Self> {
        info!(
            "Validator client monitor starting with config: {:?}",
            config
        );

        let monitor = Arc::new(Self {
            config: config.clone(),
            metrics: metrics.clone(),
            client_stats: RwLock::new(ClientObservedStats::new(config, metrics)),
            authority_aggregator,
            cached_scores: RwLock::new(None),
            tx_type,
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
            TxType::SingleWriter,
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

            /*
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
                                result: Ok(latency),
                            });
                        }
                        Ok(Err(_)) => {
                            let _latency = start.elapsed();
                            monitor.record_interaction_result(OperationFeedback {
                                authority_name: name,
                                display_name: display_name.clone(),
                                operation: OperationType::HealthCheck,
                                result: Err(()),
                            });
                        }
                        Err(_) => {
                            monitor.record_interaction_result(OperationFeedback {
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
            }*/

            // Run latency checks on all validators
            self.clone().run_latency_checks().await;

            self.update_cached_scores();
        }
    }

    /// Run latency checks on all validators using two-phase measurement.
    ///
    /// Phase 1: Get consensus position from each validator
    /// Phase 2: Use position to measure quorum latency similar to get_certified_finalized_effects
    async fn run_latency_checks(self: Arc<Self>) {
        let authority_agg = self.authority_aggregator.load();
        let mut tasks = JoinSet::new();

        for (name, safe_client) in authority_agg.authority_clients.iter() {
            let name = *name;
            let display_name = authority_agg.get_display_name(&name);
            let client = safe_client.clone();
            let timeout_duration = self.config.health_check_timeout;
            let monitor = self.clone();
            let authority_agg_clone = authority_agg.clone();
            let delay_ms = rand::thread_rng().gen_range(0..1_000);

            tasks.spawn(async move {
                monitor
                    .measure_validator_latency(
                        name,
                        display_name,
                        client,
                        authority_agg_clone,
                        timeout_duration,
                        delay_ms,
                    )
                    .await;
            });
        }

        while let Some(result) = tasks.join_next().await {
            if let Err(e) = result {
                warn!("Latency check task failed: {}", e);
            }
        }
    }

    /// Measure latency for a single validator using two-phase approach
    async fn measure_validator_latency(
        self: Arc<Self>,
        validator_name: AuthorityName,
        display_name: String,
        client: Arc<crate::safe_client::SafeClient<A>>,
        authority_agg: Arc<crate::authority_aggregator::AuthorityAggregator<A>>,
        timeout_duration: std::time::Duration,
        delay_ms: u64,
    ) {
        // Add some random delay to the task to avoid all tasks running at the same time
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

        // Get consensus position from this validator
        // Start measuring latency from here and broadcast to all validators
        let start_time = Instant::now();

        let consensus_position = match timeout(
            timeout_duration,
            client.validator_latency(ValidatorLatencyRequest {
                consensus_position: None,
            }),
        )
        .await
        {
            Ok(Ok(response)) => {
                if let Some(pos) = response.consensus_position {
                    pos
                } else {
                    return;
                }
            }
            _ => {
                self.record_interaction_result(OperationFeedback {
                    authority_name: validator_name,
                    display_name: display_name.clone(),
                    operation: OperationType::Finalize,
                    result: Err(()),
                });

                // Give it an end to end latency of timeout seconds
                self.record_interaction_result(OperationFeedback {
                    authority_name: validator_name,
                    display_name: display_name.clone(),
                    operation: OperationType::Finalize,
                    result: Ok(timeout_duration),
                });
                warn!("Failed to get consensus position from {}", display_name);
                return;
            }
        };

        let monitor = self.clone();
        match monitor
            .wait_for_latency_quorum(&authority_agg, consensus_position, timeout_duration)
            .await
        {
            Ok(_) => {
                let end_to_end_latency = start_time.elapsed();
                self.record_interaction_result(OperationFeedback {
                    authority_name: validator_name,
                    display_name: display_name.clone(),
                    operation: OperationType::Finalize,
                    result: Ok(end_to_end_latency),
                });
                self.metrics
                    .observed_latency
                    .with_label_values(&[&display_name, "latency_ping", "synthetic_tx"])
                    .observe(end_to_end_latency.as_secs_f64());
            }
            Err(_) => {
                warn!(
                    "Failed to get latency quorum from validators for {} in consensus position {}",
                    display_name, consensus_position
                );
                // Do not record latency on error
            }
        }
    }

    /// Wait for quorum responses from validators for latency measurement using StatusAggregator
    async fn wait_for_latency_quorum(
        self: Arc<Self>,
        authority_agg: &Arc<crate::authority_aggregator::AuthorityAggregator<A>>,
        consensus_position: sui_types::messages_consensus::ConsensusPosition,
        timeout_duration: std::time::Duration,
    ) -> Result<(), ()> {
        use crate::status_aggregator::StatusAggregator;
        use futures::stream::FuturesUnordered;
        use futures::StreamExt;
        use std::collections::BTreeMap;

        let clients = authority_agg.authority_clients.iter().collect::<Vec<_>>();
        let committee = authority_agg.committee.clone();

        let mut futures = FuturesUnordered::new();
        for (name, client) in clients {
            let client = client.clone();
            let name = *name;
            let display_name = authority_agg.get_display_name(&name);
            let monitor = self.clone();

            let future = async move {
                match timeout(
                    timeout_duration,
                    client.validator_latency(ValidatorLatencyRequest {
                        consensus_position: Some(consensus_position),
                    }),
                )
                .await
                {
                    Ok(Ok(response)) => (name, Ok(response)),
                    Ok(Err(e)) => (name, Err(e)),
                    Err(_) => {
                        monitor.record_interaction_result(OperationFeedback {
                            authority_name: name,
                            display_name: display_name.clone(),
                            operation: OperationType::Finalize,
                            result: Err(()),
                        });
                        (name, Err(sui_types::error::SuiError::TimeoutError))
                    }
                }
            };

            futures.push(future);
        }

        let mut digest_aggregators: BTreeMap<ConsensusPosition, StatusAggregator<()>> =
            BTreeMap::new();

        while let Some((name, response)) = futures.next().await {
            match response {
                Ok(response) => {
                    if let Some(consensus_position) = response.consensus_position {
                        let aggregator = digest_aggregators
                            .entry(consensus_position)
                            .or_insert_with(|| StatusAggregator::<()>::new(committee.clone()));
                        aggregator.insert(name, ());

                        if aggregator.reached_quorum_threshold() {
                            return Ok(());
                        }
                    }
                }
                Err(_) => {
                    // Ignore errors for quorum calculation
                }
            }
        }

        Err(())
    }
}

impl<A: Clone> ValidatorClientMonitor<A> {
    /// Calculate and cache scores for all validators.
    ///
    /// This method is called periodically after health checks complete to update
    /// the cached validator scores.
    fn update_cached_scores(&self) {
        let authority_agg = self.authority_aggregator.load();
        let committee = &authority_agg.committee;
        let tx_type = self.tx_type.as_str();

        let score_map = self.client_stats.read().get_all_validator_stats(
            committee,
            self.tx_type,
            &authority_agg.validator_display_names,
        );

        for (validator, score) in score_map.iter() {
            debug!("Validator [{tx_type}] {}: score {}", validator, score);
            let display_name = authority_agg.get_display_name(validator);
            self.metrics
                .performance_score
                .with_label_values(&[&display_name, tx_type])
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
        let operation_str = match feedback.operation {
            OperationType::Submit => "submit",
            OperationType::Effects => "effects",
            OperationType::HealthCheck => "health_check",
            OperationType::Finalize => "finalize",
        };

        let tx_type = self.tx_type.as_str();

        match feedback.result {
            Ok(latency) => {
                self.metrics
                    .observed_latency
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type])
                    .observe(latency.as_secs_f64());
                self.metrics
                    .operation_success
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type])
                    .inc();
            }
            Err(()) => {
                self.metrics
                    .operation_failure
                    .with_label_values(&[&feedback.display_name, operation_str, tx_type])
                    .inc();
            }
        }

        let mut client_stats = self.client_stats.write();
        client_stats.record_interaction_result(feedback, &self.metrics, self.tx_type);
    }

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

    #[cfg(test)]
    pub fn force_update_cached_scores(&self) {
        self.update_cached_scores();
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

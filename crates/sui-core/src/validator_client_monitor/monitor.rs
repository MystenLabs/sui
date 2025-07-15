// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::validator_client_monitor::stats::ClientObservedStats;
use crate::validator_client_monitor::{
    metrics::ValidatorClientMetrics, OperationFeedback, OperationType,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use rand::{distributions::WeightedIndex, prelude::*};
use std::collections::HashSet;
use std::{sync::Arc, time::Instant};
use sui_config::validator_client_monitor_config::{
    SelectionStrategy, ValidatorClientMonitorConfig,
};
use sui_types::committee::Committee;
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::EpochId,
    messages_grpc::ValidatorHealthRequest,
};
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
    authority_aggregator: Arc<ArcSwap<crate::authority_aggregator::AuthorityAggregator<A>>>,
}

impl<A> ValidatorClientMonitor<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        config: ValidatorClientMonitorConfig,
        metrics: Arc<ValidatorClientMetrics>,
        authority_aggregator: Arc<ArcSwap<crate::authority_aggregator::AuthorityAggregator<A>>>,
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
        });

        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            monitor_clone.run_health_checks().await;
        });

        monitor
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Records operation results including success/failure status and latency
    /// from the client's perspective. Updates both Prometheus metrics and
    /// internal client statistics. This is the primary interface for the
    /// TransactionDriver to report client-observed validator interactions.
    pub fn record_interaction_result(&self, feedback: OperationFeedback) {
        let validator_str = feedback.validator.concise().to_string();
        let operation_str = match feedback.operation {
            OperationType::Submit => "submit",
            OperationType::Effects => "effects",
            OperationType::HealthCheck => "health_check",
        };

        // Update metrics
        if let Some(actual_latency) = feedback.latency {
            self.metrics
                .operation_latency
                .with_label_values(&[&validator_str, operation_str])
                .observe(actual_latency.as_secs_f64());
        }

        if feedback.success {
            self.metrics
                .operation_success
                .with_label_values(&[&validator_str, operation_str])
                .inc();
        } else {
            self.metrics
                .operation_failure
                .with_label_values(&[&validator_str, operation_str])
                .inc();
        }

        // Update client stats
        let mut client_stats = self.client_stats.write();
        client_stats.record_interaction_result(feedback);
    }

    /// Handle epoch changes by cleaning up stale validator data.
    ///
    /// Called when the network transitions to a new epoch. Removes client-observed
    /// data for validators that are no longer in the active committee to prevent
    /// memory leaks and ensure scores are only calculated for current validators.
    pub fn on_epoch_change(&self, epoch: EpochId) {
        let authority_agg = self.authority_aggregator.load();
        if authority_agg.committee.epoch == epoch {
            return;
        }

        let current_validators: HashSet<_> =
            authority_agg.authority_clients.keys().cloned().collect();

        let mut client_stats = self.client_stats.write();
        client_stats.refresh_validator_set(&current_validators);
    }

    /// Select a validator based on client-observed performance.
    ///
    /// Uses the configured selection strategy (WeightedRandom or TopK) to choose
    /// a validator from the provided committee. The selection considers:
    /// - Current client-observed scores (reliability + latency)
    /// - Exclusion status (temporarily excluded validators are filtered out)
    /// - Committee membership (only selects from the provided committee)
    ///
    /// We need to pass the current committee here because it is possible
    /// that the fullnode is in the middle of a committee change when this
    /// is called, and we need to maintain an invariant that the selected
    /// validator is always in the committee passed in.
    ///
    /// Falls back to random selection if no client performance data is available.
    pub fn select_preferred_validator(&self, committee: &Committee) -> AuthorityName {
        // Get all validators with their scores
        let validator_scores = self.client_stats.read().calculate_all_client_scores();

        // Filter out excluded validators that are still in the current committee
        let available_validators: Vec<_> = validator_scores
            .into_iter()
            .filter(|(validator, score)| {
                // Must be in current committee
                if !committee.authority_exists(validator) {
                    return false;
                }
                self.metrics
                    .performance_score
                    .with_label_values(&[&validator.concise().to_string()])
                    .set(*score);

                true
            })
            .collect();

        if available_validators.is_empty() {
            // Fallback: select random validator from current committee
            let validator = *committee.sample();

            debug!(
                "No available validators, selecting random validator {} from current committee",
                validator.concise()
            );

            self.metrics
                .validator_selections
                .with_label_values(&[&validator.concise().to_string()])
                .inc();

            return validator;
        }

        // Select based on configured strategy
        match &self.config.selection_strategy {
            SelectionStrategy::WeightedRandom { temperature } => {
                self.select_weighted_random(available_validators, *temperature)
            }
            SelectionStrategy::TopK { k } => self.select_top_k(available_validators, *k),
        }
    }

    /// Select validator using weighted random strategy with softmax.
    ///
    /// Applies softmax function with temperature parameter to convert scores
    /// into selection probabilities. Higher temperature values make selection
    /// more uniform, while lower values favor higher-scoring validators.
    fn select_weighted_random(
        &self,
        validators: Vec<(AuthorityName, f64)>,
        temperature: f64,
    ) -> AuthorityName {
        // Apply softmax with temperature
        let scores: Vec<f64> = validators
            .iter()
            .map(|(_, score)| (*score / temperature).exp())
            .collect();

        let sum: f64 = scores.iter().sum();
        let weights: Vec<f64> = scores.iter().map(|s| s / sum).collect();

        let dist = WeightedIndex::new(&weights).unwrap();
        let mut rng = thread_rng();
        let idx = dist.sample(&mut rng);

        let (validator, score) = validators[idx];

        debug!(
            "Selected validator {} using weighted random strategy with score: {}",
            validator.concise(),
            score,
        );

        self.metrics
            .validator_selections
            .with_label_values(&[&validator.concise().to_string()])
            .inc();

        validator
    }

    /// Select validator using top-K strategy.
    ///
    /// Sorts validators by score and randomly selects from the top K performers.
    /// This provides a balance between preferring good validators and avoiding
    /// overloading any single validator.
    fn select_top_k(&self, validators: Vec<(AuthorityName, f64)>, k: usize) -> AuthorityName {
        let mut sorted = validators;
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let k = k.min(sorted.len());
        let top_k = &sorted[..k];

        // Round-robin among top-k
        let idx = thread_rng().gen_range(0..k);
        let (validator, score) = top_k[idx];
        debug!(
            "Selected validator {} using top-k strategy with score: {}",
            validator.concise(),
            score,
        );

        self.metrics
            .validator_selections
            .with_label_values(&[&validator.concise().to_string()])
            .inc();

        validator
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
            let mut tasks = JoinSet::new();

            for (name, safe_client) in authority_agg.authority_clients.iter() {
                let name = *name;
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
                        Ok(Ok(_response)) => {
                            let latency = start.elapsed();
                            monitor.record_interaction_result(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency: Some(latency),
                                success: true,
                            });
                        }
                        Ok(Err(_)) => {
                            let latency = start.elapsed();
                            monitor.record_interaction_result(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency: Some(latency),
                                success: false,
                            });
                        }
                        Err(_) => {
                            // Timeout - don't include latency as it would pollute the numbers
                            monitor.record_interaction_result(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency: None,
                                success: false,
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
        }
    }
}

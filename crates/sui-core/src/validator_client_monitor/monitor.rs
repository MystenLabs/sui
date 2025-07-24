// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::validator_client_monitor::stats::ClientObservedStats;
use crate::validator_client_monitor::{
    metrics::ValidatorClientMetrics, OperationFeedback, OperationType,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use std::collections::HashSet;
use std::{sync::Arc, time::Instant};
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
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
use tracing::{info, warn};

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

    #[cfg(test)]
    pub fn new_for_test(
        authority_aggregator: Arc<ArcSwap<crate::authority_aggregator::AuthorityAggregator<A>>>,
    ) -> Arc<Self> {
        use prometheus::Registry;

        Self::new(
            ValidatorClientMonitorConfig::default(),
            Arc::new(ValidatorClientMetrics::new(&Registry::default())),
            authority_aggregator,
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

impl<A: Clone> ValidatorClientMonitor<A> {
    /// Record client-observed interaction result with a validator.
    ///
    /// Records operation results including success/failure status and latency
    /// from the client's perspective. Updates both Prometheus metrics and
    /// internal client statistics. This is the primary interface for the
    /// TransactionDriver to report client-observed validator interactions.
    /// TODO: Consider adding a byzantine flag to the feedback.
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

    /// Select validators based on client-observed performance with shuffled top k.
    ///
    /// First selects the top `k` validators by performance score, shuffles them,
    /// then appends the remaining validators ordered by score. The selection considers:
    /// - Current client-observed scores (reliability + latency)
    /// - Exclusion status (temporarily excluded validators are filtered out)
    /// - Committee membership (only selects from the provided committee)
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
    ) -> Vec<AuthorityName> {
        // Get all validators with their scores
        // TODO: If this is becoming a bottleneck, we should consider caching the scores,
        // and only recalculate every few seconds.
        let validator_scores = self.client_stats.read().calculate_all_client_scores();

        // Filter out excluded validators that are still in the current committee
        let mut available_validators: Vec<_> = validator_scores
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

        // Sort by score (highest first)
        available_validators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Split into top k and rest
        let (top_k_with_scores, rest_with_scores) =
            available_validators.split_at(k.min(available_validators.len()));

        // Extract validator names from top k and shuffle them
        let mut top_k_validators: Vec<_> = top_k_with_scores
            .iter()
            .map(|(validator, _)| *validator)
            .collect();
        let mut rng = rand::thread_rng();
        top_k_validators.shuffle(&mut rng);

        // Extract rest validators (already sorted by score)
        let rest_validators: Vec<_> = rest_with_scores
            .iter()
            .map(|(validator, _)| *validator)
            .collect();

        // Combine shuffled top k with sorted rest
        let mut result = top_k_validators;
        result.extend(rest_validators);

        // If we don't have enough validators with scores, fill with random validators from committee
        let result_set: HashSet<_> = result.iter().cloned().collect();
        let mut unscored_validators: Vec<_> = committee
            .members()
            .filter(|(validator, _)| !result_set.contains(validator))
            .map(|(validator, _)| *validator)
            .collect();

        // Shuffle unscored validators and append
        unscored_validators.shuffle(&mut rng);
        result.extend(unscored_validators);

        result
    }
}

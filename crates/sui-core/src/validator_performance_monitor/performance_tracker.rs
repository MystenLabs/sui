// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::AuthorityAPI;
use crate::validator_performance_monitor::{
    config::{SelectionStrategy, ValidatorPerformanceConfig},
    metrics::ValidatorPerformanceMetrics,
    score_calculator::{PerformanceScore, ScoreCalculator, ValidatorStats},
    OperationFeedback, OperationType,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use rand::{distributions::WeightedIndex, prelude::*};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use sui_types::{
    base_types::{AuthorityName, ConciseableName},
    committee::EpochId,
    messages_grpc::ValidatorHealthRequest,
};
use tokio::{
    task::JoinSet,
    time::{interval, timeout},
};
use tracing::{debug, warn};

/// Record of validator performance over time
#[derive(Debug, Clone)]
pub struct ValidatorPerformanceRecord {
    pub validator: AuthorityName,
    pub score: PerformanceScore,
    pub stats: ValidatorStats,
}

/// Output of validator selection
#[derive(Debug, Clone)]
pub struct ValidatorSelectionOutput {
    pub validator: AuthorityName,
    pub score: f64,
    pub reason: SelectionReason,
}

#[derive(Debug, Clone)]
pub enum SelectionReason {
    BestScore,
    WeightedRandom,
    TopK(usize),
    EpsilonGreedy,
    Fallback,
}

#[derive(Default)]
pub struct ValidatorData {
    pub stats: ValidatorStats,
    pub score: PerformanceScore,
    /// Time when validator was temporarily excluded
    pub exclusion_time: Option<Instant>,
}

pub struct ValidatorPerformanceMonitor<A: Clone> {
    config: ValidatorPerformanceConfig,
    metrics: Arc<ValidatorPerformanceMetrics>,
    validator_data: Arc<RwLock<HashMap<AuthorityName, ValidatorData>>>,
    score_calculator: RwLock<ScoreCalculator>,
    /// ArcSwap reference to the current authority aggregator
    authority_aggregator: Arc<ArcSwap<crate::authority_aggregator::AuthorityAggregator<A>>>,
    current_epoch: parking_lot::RwLock<EpochId>,
}

impl<A> ValidatorPerformanceMonitor<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        config: ValidatorPerformanceConfig,
        metrics: Arc<ValidatorPerformanceMetrics>,
        authority_aggregator: Arc<ArcSwap<crate::authority_aggregator::AuthorityAggregator<A>>>,
    ) -> Arc<Self> {
        let initial_epoch = authority_aggregator.load().committee.epoch;

        let monitor = Arc::new(Self {
            config: config.clone(),
            metrics,
            validator_data: Arc::new(RwLock::new(HashMap::new())),
            score_calculator: RwLock::new(ScoreCalculator::new(config)),
            authority_aggregator,
            current_epoch: parking_lot::RwLock::new(initial_epoch),
        });

        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            monitor_clone.run_health_checks().await;
        });

        monitor
    }

    /// Process feedback from TransactionDriver operations
    pub fn record_feedback(&self, feedback: OperationFeedback) {
        let operation_str = match feedback.operation {
            OperationType::Submit => "submit",
            OperationType::Effects => "effects",
            OperationType::HealthCheck => "health_check",
        };

        // Unified handling for all operation types
        // Submit operations with non-timeout errors are handled at the caller level
        // by not passing the error and marking as success for latency tracking only
        self.record_operation_result(
            &feedback.validator,
            operation_str,
            feedback.success,
            feedback.latency,
            feedback.error.as_deref(),
        );
    }

    /// Handle epoch changes by cleaning up stale validator data
    pub fn on_epoch_change(&self, new_epoch: EpochId) {
        let mut current_epoch = self.current_epoch.write();
        if new_epoch <= *current_epoch {
            return;
        }

        *current_epoch = new_epoch;

        let authority_agg = self.authority_aggregator.load();
        let current_validators: std::collections::HashSet<_> =
            authority_agg.authority_clients.keys().cloned().collect();

        let mut data = self.validator_data.write();

        // Remove data for validators no longer in the active set
        let stale_validators: Vec<_> = data
            .keys()
            .filter(|validator| !current_validators.contains(validator))
            .cloned()
            .collect();

        for validator in stale_validators {
            data.remove(&validator);
            debug!("Removed stale validator data for {}", validator.concise());
        }

        debug!(
            "Epoch changed to {}, cleaned up {} stale validators",
            new_epoch,
            data.len()
        );
    }

    /// Select a validator based on performance
    pub fn select_validator(&self) -> ValidatorSelectionOutput {
        let authority_agg = self.authority_aggregator.load();

        // Check for epoch changes and cleanup if needed
        let current_epoch = authority_agg.committee.epoch;
        if current_epoch > *self.current_epoch.read() {
            self.on_epoch_change(current_epoch);
        }

        let data = self.validator_data.read();

        // Filter out excluded validators that are still in the current committee
        let available_validators: Vec<(&AuthorityName, &ValidatorData)> = data
            .iter()
            .filter(|(validator, vdata)| {
                // Must be in current committee
                if !authority_agg.authority_clients.contains_key(validator) {
                    return false;
                }

                // Check if validator is temporarily excluded
                if let Some(exclusion_time) = vdata.exclusion_time {
                    if exclusion_time.elapsed() < self.config.failure_cooldown {
                        return false;
                    }
                }

                let total_ops = vdata.stats.success_count + vdata.stats.failure_count;
                total_ops >= self.config.min_samples as u64
            })
            .collect();

        if available_validators.is_empty() {
            // Fallback: select random validator from current committee
            let all_validators: Vec<&AuthorityName> =
                authority_agg.authority_clients.keys().collect();
            let validator = all_validators
                .choose(&mut thread_rng())
                .expect("No validators available");

            return ValidatorSelectionOutput {
                validator: **validator,
                score: 0.5,
                reason: SelectionReason::Fallback,
            };
        }

        // Select based on configured strategy
        match &self.config.selection_strategy {
            SelectionStrategy::WeightedRandom { temperature } => {
                self.select_weighted_random(&available_validators, *temperature)
            }
            SelectionStrategy::TopK { k } => self.select_top_k(&available_validators, *k),
            SelectionStrategy::EpsilonGreedy { epsilon } => {
                self.select_epsilon_greedy(&available_validators, *epsilon)
            }
        }
    }

    /// Get current performance records for all validators
    pub fn get_performance_records(&self) -> Vec<ValidatorPerformanceRecord> {
        let data = self.validator_data.read();
        data.iter()
            .map(|(validator, vdata)| ValidatorPerformanceRecord {
                validator: *validator,
                score: vdata.score.clone(),
                stats: vdata.stats.clone(),
            })
            .collect()
    }

    fn record_operation_result(
        &self,
        validator: &AuthorityName,
        operation: &str,
        success: bool,
        latency: Duration,
        error: Option<&str>,
    ) {
        let validator_str = validator.concise().to_string();

        // Update metrics
        self.metrics
            .operation_latency
            .with_label_values(&[&validator_str, operation])
            .observe(latency.as_secs_f64());

        if success {
            self.metrics
                .operation_success
                .with_label_values(&[&validator_str, operation])
                .inc();
        } else {
            let error_type = error.unwrap_or("unknown");
            self.metrics
                .operation_failure
                .with_label_values(&[&validator_str, operation, error_type])
                .inc();
        }

        // Update validator data
        let mut data = self.validator_data.write();
        let vdata = data.entry(*validator).or_default();

        if success {
            vdata.stats.success_count += 1;
            vdata.stats.consecutive_failures = 0;
            vdata.stats.last_success = Some(Instant::now());
        } else {
            vdata.stats.failure_count += 1;
            vdata.stats.consecutive_failures += 1;
            vdata.stats.last_failure = Some(Instant::now());

            // Check for exclusion
            if vdata.stats.consecutive_failures >= self.config.max_consecutive_failures {
                vdata.exclusion_time = Some(Instant::now());
                vdata.stats.consecutive_failures = 0; // Reset counter
            }
        }

        // Update EMA latency for the specific operation type
        self.update_ema_latency(vdata, operation, latency);

        // Update metrics
        self.metrics
            .consecutive_failures
            .with_label_values(&[&validator_str])
            .set(vdata.stats.consecutive_failures as i64);

        if let Some(last_success) = vdata.stats.last_success {
            self.metrics
                .time_since_last_success
                .with_label_values(&[&validator_str])
                .set(last_success.elapsed().as_secs_f64());
        }

        // Recalculate score
        drop(data); // Release write lock
        self.recalculate_scores();
    }

    fn update_ema_latency(
        &self,
        vdata: &mut ValidatorData,
        operation: &str,
        new_latency: Duration,
    ) {
        // EMA smoothing factor: 0.2 means 20% weight to new value, 80% to historical
        const EMA_ALPHA: f64 = 0.2;

        // Helper function to calculate EMA
        let calculate_ema = |current_latency: Duration| -> Duration {
            if current_latency.is_zero() {
                new_latency
            } else {
                let current_secs = current_latency.as_secs_f64();
                let new_secs = new_latency.as_secs_f64();
                let ema_secs = EMA_ALPHA * new_secs + (1.0 - EMA_ALPHA) * current_secs;
                Duration::from_secs_f64(ema_secs)
            }
        };

        // Use HashMap to avoid code duplication
        let mut latency_map = HashMap::from([
            ("submit", &mut vdata.stats.ema_submit_latency),
            ("effects", &mut vdata.stats.ema_effects_latency),
            ("health_check", &mut vdata.stats.ema_health_check_latency),
        ]);

        if let Some(latency_field) = latency_map.get_mut(operation) {
            **latency_field = calculate_ema(**latency_field);
        } else {
            // Default to submit latency for unknown operation types
            vdata.stats.ema_submit_latency = calculate_ema(vdata.stats.ema_submit_latency);
        }
    }

    fn recalculate_scores(&self) {
        let mut data = self.validator_data.write();
        let all_stats: HashMap<AuthorityName, ValidatorStats> = data
            .iter()
            .map(|(name, vdata)| (*name, vdata.stats.clone()))
            .collect();

        // Update global stats
        self.score_calculator
            .write()
            .update_global_stats(&all_stats);

        // Calculate scores for each validator
        let score_calc = self.score_calculator.read();
        for (validator, vdata) in data.iter_mut() {
            let score = score_calc.calculate_score(&vdata.stats);
            vdata.score = score;

            // Update metric
            self.metrics
                .performance_score
                .with_label_values(&[&validator.concise().to_string()])
                .set(vdata.score.overall_score);
        }
    }

    fn select_weighted_random(
        &self,
        validators: &[(&AuthorityName, &ValidatorData)],
        temperature: f64,
    ) -> ValidatorSelectionOutput {
        // Apply softmax with temperature
        let scores: Vec<f64> = validators
            .iter()
            .map(|(_, vdata)| (vdata.score.overall_score / temperature).exp())
            .collect();

        let sum: f64 = scores.iter().sum();
        let weights: Vec<f64> = scores.iter().map(|s| s / sum).collect();

        let dist = WeightedIndex::new(&weights).unwrap();
        let mut rng = thread_rng();
        let idx = dist.sample(&mut rng);

        let (validator, vdata) = validators[idx];

        self.metrics
            .validator_selections
            .with_label_values(&[&validator.concise().to_string()])
            .inc();

        ValidatorSelectionOutput {
            validator: *validator,
            score: vdata.score.overall_score,
            reason: SelectionReason::WeightedRandom,
        }
    }

    fn select_top_k(
        &self,
        validators: &[(&AuthorityName, &ValidatorData)],
        k: usize,
    ) -> ValidatorSelectionOutput {
        let mut sorted: Vec<_> = validators.to_vec();
        sorted.sort_by(|a, b| {
            b.1.score
                .overall_score
                .partial_cmp(&a.1.score.overall_score)
                .unwrap()
        });

        let k = k.min(sorted.len());
        let top_k = &sorted[..k];

        // Round-robin among top-k
        let idx = thread_rng().gen_range(0..k);
        let (validator, vdata) = top_k[idx];

        self.metrics
            .validator_selections
            .with_label_values(&[&validator.concise().to_string()])
            .inc();

        ValidatorSelectionOutput {
            validator: *validator,
            score: vdata.score.overall_score,
            reason: SelectionReason::TopK(k),
        }
    }

    fn select_epsilon_greedy(
        &self,
        validators: &[(&AuthorityName, &ValidatorData)],
        epsilon: f64,
    ) -> ValidatorSelectionOutput {
        let mut rng = thread_rng();

        if rng.gen::<f64>() < epsilon {
            // Random selection
            let idx = rng.gen_range(0..validators.len());
            let (validator, vdata) = validators[idx];

            self.metrics
                .validator_selections
                .with_label_values(&[&validator.concise().to_string()])
                .inc();

            ValidatorSelectionOutput {
                validator: *validator,
                score: vdata.score.overall_score,
                reason: SelectionReason::EpsilonGreedy,
            }
        } else {
            // Select best
            let best = validators
                .iter()
                .max_by(|a, b| {
                    a.1.score
                        .overall_score
                        .partial_cmp(&b.1.score.overall_score)
                        .unwrap()
                })
                .unwrap();

            self.metrics
                .validator_selections
                .with_label_values(&[&best.0.concise().to_string()])
                .inc();

            ValidatorSelectionOutput {
                validator: *best.0,
                score: best.1.score.overall_score,
                reason: SelectionReason::BestScore,
            }
        }
    }

    async fn run_health_checks(&self) {
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
                            monitor.record_feedback(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency,
                                success: true,
                                error: None,
                            });
                        }
                        Ok(Err(e)) => {
                            let latency = start.elapsed();
                            monitor.record_feedback(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency,
                                success: false,
                                error: Some(e.to_string()),
                            });
                        }
                        Err(_) => {
                            let latency = start.elapsed();
                            monitor.record_feedback(OperationFeedback {
                                validator: name,
                                operation: OperationType::HealthCheck,
                                latency,
                                success: false,
                                error: Some("timeout".to_string()),
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

impl<A: Clone> Clone for ValidatorPerformanceMonitor<A> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            validator_data: self.validator_data.clone(),
            score_calculator: RwLock::new(ScoreCalculator::new(self.config.clone())),
            authority_aggregator: self.authority_aggregator.clone(),
            current_epoch: parking_lot::RwLock::new(*self.current_epoch.read()),
        }
    }
}

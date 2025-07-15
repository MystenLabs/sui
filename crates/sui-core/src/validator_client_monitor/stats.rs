// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_client_monitor::{OperationFeedback, OperationType};
use mysten_common::decay_moving_average::DecayMovingAverage;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::base_types::AuthorityName;
use tracing::debug;

/// Decay factor for reliability EMA - lower values give more weight to recent observations
const RELIABILITY_DECAY_FACTOR: f64 = 0.5;
/// Decay factor for latency EMA - higher values smooth out spikes better
const LATENCY_DECAY_FACTOR: f64 = 0.9;
/// Decay factor for max latency - higher values keep max stable over time
const MAX_LATENCY_DECAY_FACTOR: f64 = 0.99;

/// Complete client-observed statistics for validator interactions.
///
/// This struct maintains client-side metrics for all validators in the network,
/// including reliability scores, latency measurements, and failure tracking
/// as observed from the client's perspective. It uses exponential moving averages (EMA)
/// to smooth out transient spikes while still responding to sustained changes.
#[derive(Debug, Clone)]
pub struct ClientObservedStats {
    /// Per-validator statistics mapping validator names to their client-observed metrics
    pub validator_stats: HashMap<AuthorityName, ValidatorClientMetrics>,
    /// Global statistics used for normalization and comparison
    pub global_stats: GlobalStats,
    /// Configuration parameters for scoring and exclusion policies
    pub config: ValidatorClientMonitorConfig,
}

/// Client-observed metrics for a single validator.
///
/// Tracks reliability, latency, and failure patterns for a specific validator
/// as observed from the client's perspective. Uses exponential moving averages
/// to smooth measurements while maintaining responsiveness to changes.
#[derive(Debug, Clone)]
pub struct ValidatorClientMetrics {
    /// Exponential moving average of success rate (0.0 to 1.0)
    pub reliability: DecayMovingAverage,
    /// EMA latencies for each operation type (Submit, Effects, HealthCheck)
    pub average_latencies: HashMap<OperationType, DecayMovingAverage>,
    /// Counter for consecutive failures - resets on success
    pub consecutive_failures: u32,
    /// Time when validator was temporarily excluded due to failures.
    /// Validators are excluded when consecutive_failures >= max_consecutive_failures
    pub exclusion_time: Option<Instant>,
}

impl ValidatorClientMetrics {
    pub fn new(init_reliability: f64) -> Self {
        Self {
            reliability: DecayMovingAverage::new(init_reliability, RELIABILITY_DECAY_FACTOR),
            average_latencies: HashMap::new(),
            consecutive_failures: 0,
            exclusion_time: None,
        }
    }

    pub fn update_average_latency(&mut self, operation: OperationType, new_latency: Duration) {
        match self.average_latencies.entry(operation) {
            Entry::Occupied(mut entry) => {
                entry
                    .get_mut()
                    .update_moving_average(new_latency.as_secs_f64());
            }
            Entry::Vacant(entry) => {
                entry.insert(DecayMovingAverage::new(
                    new_latency.as_secs_f64(),
                    LATENCY_DECAY_FACTOR,
                ));
            }
        }
    }
}

/// Global statistics across all validators.
///
/// Used to track network-wide performance metrics that serve as baselines
/// for scoring individual validators. Currently tracks maximum latencies
/// for normalization purposes.
#[derive(Debug, Clone, Default)]
pub struct GlobalStats {
    /// Maximum observed latencies for each operation type across all validators.
    /// Used to normalize individual validator latencies in score calculations.
    pub max_latencies: HashMap<OperationType, DecayMovingAverage>,
}

impl ClientObservedStats {
    pub fn new(config: ValidatorClientMonitorConfig) -> Self {
        Self {
            validator_stats: HashMap::new(),
            global_stats: GlobalStats::default(),
            config,
        }
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Updates reliability scores, latency measurements, and failure counts
    /// based on client observations. Automatically excludes validators that
    /// exceed the maximum consecutive failure threshold.
    pub fn record_interaction_result(&mut self, feedback: OperationFeedback) {
        let reliability = if feedback.success { 1.0 } else { 0.0 };

        let validator_stats = self
            .validator_stats
            .entry(feedback.validator)
            .or_insert_with(|| ValidatorClientMetrics::new(reliability));

        if feedback.success {
            validator_stats
                .reliability
                .update_moving_average(reliability);
            validator_stats.consecutive_failures = 0;
        } else {
            validator_stats
                .reliability
                .update_moving_average(reliability);
            validator_stats.consecutive_failures += 1;

            // Exclude validator temporarily after too many consecutive failures
            if validator_stats.consecutive_failures >= self.config.max_consecutive_failures {
                validator_stats.exclusion_time = Some(Instant::now());
            }
        }

        if let Some(actual_latency) = feedback.latency {
            validator_stats.update_average_latency(feedback.operation, actual_latency);
            self.update_global_stats(feedback.operation, actual_latency);
        }
    }

    /// Update global maximum latency statistics.
    ///
    /// For max latencies, we use a special update strategy:
    /// - If the new latency is higher than the current max, we immediately update to it
    /// - Otherwise, we apply decay to gradually lower the max over time
    ///
    /// This ensures we always capture peak latencies while still allowing the max to decrease
    /// when network conditions improve to reduce the impact of outliers.
    pub fn update_global_stats(&mut self, operation: OperationType, latency: Duration) {
        let latency_secs = latency.as_secs_f64();

        match self.global_stats.max_latencies.entry(operation) {
            Entry::Occupied(mut entry) => {
                let current_max = entry.get().get();
                if latency_secs > current_max {
                    // New latency is higher - immediately update to this value
                    entry.get_mut().override_moving_average(latency_secs);
                } else {
                    // New latency is lower - apply decay to gradually reduce max
                    entry.get_mut().update_moving_average(latency_secs);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(DecayMovingAverage::new(
                    latency_secs,
                    MAX_LATENCY_DECAY_FACTOR,
                ));
            }
        }
    }

    /// Calculate client-observed scores for all validators.
    ///
    /// Returns a map of validator names to their computed scores based on
    /// client-side observations. Validators that are currently excluded or
    /// missing required data will not be included in the results.
    pub fn calculate_all_client_scores(&self) -> HashMap<AuthorityName, f64> {
        let mut scores = HashMap::new();
        for (validator, stats) in self.validator_stats.iter() {
            if let Some(score) = self.calculate_client_score(stats, &self.global_stats) {
                scores.insert(*validator, score);
            }
        }

        scores
    }

    /// Calculate client-observed score for a single validator.
    ///
    /// The score combines reliability and latency metrics as observed by the client,
    /// weighted according to configuration. Returns None if:
    /// - The validator is currently in exclusion cooldown period
    /// - The validator is missing data for any operation type
    /// - Global stats are missing for normalization
    ///
    /// Score calculation:
    /// 1. Latency scores are normalized against global maximums
    /// 2. Each operation type has its own weight
    /// 3. Final score = (weighted_latency_score * latency_weight) + (reliability * reliability_weight)
    fn calculate_client_score(
        &self,
        stats: &ValidatorClientMetrics,
        global_stats: &GlobalStats,
    ) -> Option<f64> {
        // Check if validator is still in exclusion cooldown
        if let Some(exclusion_time) = stats.exclusion_time {
            if exclusion_time.elapsed() < self.config.failure_cooldown {
                return None;
            }
        }

        let mut latency_score = 0.0;
        for op in OperationType::iter() {
            // Validator must have data for all operation types
            let latency = stats.average_latencies.get(&op)?.get();
            let max_latency = global_stats.max_latencies.get(&op)?.get();

            // Lower latency ratios are better (inverted for scoring)
            let latency_ratio = latency / max_latency;
            let latency_weight = match op {
                OperationType::Submit => self.config.score_weights.submit_latency_weight,
                OperationType::Effects => self.config.score_weights.effects_latency_weight,
                OperationType::HealthCheck => self.config.score_weights.health_check_latency_weight,
            };

            latency_score += 1.0 - latency_ratio * latency_weight;
        }

        let reliability_score = stats.reliability.get();
        let score = latency_score * self.config.score_weights.latency
            + reliability_score * self.config.score_weights.reliability;

        Some(score)
    }

    /// Remove validators that are no longer in the committee.
    ///
    /// Called during epoch changes to clean up statistics for validators
    /// that have left the active set. This prevents memory leaks and
    /// ensures scores are only calculated for current validators.
    pub fn refresh_validator_set(&mut self, new_committee_validators: &HashSet<AuthorityName>) {
        let cur_len = self.validator_stats.len();
        self.validator_stats
            .retain(|validator, _| new_committee_validators.contains(validator));
        let removed_count = cur_len - self.validator_stats.len();
        debug!("Removed {} stale validator data", removed_count,);
    }
}

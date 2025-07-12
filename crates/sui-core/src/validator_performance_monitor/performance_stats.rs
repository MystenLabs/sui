// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_performance_monitor::{
    OperationFeedback, OperationType, ValidatorPerformanceConfig,
};
use mysten_common::decay_moving_average::DecayMovingAverage;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;
use sui_types::base_types::AuthorityName;
use tracing::debug;

const RELIABILITY_DECAY_FACTOR: f64 = 0.5;
const LATENCY_DECAY_FACTOR: f64 = 0.1;

/// Complete performance statistics for the validator monitoring system
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Per-validator statistics
    pub validator_stats: HashMap<AuthorityName, ValidatorStats>,
    /// Global statistics
    pub global_stats: GlobalStats,
    pub config: ValidatorPerformanceConfig,
}

/// Statistics for a single validator
#[derive(Debug, Clone)]
pub struct ValidatorStats {
    pub reliability: DecayMovingAverage,
    /// EMA latencies for each operation type
    pub average_latencies: HashMap<OperationType, DecayMovingAverage>,
    /// Consecutive failures
    pub consecutive_failures: u32,
    /// Time when validator was temporarily excluded
    pub exclusion_time: Option<Instant>,
}

impl ValidatorStats {
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

/// Global statistics across all validators
#[derive(Debug, Clone, Default)]
pub struct GlobalStats {
    pub max_latencies: HashMap<OperationType, DecayMovingAverage>,
}

impl PerformanceStats {
    pub fn new(config: ValidatorPerformanceConfig) -> Self {
        Self {
            validator_stats: HashMap::new(),
            global_stats: GlobalStats::default(),
            config,
        }
    }

    pub fn record_feedback(&mut self, feedback: OperationFeedback) {
        let reliability = if feedback.success { 1.0 } else { 0.0 };

        let validator_stats = self
            .validator_stats
            .entry(feedback.validator)
            .or_insert_with(|| ValidatorStats::new(reliability));

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

            if validator_stats.consecutive_failures >= self.config.max_consecutive_failures {
                validator_stats.exclusion_time = Some(Instant::now());
            }
        }

        if let Some(actual_latency) = feedback.latency {
            validator_stats.update_average_latency(feedback.operation, actual_latency);
            self.update_global_stats(feedback.operation, actual_latency);
        }
    }

    /// Update global statistics based on current validator stats
    pub fn update_global_stats(&mut self, operation: OperationType, latency: Duration) {
        match self.global_stats.max_latencies.entry(operation) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().update_moving_average(latency.as_secs_f64());
            }
            Entry::Vacant(entry) => {
                entry.insert(DecayMovingAverage::new(
                    latency.as_secs_f64(),
                    LATENCY_DECAY_FACTOR,
                ));
            }
        }
    }

    pub fn calculate_all_scores(&self) -> HashMap<AuthorityName, f64> {
        let mut scores = HashMap::new();
        for (validator, stats) in self.validator_stats.iter() {
            if let Some(score) = self.calculate_validator_score(stats, &self.global_stats) {
                scores.insert(validator, score);
            }
        }

        scores
    }

    fn calculate_validator_score(
        &self,
        stats: &ValidatorStats,
        global_stats: &GlobalStats,
    ) -> Option<f64> {
        let mut latency_score = 0.0;
        for op in OperationType::iter() {
            if let Some(exclusion_time) = stats.exclusion_time {
                if exclusion_time.elapsed() < self.config.failure_cooldown {
                    return None;
                }
            }

            let latency = stats.average_latencies.get(&op)?.get();
            let max_latency = global_stats.max_latencies.get(&op)?.get();

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
    pub fn refresh_validator_set(&mut self, new_committee_validators: &HashSet<AuthorityName>) {
        let cur_len = self.validator_stats.len();
        self.validator_stats
            .retain(|validator, _| new_committee_validators.contains(validator));
        let removed_count = cur_len - self.validator_stats.len();
        debug!("Removed {} stale validator data", removed_count,);
    }
}

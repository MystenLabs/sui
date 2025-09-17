// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_client_monitor::{
    OperationFeedback, OperationType, TxType, ValidatorClientMetrics,
};
use mysten_common::moving_window::MovingWindow;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::base_types::AuthorityName;
use sui_types::committee::Committee;
use tracing::debug;

// TODO: A few optimization to consider:
// 1. There may be times when the entire network is unstable, and in that case
//    we may not want to punish validators when they have errors.
// 2. Some reports are more critical than others. For example, a health check
//    report is more critical than a submit report in terms of failures status.

/// Size of the moving window for reliability measurements
const RELIABILITY_MOVING_WINDOW_SIZE: usize = 100;
/// Size of the moving window for latency measurements
const LATENCY_MOVING_WINDOW_SIZE: usize = 100;

/// Complete client-observed statistics for validator interactions.
///
/// This struct maintains client-side metrics for all validators in the network,
/// including reliability scores, latency measurements, and failure tracking
/// as observed from the client's perspective. It uses exponential moving averages (EMA)
/// to smooth out transient spikes while still responding to sustained changes.
#[derive(Debug, Clone)]
pub struct ClientObservedStats {
    /// Per-validator statistics mapping authority names to their client-observed metrics
    pub validator_stats: HashMap<AuthorityName, ValidatorClientStats>,
    /// Configuration parameters for scoring and exclusion policies
    pub config: ValidatorClientMonitorConfig,
}

/// Client-observed stats for a single validator.
///
/// Tracks reliability, latency, and failure patterns for a specific validator
/// as observed from the client's perspective. Uses exponential moving averages
/// to smooth measurements while maintaining responsiveness to changes.
#[derive(Debug, Clone)]
pub struct ValidatorClientStats {
    /// Moving window of success rate (0.0 to 1.0)
    pub reliability: MovingWindow,
    /// Moving window of latencies for each operation type (Submit, Effects, HealthCheck)
    pub average_latencies: HashMap<OperationType, MovingWindow>,
    /// Counter for consecutive failures - resets on success
    pub consecutive_failures: u32,
    /// Time when validator was temporarily excluded due to failures.
    /// Validators are excluded when consecutive_failures >= max_consecutive_failures
    pub exclusion_time: Option<Instant>,
}

impl ValidatorClientStats {
    pub fn new(init_reliability: f64) -> Self {
        Self {
            reliability: MovingWindow::new(init_reliability, RELIABILITY_MOVING_WINDOW_SIZE),
            average_latencies: HashMap::new(),
            consecutive_failures: 0,
            exclusion_time: None,
        }
    }

    pub fn update_average_latency(&mut self, operation: OperationType, new_latency: Duration) {
        match self.average_latencies.entry(operation) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_value(new_latency.as_secs_f64());
            }
            Entry::Vacant(entry) => {
                entry.insert(MovingWindow::new(
                    new_latency.as_secs_f64(),
                    LATENCY_MOVING_WINDOW_SIZE,
                ));
            }
        }
    }
}

impl ClientObservedStats {
    pub fn new(config: ValidatorClientMonitorConfig) -> Self {
        Self {
            validator_stats: HashMap::new(),
            config,
        }
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Updates reliability scores, latency measurements, and failure counts
    /// based on client observations. Automatically excludes validators that
    /// exceed the maximum consecutive failure threshold.
    pub fn record_interaction_result(
        &mut self,
        feedback: OperationFeedback,
        metrics: &ValidatorClientMetrics,
    ) {
        let validator_stats = self
            .validator_stats
            .entry(feedback.authority_name)
            .or_insert_with(|| ValidatorClientStats::new(1.0));

        match feedback.result {
            Ok(latency) => {
                validator_stats.reliability.add_value(1.0);
                validator_stats.consecutive_failures = 0;
                validator_stats.update_average_latency(feedback.operation, latency);
            }
            Err(()) => {
                validator_stats.reliability.add_value(0.0);
                validator_stats.consecutive_failures += 1;

                // Exclude validator temporarily after too many consecutive failures
                if validator_stats.consecutive_failures >= self.config.max_consecutive_failures {
                    validator_stats.exclusion_time = Some(Instant::now());
                }
            }
        }

        metrics
            .consecutive_failures
            .with_label_values(&[&feedback.display_name])
            .set(validator_stats.consecutive_failures as i64);
    }

    /// Get validator scores for all validators in the committee for the provided tx type.
    ///
    /// Returns a map of all tracked validators to their scores.
    /// Score is 0 if the validator is excluded or has no stats.
    pub fn get_all_validator_stats(
        &self,
        committee: &Committee,
        tx_type: TxType,
    ) -> HashMap<AuthorityName, f64> {
        let max_latencies = self.calculate_max_latencies(committee);

        committee
            .names()
            .map(|validator| {
                let score = if let Some(stats) = self.validator_stats.get(validator) {
                    let is_excluded = if let Some(exclusion_time) = stats.exclusion_time {
                        exclusion_time.elapsed() < self.config.failure_cooldown
                    } else {
                        false
                    };
                    if is_excluded {
                        0.0
                    } else {
                        self.calculate_client_score(stats, &max_latencies, tx_type)
                    }
                } else {
                    0.0
                };
                (*validator, score)
            })
            .collect()
    }

    /// Calculate the max latencies for each operation type for the provided committee.
    /// The max latencies are calculated by taking the maximum latency for each operation type
    /// for each validator in the committee.
    fn calculate_max_latencies(&self, committee: &Committee) -> HashMap<OperationType, f64> {
        let mut max_latencies = HashMap::new();

        for validator in committee.names() {
            let stats = self.validator_stats.get(validator).unwrap();
            // We are specifically excluding from the max latencies calculations the validators that are meant to be excluded
            // from the score calculations anyways. Only the ones participating in the pool should be considered to avoid score inflation.
            let is_excluded = if let Some(exclusion_time) = stats.exclusion_time {
                exclusion_time.elapsed() < self.config.failure_cooldown
            } else {
                false
            };

            for op in OperationType::iter() {
                let latency = if is_excluded {
                    0.0
                } else {
                    stats
                        .average_latencies
                        .get(&op)
                        .map(|mw| mw.get())
                        .unwrap_or(0.0)
                };
                if let Some(max_latency) = max_latencies.get(&op) {
                    if latency > *max_latency {
                        max_latencies.insert(op, latency);
                    }
                } else {
                    max_latencies.insert(op, latency);
                }
            }
        }

        max_latencies
    }

    /// Calculate client-observed score for a single validator for the provided tx type.
    ///
    /// The score combines reliability and latency metrics as observed by the client,
    /// weighted according to configuration.
    ///
    /// If a validator is missing local stats for an operation type, we use a
    /// conservative default (assuming they are at the global maximum latency)
    /// to ensure fairness while still allowing them to be scored.
    ///
    /// Score calculation:
    /// 1. Latency scores are normalized against global maximums
    /// 2. Each operation type has its own weight
    /// 3. Final score = (weighted_latency_score * latency_weight) + (reliability * reliability_weight)
    fn calculate_client_score(
        &self,
        stats: &ValidatorClientStats,
        max_latencies: &HashMap<OperationType, f64>,
        tx_type: TxType,
    ) -> f64 {
        let mut latency_score = 0.0;
        let mut total_weight = 0.0;

        for op in OperationType::iter() {
            let latency_weight = match op {
                OperationType::Submit => self.config.score_weights.submit_latency_weight,
                OperationType::Effects => self.config.score_weights.effects_latency_weight,
                OperationType::HealthCheck => self.config.score_weights.health_check_latency_weight,
                OperationType::FastPath => self.config.score_weights.fast_path_latency_weight,
                OperationType::Consensus => self.config.score_weights.consensus_latency_weight,
            };

            if tx_type == TxType::SingleWriter && op == OperationType::Consensus {
                continue;
            }

            if tx_type == TxType::SharedObject && op == OperationType::FastPath {
                continue;
            }

            // Skip if max latency is missing for this operation
            let Some(max_latency) = max_latencies.get(&op) else {
                continue;
            };

            // If validator has local stats, use them; otherwise assume max latency (conservative)
            let latency = stats
                .average_latencies
                .get(&op)
                .map(|ma| ma.get())
                .unwrap_or(*max_latency);

            // Lower latency ratios are better (inverted for scoring)
            let latency_ratio = (latency / max_latency).min(1.0);
            latency_score += (1.0 - latency_ratio) * latency_weight;
            total_weight += latency_weight;
        }

        let latency_score = if total_weight == 0.0 {
            0.0
        } else {
            // Normalize latency score by total weight
            latency_score / total_weight
        };

        let reliability_score = stats.reliability.get();
        latency_score * self.config.score_weights.latency
            + reliability_score * self.config.score_weights.reliability
    }

    /// Retain only the specified validators, removing any others.
    ///
    /// Called periodically during health checks to clean up statistics for validators
    /// that are no longer in the active set. This prevents memory leaks and
    /// ensures scores are only calculated for current validators.
    pub fn retain_validators(&mut self, current_validators: &[AuthorityName]) {
        let cur_len = self.validator_stats.len();
        let validator_set: HashSet<_> = current_validators.iter().collect();
        self.validator_stats
            .retain(|validator, _| validator_set.contains(validator));
        let removed_count = cur_len - self.validator_stats.len();
        if removed_count > 0 {
            debug!("Removed {} stale validator data", removed_count);
        }
    }
}

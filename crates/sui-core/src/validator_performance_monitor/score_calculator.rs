// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_performance_monitor::config::ValidatorPerformanceConfig;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use sui_types::base_types::AuthorityName;

/// Performance score for a validator
#[derive(Debug, Clone, Default)]
pub struct PerformanceScore {
    /// Overall score (0.0 to 1.0, higher is better)
    pub overall_score: f64,
    /// Individual component scores
    pub components: ScoreComponents,
    /// Last update time
    pub last_updated: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreComponents {
    pub latency_score: f64,
    pub success_rate_score: f64,
}

/// Statistics for a validator
#[derive(Debug, Clone, Default)]
pub struct ValidatorStats {
    /// Success count
    pub success_count: u64,
    /// Failure count
    pub failure_count: u64,
    /// Average latency for submit operations
    pub avg_submit_latency: Duration,
    /// Average latency for effects operations
    pub avg_effects_latency: Duration,
    /// Average latency for health check operations
    pub avg_health_check_latency: Duration,
    /// Consecutive failures
    pub consecutive_failures: u32,
    /// Last success time
    pub last_success: Option<Instant>,
    /// Last failure time
    pub last_failure: Option<Instant>,
}

pub struct ScoreCalculator {
    config: ValidatorPerformanceConfig,
    /// Global statistics for normalization
    global_stats: GlobalStats,
}

#[derive(Debug, Clone, Default)]
struct GlobalStats {
    /// Maximum observed consensus round
    max_consensus_round: u64,
    /// Maximum observed checkpoint sequence
    _max_checkpoint_sequence: u64,
    /// Average latency across all validators
    _avg_global_latency: Duration,
    /// Maximum submit latency across all validators
    max_submit_latency: Duration,
    /// Maximum effects latency across all validators
    max_effects_latency: Duration,
    /// Maximum health check latency across all validators
    max_health_check_latency: Duration,
}

impl ScoreCalculator {
    pub fn new(config: ValidatorPerformanceConfig) -> Self {
        Self {
            config,
            global_stats: GlobalStats::default(),
        }
    }

    /// Update global statistics based on all validator stats
    pub fn update_global_stats(&mut self, all_stats: &HashMap<AuthorityName, ValidatorStats>) {
        let mut total_latency = Duration::ZERO;
        let mut latency_count = 0;
        let mut max_submit_latency = Duration::ZERO;
        let mut max_effects_latency = Duration::ZERO;
        let mut max_health_check_latency = Duration::ZERO;

        for stats in all_stats.values() {
            if stats.success_count > 0 {
                // Use submit latency as the primary latency for backward compatibility
                total_latency += stats.avg_submit_latency * stats.success_count as u32;
                latency_count += stats.success_count;

                max_submit_latency = max_submit_latency.max(stats.avg_submit_latency);
                max_effects_latency = max_effects_latency.max(stats.avg_effects_latency);
                max_health_check_latency =
                    max_health_check_latency.max(stats.avg_health_check_latency);
            }
        }

        // Helper to ensure minimum latency values
        let ensure_min_latency = |latency: Duration| {
            if latency.is_zero() {
                Duration::from_secs(1)
            } else {
                latency
            }
        };

        self.global_stats = GlobalStats {
            max_consensus_round: 0,
            _max_checkpoint_sequence: 0,
            _avg_global_latency: if latency_count > 0 {
                total_latency / latency_count as u32
            } else {
                Duration::from_millis(100)
            },
            max_submit_latency: ensure_min_latency(max_submit_latency),
            max_effects_latency: ensure_min_latency(max_effects_latency),
            max_health_check_latency: ensure_min_latency(max_health_check_latency),
        };
    }

    /// Calculate performance score for a validator
    pub fn calculate_score(&self, stats: &ValidatorStats) -> PerformanceScore {
        let mut components = ScoreComponents::default();

        // Skip if not enough samples
        let total_ops = stats.success_count + stats.failure_count;
        if total_ops < self.config.min_samples as u64 {
            return PerformanceScore::default();
        }

        // Calculate success rate score
        components.success_rate_score = stats.success_count as f64 / total_ops as f64;

        // Calculate composite latency score from all operation types
        let calc_latency_ratio = |avg_latency: Duration, max_latency: Duration| {
            if max_latency.as_secs_f64() > 0.0 {
                avg_latency.as_secs_f64() / max_latency.as_secs_f64()
            } else {
                0.0
            }
        };

        let submit_ratio = calc_latency_ratio(
            stats.avg_submit_latency,
            self.global_stats.max_submit_latency,
        );
        let effects_ratio = calc_latency_ratio(
            stats.avg_effects_latency,
            self.global_stats.max_effects_latency,
        );
        let health_ratio = calc_latency_ratio(
            stats.avg_health_check_latency,
            self.global_stats.max_health_check_latency,
        );

        // Weighted average of latency ratios (submit gets highest weight as it's most critical)
        let composite_latency_ratio =
            (submit_ratio * 0.5 + effects_ratio * 0.3 + health_ratio * 0.2).min(1.0);
        components.latency_score = 1.0 - composite_latency_ratio;

        // Apply consecutive failure penalty
        let failure_penalty = if stats.consecutive_failures > 0 {
            0.9_f64.powi(stats.consecutive_failures as i32)
        } else {
            1.0
        };

        // Calculate weighted overall score using only latency and success rate
        let weights = &self.config.score_weights;
        let weighted_sum = components.latency_score * weights.latency
            + components.success_rate_score * weights.success_rate;

        let weight_total = weights.latency + weights.success_rate;

        let overall_score = (weighted_sum / weight_total) * failure_penalty;

        PerformanceScore {
            overall_score: overall_score.clamp(0.0, 1.0),
            components,
            last_updated: Some(Instant::now()),
        }
    }

    /// Apply adaptive scoring adjustments based on recent performance
    pub fn apply_adaptive_adjustments(&self, score: &mut PerformanceScore, stats: &ValidatorStats) {
        if !self.config.adaptive_scoring {
            return;
        }

        // Boost score for recently recovered validators
        if let (Some(last_success), Some(last_failure)) = (stats.last_success, stats.last_failure) {
            if last_success > last_failure && stats.consecutive_failures == 0 {
                let recovery_time = last_success.duration_since(last_failure);
                if recovery_time < Duration::from_secs(60) {
                    // Recent recovery, give a small boost
                    score.overall_score = (score.overall_score * 1.1).min(1.0);
                }
            }
        }

        // Apply time-based decay for stale data
        if let Some(last_updated) = score.last_updated {
            let age = Instant::now().duration_since(last_updated);
            if age > self.config.metrics_window {
                // Decay score for stale data
                let decay_factor = 0.5
                    + 0.5
                        * (1.0
                            - age.as_secs_f64() / (2.0 * self.config.metrics_window.as_secs_f64()))
                        .max(0.0);
                score.overall_score *= decay_factor;
            }
        }
    }
}

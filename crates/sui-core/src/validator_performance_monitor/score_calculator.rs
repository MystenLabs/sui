// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_performance_monitor::{config::ValidatorPerformanceConfig, HealthMetrics};
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
    pub pending_certificates_score: f64,
    pub consensus_lag_score: f64,
    pub queue_size_score: f64,
    pub resource_usage_score: f64,
}

/// Statistics for a validator
#[derive(Debug, Clone, Default)]
pub struct ValidatorStats {
    /// Success count
    pub success_count: u64,
    /// Failure count
    pub failure_count: u64,
    /// Average latency
    pub avg_latency: Duration,
    /// Latest health metrics
    pub latest_health: Option<HealthMetrics>,
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
    /// Maximum latency across all validators
    max_latency: Duration,
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
        let mut max_latency = Duration::ZERO;
        let mut max_consensus = 0u64;
        let mut max_checkpoint = 0u64;

        for stats in all_stats.values() {
            if stats.success_count > 0 {
                total_latency += stats.avg_latency * stats.success_count as u32;
                latency_count += stats.success_count;
                max_latency = max_latency.max(stats.avg_latency);
            }

            if let Some(health) = &stats.latest_health {
                max_consensus = max_consensus.max(health.consensus_round);
                max_checkpoint = max_checkpoint.max(health.checkpoint_sequence);
            }
        }

        self.global_stats = GlobalStats {
            max_consensus_round: max_consensus,
            _max_checkpoint_sequence: max_checkpoint,
            _avg_global_latency: if latency_count > 0 {
                total_latency / latency_count as u32
            } else {
                Duration::from_millis(100)
            },
            max_latency: if max_latency.is_zero() {
                Duration::from_secs(1)
            } else {
                max_latency
            },
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

        // Calculate latency score (normalized, inverted so lower is better)
        let latency_ratio =
            stats.avg_latency.as_secs_f64() / self.global_stats.max_latency.as_secs_f64();
        components.latency_score = 1.0 - latency_ratio.min(1.0);

        // Calculate health-based scores if available
        if let Some(health) = &stats.latest_health {
            // Pending certificates score (normalized, inverted)
            let pending_ratio = (health.pending_certificates as f64 / 1000.0).min(1.0);
            components.pending_certificates_score = 1.0 - pending_ratio;

            // Consensus lag score
            if self.global_stats.max_consensus_round > 0 {
                let lag = self
                    .global_stats
                    .max_consensus_round
                    .saturating_sub(health.consensus_round);
                let lag_ratio = (lag as f64 / 100.0).min(1.0); // Consider 100 rounds as max lag
                components.consensus_lag_score = 1.0 - lag_ratio;
            } else {
                components.consensus_lag_score = 1.0;
            }

            // Queue size score
            let queue_ratio = (health.tx_queue_size as f64 / 10000.0).min(1.0);
            components.queue_size_score = 1.0 - queue_ratio;

            // Resource usage score
            let cpu_score = if let Some(cpu) = health.cpu_usage {
                1.0 - (cpu as f64 / 100.0).min(1.0)
            } else {
                0.5 // Neutral if not available
            };

            let memory_score = if let Some(mem) = health.available_memory {
                // Assume 1GB as critical low memory
                (mem as f64 / 1_000_000_000.0).min(1.0)
            } else {
                0.5 // Neutral if not available
            };

            components.resource_usage_score = (cpu_score + memory_score) / 2.0;
        } else {
            // Use neutral scores if health data not available
            components.pending_certificates_score = 0.5;
            components.consensus_lag_score = 0.5;
            components.queue_size_score = 0.5;
            components.resource_usage_score = 0.5;
        }

        // Apply consecutive failure penalty
        let failure_penalty = if stats.consecutive_failures > 0 {
            0.9_f64.powi(stats.consecutive_failures as i32)
        } else {
            1.0
        };

        // Calculate weighted overall score
        let weights = &self.config.score_weights;
        let weighted_sum = components.latency_score * weights.latency
            + components.success_rate_score * weights.success_rate
            + components.pending_certificates_score * weights.pending_certificates
            + components.consensus_lag_score * weights.consensus_lag
            + components.queue_size_score * weights.queue_size
            + components.resource_usage_score * weights.resource_usage;

        let weight_total = weights.latency
            + weights.success_rate
            + weights.pending_certificates
            + weights.consensus_lag
            + weights.queue_size
            + weights.resource_usage;

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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the validator performance monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorPerformanceConfig {
    /// How often to perform health checks on validators
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: Duration,

    /// Timeout for health check requests
    #[serde(default = "default_health_check_timeout")]
    pub health_check_timeout: Duration,

    /// Weight configuration for score calculation
    #[serde(default)]
    pub score_weights: ScoreWeights,

    /// Selection strategy configuration
    #[serde(default)]
    pub selection_strategy: SelectionStrategy,

    /// Window size for rolling metrics
    #[serde(default = "default_metrics_window")]
    pub metrics_window: Duration,

    /// Minimum number of samples before considering a validator
    #[serde(default = "default_min_samples")]
    pub min_samples: usize,

    /// Whether to enable adaptive scoring
    #[serde(default = "default_adaptive_scoring")]
    pub adaptive_scoring: bool,

    /// Cooldown period after failures
    #[serde(default = "default_failure_cooldown")]
    pub failure_cooldown: Duration,

    /// Maximum number of consecutive failures before temporary exclusion
    #[serde(default = "default_max_consecutive_failures")]
    pub max_consecutive_failures: u32,
}

/// Weights for different factors in score calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    /// Weight for latency (lower is better)
    #[serde(default = "default_latency_weight")]
    pub latency: f64,

    /// Weight for success rate
    #[serde(default = "default_success_rate_weight")]
    pub success_rate: f64,
}

/// Strategy for selecting validators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionStrategy {
    /// Weighted random selection based on scores
    WeightedRandom {
        /// Temperature parameter for softmax (higher = more uniform)
        temperature: f64,
    },
    /// Top-K selection with round-robin
    TopK {
        /// Number of top validators to consider
        k: usize,
    },
    /// Epsilon-greedy selection
    EpsilonGreedy {
        /// Probability of random selection
        epsilon: f64,
    },
}

impl Default for ValidatorPerformanceConfig {
    fn default() -> Self {
        Self {
            health_check_interval: default_health_check_interval(),
            health_check_timeout: default_health_check_timeout(),
            score_weights: ScoreWeights::default(),
            selection_strategy: SelectionStrategy::default(),
            metrics_window: default_metrics_window(),
            min_samples: default_min_samples(),
            adaptive_scoring: default_adaptive_scoring(),
            failure_cooldown: default_failure_cooldown(),
            max_consecutive_failures: default_max_consecutive_failures(),
        }
    }
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            latency: default_latency_weight(),
            success_rate: default_success_rate_weight(),
        }
    }
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        SelectionStrategy::WeightedRandom { temperature: 1.0 }
    }
}

fn default_health_check_interval() -> Duration {
    Duration::from_secs(10)
}

fn default_health_check_timeout() -> Duration {
    Duration::from_secs(2)
}

fn default_metrics_window() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

fn default_min_samples() -> usize {
    3
}

fn default_adaptive_scoring() -> bool {
    true
}

fn default_failure_cooldown() -> Duration {
    Duration::from_secs(30)
}

fn default_max_consecutive_failures() -> u32 {
    5
}

fn default_latency_weight() -> f64 {
    0.4
}

fn default_success_rate_weight() -> f64 {
    0.6
}

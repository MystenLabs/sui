// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the validator client monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorClientMonitorConfig {
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
    #[serde(default = "default_reliability_weight")]
    pub reliability: f64,

    /// Weight for submit latency
    #[serde(default = "default_submit_latency_weight")]
    pub submit_latency_weight: f64,

    /// Weight for effects latency
    #[serde(default = "default_effects_latency_weight")]
    pub effects_latency_weight: f64,

    /// Weight for health check latency
    #[serde(default = "default_health_check_latency_weight")]
    pub health_check_latency_weight: f64,
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
}

impl Default for ValidatorClientMonitorConfig {
    fn default() -> Self {
        Self {
            health_check_interval: default_health_check_interval(),
            health_check_timeout: default_health_check_timeout(),
            score_weights: ScoreWeights::default(),
            selection_strategy: SelectionStrategy::default(),
            failure_cooldown: default_failure_cooldown(),
            max_consecutive_failures: default_max_consecutive_failures(),
        }
    }
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            latency: default_latency_weight(),
            reliability: default_reliability_weight(),
            submit_latency_weight: default_submit_latency_weight(),
            effects_latency_weight: default_effects_latency_weight(),
            health_check_latency_weight: default_health_check_latency_weight(),
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

fn default_failure_cooldown() -> Duration {
    Duration::from_secs(30)
}

fn default_max_consecutive_failures() -> u32 {
    5
}

fn default_latency_weight() -> f64 {
    0.4
}

fn default_reliability_weight() -> f64 {
    0.6
}

fn default_submit_latency_weight() -> f64 {
    0.3
}

fn default_effects_latency_weight() -> f64 {
    0.5
}

fn default_health_check_latency_weight() -> f64 {
    0.2
}

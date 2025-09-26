// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Configuration for the Validator Client Monitor
//!
//! The Validator Client Monitor tracks client-observed performance metrics for validators
//! in the Sui network. It runs from the perspective of a fullnode and monitors:
//! - Transaction submission latency
//! - Effects retrieval latency
//! - Health check response times
//! - Success/failure rates
//!
//! # Tuning Guide
//!
//! ## Monitoring Metrics
//!
//! The following Prometheus metrics can help tune the configuration:
//!
//! - `validator_client_observed_latency` - Histogram of operation latencies per validator
//! - `validator_client_operation_success_total` - Counter of successful operations
//! - `validator_client_operation_failure_total` - Counter of failed operations
//! - `validator_client_observed_score` - Current score for each validator (0-1)
//! - `validator_client_consecutive_failures` - Current consecutive failure count
//! - `validator_client_selections_total` - How often each validator is selected
//!
//! ## Configuration Parameters
//!
//! ### Health Check Settings
//!
//! - `health_check_interval`: How often to probe validator health
//!   - Default: 10s
//!   - Decrease for more responsive failure detection (higher overhead)
//!   - Increase to reduce network traffic
//!   - Monitor `validator_client_operation_success_total{operation="health_check"}` to see probe frequency
//!
//! - `health_check_timeout`: Maximum time to wait for health check response
//!   - Default: 2s
//!   - Should be less than `health_check_interval`
//!   - Set based on p99 of `validator_client_observed_latency{operation="health_check"}`
//!
//! ### Failure Handling
//!
//! - `max_consecutive_failures`: Failures before temporary exclusion
//!   - Default: 5
//!   - Lower values = faster exclusion of problematic validators
//!   - Higher values = more tolerance for transient issues
//!   - Monitor `validator_client_consecutive_failures` to see failure patterns
//!
//! - `failure_cooldown`: How long to exclude failed validators
//!   - Default: 30s
//!   - Should be several times the `health_check_interval`
//!   - Too short = thrashing between exclusion/inclusion
//!   - Too long = reduced validator pool during transient issues
//!
//! ### Score Weights
//!
//! Scores combine reliability and latency metrics. Adjust weights based on priorities:
//!
//! - `reliability`: Weight for success rate (0-1)
//!   - Default: 0.6
//!   - Increase if consistency is critical
//!   - Decrease if latency is more important than occasional failures
//!
//! - `latency`: Weight for latency scores
//!   - Default: 0.4
//!   - Increase for latency-sensitive applications
//!   - Individual operation weights can be tuned separately
//!
//! # Example Configurations
//!
//! ## Low Latency Priority
//! ```yaml
//! validator-client-monitor-config:
//!   health-check-interval: 5s
//!   health-check-timeout: 1s
//!   max-consecutive-failures: 3
//!   failure-cooldown: 20s
//!   score-weights:
//!     latency: 0.7
//!     reliability: 0.3
//!     effects-latency-weight: 0.6  # Effects queries are critical
//! ```
//!
//! ## High Reliability Priority
//! ```yaml
//! validator-client-monitor-config:
//!   health-check-interval: 15s
//!   max-consecutive-failures: 10  # Very tolerant
//!   failure-cooldown: 60s
//!   score-weights:
//!     latency: 0.2
//!     reliability: 0.8
//! ```

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for validator client monitoring from the client perspective
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorClientMonitorConfig {
    /// How often to perform health checks on validators.
    ///
    /// Lower values provide faster failure detection but increase network overhead.
    /// This should be balanced against the `failure_cooldown` period.
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: Duration,

    /// Timeout for health check requests.
    ///
    /// Should be less than `health_check_interval` to avoid overlapping checks.
    /// Set based on network latency characteristics - typically 2-3x p99 latency.
    #[serde(default = "default_health_check_timeout")]
    pub health_check_timeout: Duration,

    /// Weight configuration for score calculation.
    ///
    /// Determines how different factors contribute to validator selection.
    #[serde(default)]
    pub score_weights: ScoreWeights,

    /// Cooldown period after failures before considering a validator again.
    ///
    /// Should be long enough to allow transient issues to resolve,
    /// but short enough to quickly recover capacity when issues are fixed.
    #[serde(default = "default_failure_cooldown")]
    pub failure_cooldown: Duration,

    /// Maximum number of consecutive failures before temporary exclusion.
    ///
    /// Lower values are more aggressive about excluding problematic validators.
    /// Higher values are more tolerant of intermittent issues.
    #[serde(default = "default_max_consecutive_failures")]
    pub max_consecutive_failures: u32,
}

/// Weights for different factors in score calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ScoreWeights {
    /// Weight for latency (lower is better).
    ///
    /// This is the overall weight for all latency scores combined.
    /// Individual operation latencies are weighted separately below.
    #[serde(default = "default_latency_weight")]
    pub latency: f64,

    /// Weight for success rate.
    ///
    /// Higher values prioritize reliability over performance.
    #[serde(default = "default_reliability_weight")]
    pub reliability: f64,

    /// Weight for submit transaction latency.
    ///
    /// Controls importance of transaction submission speed.
    #[serde(default = "default_submit_latency_weight")]
    pub submit_latency_weight: f64,

    /// Weight for effects retrieval latency.
    ///
    /// Controls importance of effects query speed.
    /// Often the most critical operation for application responsiveness.
    #[serde(default = "default_effects_latency_weight")]
    pub effects_latency_weight: f64,

    /// Weight for health check latency.
    ///
    /// Usually less critical than actual operations.
    #[serde(default = "default_health_check_latency_weight")]
    pub health_check_latency_weight: f64,

    /// Weight for fast path latency.
    ///
    /// Controls importance of finalization speed.
    #[serde(default = "default_fast_path_latency_weight")]
    pub fast_path_latency_weight: f64,

    /// Weight for consensus latency.
    ///
    /// Controls importance of consensus speed.
    #[serde(default = "default_consensus_latency_weight")]
    pub consensus_latency_weight: f64,
}

impl Default for ValidatorClientMonitorConfig {
    fn default() -> Self {
        Self {
            health_check_interval: default_health_check_interval(),
            health_check_timeout: default_health_check_timeout(),
            score_weights: ScoreWeights::default(),
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
            fast_path_latency_weight: default_fast_path_latency_weight(),
            consensus_latency_weight: default_consensus_latency_weight(),
        }
    }
}

// Default value functions

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
    100
}

fn default_latency_weight() -> f64 {
    0.9
}

fn default_reliability_weight() -> f64 {
    0.1
}

fn default_submit_latency_weight() -> f64 {
    0.0
}

fn default_effects_latency_weight() -> f64 {
    0.0
}

fn default_health_check_latency_weight() -> f64 {
    0.0
}

fn default_fast_path_latency_weight() -> f64 {
    1.0
}

fn default_consensus_latency_weight() -> f64 {
    1.0
}

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

    /// Weight for reliability.
    ///
    /// Controls importance of reliability when adjusting the validator's latency for transaction submission
    /// selection. The higher the weight, the more penalty is given to unreliable validators.
    /// Default to 2.0. Value should be positive.
    #[serde(default = "default_reliability_weight")]
    pub reliability_weight: f64,

    /// Size of the moving window for latency measurements
    #[serde(default = "default_latency_moving_window_size")]
    pub latency_moving_window_size: usize,

    /// Size of the moving window for reliability measurements
    #[serde(default = "default_reliability_moving_window_size")]
    pub reliability_moving_window_size: usize,
}

impl Default for ValidatorClientMonitorConfig {
    fn default() -> Self {
        Self {
            health_check_interval: default_health_check_interval(),
            health_check_timeout: default_health_check_timeout(),
            reliability_weight: default_reliability_weight(),
            latency_moving_window_size: default_latency_moving_window_size(),
            reliability_moving_window_size: default_reliability_moving_window_size(),
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

fn default_reliability_weight() -> f64 {
    2.0
}

fn default_latency_moving_window_size() -> usize {
    40
}

fn default_reliability_moving_window_size() -> usize {
    20
}

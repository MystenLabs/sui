// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod config;
mod metrics;
mod performance_tracker;
mod score_calculator;

#[cfg(test)]
mod tests;

pub use config::ValidatorPerformanceConfig;
pub use metrics::ValidatorPerformanceMetrics;
pub use performance_tracker::{
    SelectionReason, ValidatorData, ValidatorPerformanceMonitor, ValidatorPerformanceRecord,
    ValidatorSelectionOutput,
};
pub use score_calculator::{PerformanceScore, ScoreCalculator};

use std::time::Duration;
use sui_types::base_types::AuthorityName;

/// Feedback from TransactionDriver operations
#[derive(Debug, Clone)]
pub enum OperationFeedback {
    /// Transaction submission succeeded
    SubmitSuccess {
        validator: AuthorityName,
        latency: Duration,
    },
    /// Transaction submission failed
    SubmitFailure {
        validator: AuthorityName,
        latency: Duration,
        error: String,
    },
    /// Effects retrieval succeeded
    EffectsSuccess {
        validator: AuthorityName,
        latency: Duration,
    },
    /// Effects retrieval failed
    EffectsFailure {
        validator: AuthorityName,
        latency: Duration,
        error: String,
    },
    /// Health check succeeded (we only care about latency, not the response data)
    HealthCheckSuccess {
        validator: AuthorityName,
        latency: Duration,
    },
    /// Health check failed
    HealthCheckFailure {
        validator: AuthorityName,
        latency: Duration,
        error: String,
    },
}

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
    /// Health check result
    HealthCheckResult {
        validator: AuthorityName,
        latency: Duration,
        metrics: HealthMetrics,
    },
    /// Health check failed
    HealthCheckFailure {
        validator: AuthorityName,
        latency: Duration,
        error: String,
    },
}

/// Health metrics reported by validators
#[derive(Debug, Clone, Default)]
pub struct HealthMetrics {
    /// Number of pending certificates
    pub pending_certificates: u64,
    /// Number of in-flight consensus messages
    pub inflight_consensus_messages: u64,
    /// Current consensus round
    pub consensus_round: u64,
    /// Current checkpoint sequence number
    pub checkpoint_sequence: u64,
    /// Transaction execution queue size
    pub tx_queue_size: u64,
    /// Available system memory in bytes
    pub available_memory: Option<u64>,
    /// CPU usage percentage (0-100)
    pub cpu_usage: Option<f32>,
}

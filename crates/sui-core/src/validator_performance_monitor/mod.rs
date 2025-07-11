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

/// Operation types for validator performance tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Submit,
    Effects,
    HealthCheck,
}

/// Feedback from TransactionDriver operations
#[derive(Debug, Clone)]
pub struct OperationFeedback {
    pub validator: AuthorityName,
    pub operation: OperationType,
    pub latency: Duration,
    pub success: bool,
    pub error: Option<String>,
}

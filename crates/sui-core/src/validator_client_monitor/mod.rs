// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod client_stats;
mod client_tracker;
mod config;
mod metrics;

#[cfg(test)]
mod tests;

pub use client_tracker::ValidatorClientMonitor;
pub use config::ValidatorClientMonitorConfig;
pub use metrics::ValidatorClientMetrics;
use strum::EnumIter;

use std::time::Duration;
use sui_types::base_types::AuthorityName;

/// Operation types for validator performance tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
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
    pub latency: Option<Duration>,
    pub success: bool,
}

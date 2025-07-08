// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
mod monitor;
mod stats;

#[cfg(test)]
mod tests;

pub use metrics::ValidatorClientMetrics;
pub use monitor::ValidatorClientMonitor;
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

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
use sui_types::base_types::AuthorityName;

use std::time::Duration;

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
    /// The unique authority name (public key)
    pub authority_name: AuthorityName,
    /// The human-readable display name for the validator
    pub display_name: String,
    /// The operation type
    pub operation: OperationType,
    /// Result of the operation: Ok(latency) if successful, Err(()) if failed
    pub result: Result<Duration, ()>,
}

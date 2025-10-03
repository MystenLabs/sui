// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod metrics;
mod monitor;
mod stats;

#[cfg(test)]
mod tests;

pub use metrics::ValidatorClientMetrics;
pub use monitor::ValidatorClientMonitor;
use std::time::Duration;
use strum::EnumIter;
use sui_types::base_types::AuthorityName;

/// Operation types for validator performance tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter)]
pub enum OperationType {
    Submit,
    Effects,
    FastPath,
    HealthCheck,
    Consensus,
}

impl OperationType {
    pub fn as_str(&self) -> &str {
        match self {
            OperationType::Submit => "submit",
            OperationType::Effects => "effects",
            OperationType::HealthCheck => "health_check",
            OperationType::FastPath => "fast_path",
            OperationType::Consensus => "consensus",
        }
    }
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
    /// Result of the operation: Ok(latency) if successful, Err(()) if failed.
    /// Only errors specific to the target validator should be recorded,
    /// for example, timeout, unavailability or misbehavior from validators can be recorded.
    /// But other errors unrelated to a specific validator, for example invalid user transaction,
    /// should not be recorded.
    pub result: Result<Duration, ()>,
}

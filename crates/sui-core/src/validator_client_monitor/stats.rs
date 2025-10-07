// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::validator_client_monitor::{OperationFeedback, OperationType};
use mysten_common::moving_window::MovingWindow;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::base_types::AuthorityName;
use sui_types::committee::Committee;
use sui_types::messages_grpc::TxType;
use tracing::debug;

// TODO: A few optimization to consider:
// 1. There may be times when the entire network is unstable, and in that case
//    we may not want to punish validators when they have errors.
// 2. Some reports are more critical than others. For example, a health check
//    report is more critical than a submit report in terms of failures status.

/// Maximum adjusted latency from completely unreachable (reliability = 0.0) or very slow validators.
const MAX_LATENCY: Duration = Duration::from_secs(10);

/// Complete client-observed statistics for validator interactions.
///
/// This struct maintains client-side metrics for all validators in the network,
/// including latency measurements, reliability measurements, and failure tracking
/// as observed from the client's perspective. It uses exponential moving averages (EMA)
/// to smooth out transient spikes while still responding to sustained changes.
#[derive(Debug, Clone)]
pub struct ClientObservedStats {
    /// Per-validator statistics mapping authority names to their client-observed metrics
    pub validator_stats: HashMap<AuthorityName, ValidatorClientStats>,
    /// Configuration parameters for scoring and exclusion policies
    pub config: ValidatorClientMonitorConfig,
}

/// Client-observed stats for a single validator.
///
/// Tracks reliability, latency, and failure patterns for a specific validator
/// as observed from the client's perspective. Uses exponential moving averages
/// to smooth measurements while maintaining responsiveness to changes.
#[derive(Debug, Clone)]
pub struct ValidatorClientStats {
    /// Moving window of success rate (0.0 to 1.0)
    pub reliability: MovingWindow<f64>,
    /// Moving window of latencies for each operation type (Submit, Effects, HealthCheck)
    pub average_latencies: BTreeMap<OperationType, MovingWindow<Duration>>,
    /// Size of the moving window for latency measurements
    pub latency_moving_window_size: usize,
}

impl ValidatorClientStats {
    pub fn new(
        init_reliability: f64,
        reliability_moving_window_size: usize,
        latency_moving_window_size: usize,
    ) -> Self {
        Self {
            reliability: MovingWindow::new(init_reliability, reliability_moving_window_size),
            average_latencies: BTreeMap::new(),
            latency_moving_window_size,
        }
    }

    pub fn update_average_latency(&mut self, operation: OperationType, new_latency: Duration) {
        match self.average_latencies.entry(operation) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_value(new_latency);
            }
            Entry::Vacant(entry) => {
                entry.insert(MovingWindow::new(
                    new_latency,
                    self.latency_moving_window_size,
                ));
            }
        }
    }
}

impl ClientObservedStats {
    pub fn new(config: ValidatorClientMonitorConfig) -> Self {
        Self {
            validator_stats: HashMap::new(),
            config,
        }
    }

    /// Record client-observed interaction result with a validator.
    ///
    /// Updates reliability scores and latency measurements.
    pub fn record_interaction_result(&mut self, feedback: OperationFeedback) {
        let validator_stats = self
            .validator_stats
            .entry(feedback.authority_name)
            .or_insert_with(|| {
                ValidatorClientStats::new(
                    1.0,
                    self.config.reliability_moving_window_size,
                    self.config.latency_moving_window_size,
                )
            });

        match feedback.result {
            Ok(latency) => {
                validator_stats.reliability.add_value(1.0);
                validator_stats.update_average_latency(feedback.operation, latency);
            }
            Err(()) => {
                validator_stats.reliability.add_value(0.0);
            }
        }
    }

    /// Get validator latencies for all validators in the committee for the provided tx type.
    ///
    /// Returns a map of all tracked validators to their stats. For a validator
    pub fn get_all_validator_stats(
        &self,
        committee: &Committee,
        tx_type: TxType,
    ) -> HashMap<AuthorityName, Duration> {
        committee
            .names()
            .map(|validator| {
                let latency = self.calculate_client_latency(validator, tx_type);
                (*validator, latency)
            })
            .collect()
    }

    /// Calculate adjusted latency for a single validator for the provided tx type.
    ///
    /// Returns the average latency for relevant operations (Consensus and FastPath only)
    /// with reliability penalty applied. Lower values are better.
    ///
    /// Only considers:
    /// - Consensus operations (for SharedObject transactions)
    /// - FastPath operations (for SingleWriter transactions)
    ///
    /// Returns latency in seconds, with reliability penalty applied as a multiplier.
    fn calculate_client_latency(&self, validator: &AuthorityName, tx_type: TxType) -> Duration {
        let Some(stats) = self.validator_stats.get(validator) else {
            return MAX_LATENCY;
        };

        let operation = match tx_type {
            TxType::SharedObject => OperationType::Consensus,
            TxType::SingleWriter => OperationType::FastPath,
        };
        let Some(latency) = stats.average_latencies.get(&operation) else {
            // For the target validator and operation type, no latency data has been recorded yet.
            return MAX_LATENCY;
        };

        // Get the latency for the relevant operation
        let base_latency = latency.get();
        let reliability = stats.reliability.get();
        let reliability_weight = self.config.reliability_weight;
        let penalty = MAX_LATENCY.mul_f64((1.0 - reliability) * reliability_weight);
        (base_latency + penalty).min(MAX_LATENCY)
    }

    /// Retain only the specified validators, removing any others.
    ///
    /// Called periodically during health checks to clean up statistics for validators
    /// that are no longer in the active set. This prevents memory leaks and
    /// ensures scores are only calculated for current validators.
    pub fn retain_validators(&mut self, current_validators: &[AuthorityName]) {
        let cur_len = self.validator_stats.len();
        let validator_set: HashSet<_> = current_validators.iter().collect();
        self.validator_stats
            .retain(|validator, _| validator_set.contains(validator));
        let removed_count = cur_len - self.validator_stats.len();
        if removed_count > 0 {
            debug!("Removed {} stale validator data", removed_count);
        }
    }
}

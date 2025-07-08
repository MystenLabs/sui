// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, GaugeVec,
    HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

#[derive(Clone)]
pub struct ValidatorPerformanceMetrics {
    /// Latency of operations per validator
    pub operation_latency: HistogramVec,

    /// Success count per validator and operation type
    pub operation_success: IntCounterVec,

    /// Failure count per validator and operation type
    pub operation_failure: IntCounterVec,

    /// Current performance score per validator
    pub performance_score: GaugeVec,

    /// Health check latency per validator
    pub health_check_latency: HistogramVec,

    /// Pending certificates reported by validators
    pub pending_certificates: IntGaugeVec,

    /// Consensus round reported by validators
    pub consensus_round: IntGaugeVec,

    /// Checkpoint sequence reported by validators
    pub checkpoint_sequence: IntGaugeVec,

    /// Transaction queue size reported by validators
    pub tx_queue_size: IntGaugeVec,

    /// CPU usage reported by validators
    pub cpu_usage: GaugeVec,

    /// Available memory reported by validators
    pub available_memory: IntGaugeVec,

    /// Number of times each validator was selected
    pub validator_selections: IntCounterVec,

    /// Consecutive failures per validator
    pub consecutive_failures: IntGaugeVec,

    /// Time since last successful operation per validator
    pub time_since_last_success: GaugeVec,
}

impl ValidatorPerformanceMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            operation_latency: register_histogram_vec_with_registry!(
                "validator_operation_latency",
                "Latency of operations per validator",
                &["validator", "operation_type"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            operation_success: register_int_counter_vec_with_registry!(
                "validator_operation_success_total",
                "Total successful operations per validator",
                &["validator", "operation_type"],
                registry,
            )
            .unwrap(),

            operation_failure: register_int_counter_vec_with_registry!(
                "validator_operation_failure_total",
                "Total failed operations per validator",
                &["validator", "operation_type", "error_type"],
                registry,
            )
            .unwrap(),

            performance_score: register_gauge_vec_with_registry!(
                "validator_performance_score",
                "Current performance score per validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            health_check_latency: register_histogram_vec_with_registry!(
                "validator_health_check_latency",
                "Health check latency per validator",
                &["validator"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            pending_certificates: register_int_gauge_vec_with_registry!(
                "validator_pending_certificates",
                "Pending certificates reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            consensus_round: register_int_gauge_vec_with_registry!(
                "validator_consensus_round",
                "Current consensus round reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            checkpoint_sequence: register_int_gauge_vec_with_registry!(
                "validator_checkpoint_sequence",
                "Current checkpoint sequence reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            tx_queue_size: register_int_gauge_vec_with_registry!(
                "validator_tx_queue_size",
                "Transaction queue size reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            cpu_usage: register_gauge_vec_with_registry!(
                "validator_cpu_usage",
                "CPU usage percentage reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            available_memory: register_int_gauge_vec_with_registry!(
                "validator_available_memory",
                "Available memory in bytes reported by validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            validator_selections: register_int_counter_vec_with_registry!(
                "validator_selections_total",
                "Total number of times each validator was selected",
                &["validator"],
                registry,
            )
            .unwrap(),

            consecutive_failures: register_int_gauge_vec_with_registry!(
                "validator_consecutive_failures",
                "Current consecutive failures per validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            time_since_last_success: register_gauge_vec_with_registry!(
                "validator_time_since_last_success",
                "Time in seconds since last successful operation",
                &["validator"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

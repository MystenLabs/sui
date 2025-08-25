// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, GaugeVec,
    HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.04, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

#[derive(Clone)]
pub struct ValidatorClientMetrics {
    /// Latency of operations per validator
    pub observed_latency: HistogramVec,

    /// Success count per validator and operation type
    pub operation_success: IntCounterVec,

    /// Failure count per validator and operation type
    pub operation_failure: IntCounterVec,

    /// Current performance score per validator
    pub performance_score: GaugeVec,

    /// Consecutive failures per validator
    pub consecutive_failures: IntGaugeVec,

    /// Time since last successful operation per validator
    pub time_since_last_success: GaugeVec,
}

impl ValidatorClientMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            observed_latency: register_histogram_vec_with_registry!(
                "validator_client_observed_latency",
                "Client-observed latency of operations per validator",
                &["validator", "operation_type"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            operation_success: register_int_counter_vec_with_registry!(
                "validator_client_operation_success_total",
                "Total successful operations observed by client per validator",
                &["validator", "operation_type"],
                registry,
            )
            .unwrap(),

            operation_failure: register_int_counter_vec_with_registry!(
                "validator_client_operation_failure_total",
                "Total failed operations observed by client per validator",
                &["validator", "operation_type"],
                registry,
            )
            .unwrap(),

            performance_score: register_gauge_vec_with_registry!(
                "validator_client_observed_score",
                "Current client-observed score per validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            consecutive_failures: register_int_gauge_vec_with_registry!(
                "validator_client_consecutive_failures",
                "Current consecutive failures observed by client per validator",
                &["validator"],
                registry,
            )
            .unwrap(),

            time_since_last_success: register_gauge_vec_with_registry!(
                "validator_client_time_since_last_success",
                "Time in seconds since last successful client interaction",
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

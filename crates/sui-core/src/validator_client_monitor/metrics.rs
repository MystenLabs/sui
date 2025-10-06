// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::{COUNT_BUCKETS, LATENCY_SEC_BUCKETS};
use prometheus::{
    register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, GaugeVec, HistogramVec, IntCounterVec, Registry,
};

#[derive(Clone)]
pub struct ValidatorClientMetrics {
    /// Latency of operations per validator
    pub observed_latency: HistogramVec,

    /// Success count per validator and operation type
    pub operation_success: IntCounterVec,

    /// Failure count per validator and operation type
    pub operation_failure: IntCounterVec,

    /// Current performance per validator. The performance is the average latency of the validator
    /// weighted by the reliability of the validator.
    pub performance: GaugeVec,

    /// Number of low latency validators that got shuffled.
    pub shuffled_validators: HistogramVec,
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

            performance: register_gauge_vec_with_registry!(
                "validator_client_observed_performance",
                "Current client-observed performance per validator. The performance is the average latency of the validator
                weighted by the reliability of the validator.",
                &["validator", "tx_type"],
                registry,
            )
            .unwrap(),

            shuffled_validators: register_histogram_vec_with_registry!(
                "validator_client_shuffled_validators",
                "Number of low latency validators that got shuffled",
                &["tx_type"],
                COUNT_BUCKETS.to_vec(),
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

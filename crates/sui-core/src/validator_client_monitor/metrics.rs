// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::{COUNT_BUCKETS, SUBSECOND_LATENCY_SEC_BUCKETS};
use prometheus::{
    GaugeVec, Histogram, HistogramVec, IntCounterVec, IntGauge, Registry,
    register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_with_registry,
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
    pub shuffled_validators: Histogram,

    /// Whether the driver currently considers staggered submission active (1) or not (0).
    pub staggering_active: IntGauge,

    /// Stake of validators with a fresh report of staggering being active.
    pub staggering_active_stake: IntGauge,
}

impl ValidatorClientMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            observed_latency: register_histogram_vec_with_registry!(
                "validator_client_observed_latency",
                "Client-observed latency of operations per validator",
                &["validator", "operation_type", "ping"],
                SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            operation_success: register_int_counter_vec_with_registry!(
                "validator_client_operation_success_total",
                "Total successful operations observed by client per validator",
                &["validator", "operation_type", "ping"],
                registry,
            )
            .unwrap(),

            operation_failure: register_int_counter_vec_with_registry!(
                "validator_client_operation_failure_total",
                "Total failed operations observed by client per validator",
                &["validator", "operation_type", "ping"],
                registry,
            )
            .unwrap(),

            performance: register_gauge_vec_with_registry!(
                "validator_client_observed_performance",
                "Current client-observed performance per validator and transaction class. The performance is the average latency of the validator
                weighted by the reliability of the validator.",
                &["validator", "tx_class"],
                registry,
            )
            .unwrap(),

            shuffled_validators: register_histogram_with_registry!(
                "validator_client_shuffled_validators",
                "Number of low latency validators that got shuffled",
                COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            staggering_active: register_int_gauge_with_registry!(
                "validator_client_staggering_active",
                "Whether the driver considers staggered submission active (1) or not (0); while active, transactions without allowed proposers are targeted at their free stagger slots",
                registry,
            )
            .unwrap(),

            staggering_active_stake: register_int_gauge_with_registry!(
                "validator_client_staggering_active_stake",
                "Stake of validators with a fresh report of staggering being active; staggering is considered active once this reaches the committee's validity threshold (f+1)",
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

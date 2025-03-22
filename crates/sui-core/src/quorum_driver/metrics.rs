// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    Registry,
};

const FINALITY_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85,
    0.9, 0.95, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6,
    2.7, 2.8, 2.9, 3.0, 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8, 3.9, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5,
    7.0, 7.5, 8.0, 8.5, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0,
    25.0,
];

#[derive(Clone)]
pub struct QuorumDriverMetrics {
    pub(crate) total_requests: IntCounter,
    pub(crate) total_enqueued: IntCounter,
    pub(crate) total_ok_responses: IntCounter,
    pub(crate) total_err_responses: IntCounterVec,
    pub(crate) attempt_times_ok_response: Histogram,

    // TODO: add histogram of attempt that tx succeeds
    pub(crate) current_requests_in_flight: IntGauge,

    pub(crate) total_retryable_overload_errors: IntCounter,
    pub(crate) transaction_retry_count: Histogram,
    pub(crate) current_transactions_in_retry: IntGauge,

    pub(crate) settlement_finality_latency: HistogramVec,
}

impl QuorumDriverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests: register_int_counter_with_registry!(
                "quorum_driver_total_requests",
                "Total number of requests received",
                registry,
            )
            .unwrap(),
            total_enqueued: register_int_counter_with_registry!(
                "quorum_driver_total_enqueued",
                "Total number of requests enqueued",
                registry,
            )
            .unwrap(),
            total_ok_responses: register_int_counter_with_registry!(
                "quorum_driver_total_ok_responses",
                "Total number of requests processed with Ok responses",
                registry,
            )
            .unwrap(),
            total_err_responses: register_int_counter_vec_with_registry!(
                "quorum_driver_total_err_responses",
                "Total number of requests returned with Err responses, grouped by error type",
                &["error"],
                registry,
            )
            .unwrap(),
            attempt_times_ok_response: register_histogram_with_registry!(
                "quorum_driver_attempt_times_ok_response",
                "Total attempt times of ok response",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            current_requests_in_flight: register_int_gauge_with_registry!(
                "current_requests_in_flight",
                "Current number of requests being processed in QuorumDriver",
                registry,
            )
            .unwrap(),
            total_retryable_overload_errors: register_int_counter_with_registry!(
                "quorum_driver_total_retryable_overload_errors",
                "Total number of transactions experiencing retryable overload error",
                registry,
            )
            .unwrap(),
            transaction_retry_count: register_histogram_with_registry!(
                "quorum_driver_transaction_retry_count",
                "Histogram of transaction retry count",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            current_transactions_in_retry: register_int_gauge_with_registry!(
                "current_transactions_in_retry",
                "Current number of transactions in retry loop in QuorumDriver",
                registry,
            )
            .unwrap(),
            settlement_finality_latency: register_histogram_vec_with_registry!(
                "quorum_driver_settlement_finality_latency",
                "Settlement finality latency observed from quorum driver",
                &["tx_type"],
                FINALITY_LATENCY_SEC_BUCKETS.to_vec(),
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

// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    Registry,
};

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
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
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

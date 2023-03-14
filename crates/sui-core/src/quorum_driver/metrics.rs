// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, Registry,
};

use mysten_metrics::histogram::Histogram;

#[derive(Clone)]
pub struct QuorumDriverMetrics {
    pub(crate) total_requests: IntCounter,
    pub(crate) total_enqueued: IntCounter,
    pub(crate) total_ok_responses: IntCounter,
    pub(crate) total_err_responses: IntCounterVec,
    pub(crate) attempt_times_ok_response: Histogram,

    // TODO: add histogram of attempt that tx succeeds
    pub(crate) current_requests_in_flight: IntGauge,

    pub(crate) total_err_process_tx_responses_with_nonzero_conflicting_transactions: IntCounter,
    pub(crate) total_attempts_retrying_conflicting_transaction: IntCounter,
    pub(crate) total_successful_attempts_retrying_conflicting_transaction: IntCounter,
    pub(crate) total_times_conflicting_transaction_already_finalized_when_retrying: IntCounter,
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
            attempt_times_ok_response: Histogram::new_in_registry(
                "quorum_driver_attempt_times_ok_response",
                "Total attempt times of ok response",
                registry,
            ),
            current_requests_in_flight: register_int_gauge_with_registry!(
                "current_requests_in_flight",
                "Current number of requests being processed in QuorumDriver",
                registry,
            )
            .unwrap(),
            total_err_process_tx_responses_with_nonzero_conflicting_transactions: register_int_counter_with_registry!(
                "quorum_driver_total_err_process_tx_responses_with_nonzero_conflicting_transactions",
                "Total number of err process_tx responses with non empty conflicting transactions",
                registry,
            )
            .unwrap(),
            total_attempts_retrying_conflicting_transaction: register_int_counter_with_registry!(
                "quorum_driver_total_attempts_trying_conflicting_transaction",
                "Total number of attempts to retry a conflicting transaction",
                registry,
            )
            .unwrap(),
            total_successful_attempts_retrying_conflicting_transaction: register_int_counter_with_registry!(
                "quorum_driver_total_successful_attempts_trying_conflicting_transaction",
                "Total number of successful attempts to retry a conflicting transaction",
                registry,
            )
            .unwrap(),
            total_times_conflicting_transaction_already_finalized_when_retrying: register_int_counter_with_registry!(
                "quorum_driver_total_times_conflicting_transaction_already_finalized_when_retrying",
                "Total number of times the conflicting transaction is already finalized when retrying",
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

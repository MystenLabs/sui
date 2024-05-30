// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, IntCounter, IntCounterVec, IntGaugeVec, Registry,
};

#[derive(Clone)]
pub struct BridgeMetrics {
    pub(crate) total_err_build_sui_transaction: IntCounter,
    pub(crate) total_err_sui_transaction_submission: IntCounter,
    pub(crate) total_err_sui_transaction_submission_too_many_failures: IntCounter,
    pub(crate) total_err_sui_transaction_execution: IntCounter,
    pub(crate) total_requests_received: IntCounterVec,
    pub(crate) total_requests_ok: IntCounterVec,
    pub(crate) total_requests_error: IntCounterVec,
    pub(crate) total_requests_inflight: IntGaugeVec,
}

impl BridgeMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_err_build_sui_transaction: register_int_counter_with_registry!(
                "bridge_total_err_build_sui_transaction",
                "Total number of errors of building sui transactions",
                registry,
            )
            .unwrap(),
            total_err_sui_transaction_submission: register_int_counter_with_registry!(
                "bridge_total_err_sui_transaction_submission",
                "Total number of errors of submitting sui transactions",
                registry,
            )
            .unwrap(),
            total_err_sui_transaction_submission_too_many_failures:
                register_int_counter_with_registry!(
                    "bridge_total_err_sui_transaction_submission_too_many_failures",
                    "Total number of continuous failures to submitting sui transactions",
                    registry,
                )
                .unwrap(),
            total_err_sui_transaction_execution: register_int_counter_with_registry!(
                "bridge_total_err_sui_transaction_execution",
                "Total number of failures of sui transaction execution",
                registry,
            )
            .unwrap(),
            total_requests_received: register_int_counter_vec_with_registry!(
                "bridge_total_requests_received",
                "Total number of requests received in Server, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            total_requests_ok: register_int_counter_vec_with_registry!(
                "bridge_total_requests_ok",
                "Total number of ok requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            total_requests_error: register_int_counter_vec_with_registry!(
                "bridge_total_requests_error",
                "Total number of erred requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
            total_requests_inflight: register_int_gauge_vec_with_registry!(
                "bridge_total_requests_inflight",
                "Total number of inflight requests, by request type",
                &["type"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_testing() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

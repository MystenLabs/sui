// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};

#[derive(Clone, Debug)]
pub struct QuorumDriverMetrics {
    pub(crate) total_requests_immediate_return: IntCounter,
    pub(crate) total_ok_responses_immediate_return: IntCounter,
    pub(crate) total_requests_wait_for_tx_cert: IntCounter,
    pub(crate) total_ok_responses_wait_for_tx_cert: IntCounter,
    pub(crate) total_requests_wait_for_effects_cert: IntCounter,
    pub(crate) total_ok_responses_wait_for_effects_cert: IntCounter,

    pub(crate) latency_sec_immediate_return: Histogram,
    pub(crate) latency_sec_wait_for_tx_cert: Histogram,
    pub(crate) latency_sec_wait_for_effects_cert: Histogram,

    pub(crate) current_requests_in_flight: IntGauge,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.01, 0.05, 0.1, 0.25, 0.5, 1., 2., 4., 6., 8., 10., 20., 30., 60., 90.,
];

impl QuorumDriverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests_immediate_return: register_int_counter_with_registry!(
                "quorum_driver_total_requests_immediate_return",
                "Total number of immediate_return requests received",
                registry,
            )
            .unwrap(),
            total_ok_responses_immediate_return: register_int_counter_with_registry!(
                "quorum_driver_total_ok_responses_immediate_return",
                "Total number of immediate_return requests processed with Ok responses",
                registry,
            )
            .unwrap(),
            total_requests_wait_for_tx_cert: register_int_counter_with_registry!(
                "quorum_driver_total_requests_wait_for_tx_cert",
                "Total number of wait_for_tx_cert requests received",
                registry,
            )
            .unwrap(),
            total_ok_responses_wait_for_tx_cert: register_int_counter_with_registry!(
                "quorum_driver_total_ok_responses_wait_for_tx_cert",
                "Total number of wait_fort_tx_cert requests processed with Ok responses",
                registry,
            )
            .unwrap(),
            total_requests_wait_for_effects_cert: register_int_counter_with_registry!(
                "quorum_driver_total_requests_wait_for_effects_cert",
                "Total number of wait_for_effects_cert requests received",
                registry,
            )
            .unwrap(),
            total_ok_responses_wait_for_effects_cert: register_int_counter_with_registry!(
                "quorum_driver_total_ok_responses_wait_for_effects_cert",
                "Total number of wait_for_effects_cert requests processed with Ok responses",
                registry,
            )
            .unwrap(),
            latency_sec_immediate_return: register_histogram_with_registry!(
                "quorum_driver_latency_sec_immediate_return",
                "Latency of processing an immdediate_return execution request, in sec",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latency_sec_wait_for_tx_cert: register_histogram_with_registry!(
                "quorum_driver_latency_sec_wait_for_tx_cert",
                "Latency of processing an wait_for_tx_cert execution request, in sec",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latency_sec_wait_for_effects_cert: register_histogram_with_registry!(
                "quorum_driver_latency_sec_wait_for_effects_cert",
                "Latency of processing an wait_for_effects_cert execution request, in sec",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            current_requests_in_flight: register_int_gauge_with_registry!(
                "current_requests_in_flight",
                "Current number of requests being processed in QuorumDriver",
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

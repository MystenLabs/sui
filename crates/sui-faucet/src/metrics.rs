// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
#[derive(Clone, Debug)]
pub struct FaucetMetrics {
    pub(crate) total_requests_received: IntCounter,
    pub(crate) total_requests_succeeded: IntCounter,
    pub(crate) current_requests_in_flight: IntGauge,
    pub(crate) process_latency: Histogram,
}

impl FaucetMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests_received: register_int_counter_with_registry!(
                "total_requests_received",
                "Total number of requests received in Faucet",
                registry,
            )
            .unwrap(),
            total_requests_succeeded: register_int_counter_with_registry!(
                "total_requests_succeeded",
                "Total number of requests processed successfully in Faucet",
                registry,
            )
            .unwrap(),
            current_requests_in_flight: register_int_gauge_with_registry!(
                "current_requests_in_flight",
                "Current number of requests being processed in Faucet",
                registry,
            )
            .unwrap(),
            process_latency: register_histogram_with_registry!(
                "process_latency",
                "Latency of processing a Faucet request",
                registry,
            )
            .unwrap(),
        }
    }
}

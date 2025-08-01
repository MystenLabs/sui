// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};

/// Histogram buckets for the distribution of latency (time between sending a DB request and
/// receiving a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

#[derive(Clone)]
pub(crate) struct DbReaderMetrics {
    pub latency: Histogram,
    pub requests_received: IntCounter,
    pub requests_succeeded: IntCounter,
    pub requests_failed: IntCounter,
}

impl DbReaderMetrics {
    pub(crate) fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("db");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            latency: register_histogram_with_registry!(
                name("latency"),
                "Time taken by the database to respond to queries",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_with_registry!(
                name("requests_received"),
                "Number of database requests sent by the service",
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_with_registry!(
                name("requests_succeeded"),
                "Number of database requests that completed successfully",
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_with_registry!(
                name("requests_failed"),
                "Number of database requests that completed with an error",
                registry,
            )
            .unwrap(),
        })
    }
}

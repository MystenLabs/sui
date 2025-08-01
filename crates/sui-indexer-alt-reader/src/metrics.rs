// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, Registry,
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

#[derive(Clone)]
pub(crate) struct ConsistentReaderMetrics {
    pub latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,
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

impl ConsistentReaderMetrics {
    pub(crate) fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("consistent_store");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            latency: register_histogram_vec_with_registry!(
                name("latency"),
                "Time taken by the consistent store to respond to queries",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                name("requests_received"),
                "Number of consistent store requests sent by the service",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                name("requests_succeeded"),
                "Number of consistent store requests that completed successfully",
                &["method"],
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                name("requests_failed"),
                "Number of consistent store requests that completed with an error",
                &["method"],
                registry,
            )
            .unwrap(),
        })
    }
}

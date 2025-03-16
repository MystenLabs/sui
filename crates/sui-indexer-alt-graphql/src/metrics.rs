// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

pub struct RpcMetrics {
    // Top-level metrics for all read requests (queries).
    pub query_latency: Histogram,
    pub queries_received: IntCounter,
    pub queries_succeeded: IntCounter,
    pub queries_failed: IntCounter,
    pub queries_in_flight: IntGauge,

    // Metrics per type and field.
    pub fields_received: IntCounterVec,
    pub fields_succeeded: IntCounterVec,
    pub fields_failed: IntCounterVec,
}

impl RpcMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            query_latency: register_histogram_with_registry!(
                "query_latency",
                "Time taken to respond to read requests",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            queries_received: register_int_counter_with_registry!(
                "queries_received",
                "Number of read requests the service has received",
                registry,
            )
            .unwrap(),

            queries_succeeded: register_int_counter_with_registry!(
                "queries_succeeded",
                "Number of read requests that have completed without any errors",
                registry,
            )
            .unwrap(),

            queries_failed: register_int_counter_with_registry!(
                "queries_failed",
                "Number of read requests that have completed with at least one error",
                registry,
            )
            .unwrap(),

            queries_in_flight: register_int_gauge_with_registry!(
                "queries_in_flight",
                "Number of read requests currently flowing through the service",
                registry
            )
            .unwrap(),

            fields_received: register_int_counter_vec_with_registry!(
                "fields_received",
                "Number of times a field of a type has been requested in the GraphQL schema",
                &["type", "field"],
                registry,
            )
            .unwrap(),

            fields_succeeded: register_int_counter_vec_with_registry!(
                "fields_succeeded",
                "Number of times a field of a type has been successfully resolved",
                &["type", "field"],
                registry,
            )
            .unwrap(),

            fields_failed: register_int_counter_vec_with_registry!(
                "fields_failed",
                "Number of times a field of a type has failed to resolve",
                &["type", "field"],
                registry,
            )
            .unwrap(),
        })
    }
}

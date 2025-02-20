// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, Registry,
};

pub(crate) mod middleware;

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

#[derive(Clone)]
pub struct RpcMetrics {
    pub db_latency: Histogram,
    pub db_requests_received: IntCounter,
    pub db_requests_succeeded: IntCounter,
    pub db_requests_failed: IntCounter,

    pub request_latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,
}

impl RpcMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            db_latency: register_histogram_with_registry!(
                "db_latency",
                "Time taken by the database to respond to queries",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),

            db_requests_received: register_int_counter_with_registry!(
                "db_requests_received",
                "Number of database requests sent by the service",
                registry,
            ).unwrap(),

            db_requests_succeeded: register_int_counter_with_registry!(
                "db_requests_succeeded",
                "Number of database requests that completed successfully",
                registry,
            ).unwrap(),

            db_requests_failed: register_int_counter_with_registry!(
                "db_requests_failed",
                "Number of database requests that completed with an error",
                registry,
            ).unwrap(),

            request_latency: register_histogram_vec_with_registry!(
                "rpc_request_latency",
                "Time taken to respond to JSON-RPC requests, by method",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                "rpc_requests_received",
                "Number of requests initiated for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                "rpc_requests_succeeded",
                "Number of requests that completed successfully for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                "rpc_requests_failed",
                "Number of requests that completed with an error for each JSON-RPC method, by error code",
                &["method", "code"],
                registry
            )
            .unwrap(),
        })
    }
}

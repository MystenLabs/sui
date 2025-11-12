// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    HistogramVec, IntCounterVec, Registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry,
};

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

pub struct RpcMetrics {
    pub request_latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,
    pub requests_cancelled: IntCounterVec,
}

impl RpcMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            request_latency: register_histogram_vec_with_registry!(
                "consistent_rpc_request_latency",
                "Time taken to response to gRPC requests, by path",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                "consistent_rpc_requests_received",
                "Number of requests initiated for each gRPC method",
                &["path"],
                registry,
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                "consistent_rpc_requests_succeeded",
                "Number of requests that completed successfully, by path",
                &["path", "code"],
                registry,
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                "consistent_rpc_requests_failed",
                "Number of requests that completed with an error, by path",
                &["path", "code"],
                registry,
            )
            .unwrap(),

            requests_cancelled: register_int_counter_vec_with_registry!(
                "consistent_rpc_requests_cancelled",
                "Number of requests that were cancelled before completion, by path",
                &["path"],
                registry,
            )
            .unwrap(),
        }
    }
}

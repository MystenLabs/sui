// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, Histogram, HistogramVec, IntCounterVec, Registry,
};

pub(crate) mod middleware;

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

/// Histogram buckets for the distribution of the number of pages of data fetched/scanned from the
/// database.
const PAGE_SCAN_BUCKETS: &[f64] = &[1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0];

#[derive(Clone)]
pub struct RpcMetrics {
    pub request_latency: HistogramVec,
    pub requests_received: IntCounterVec,
    pub requests_succeeded: IntCounterVec,
    pub requests_failed: IntCounterVec,

    pub owned_objects_filter_scans: Histogram,
    pub read_retries: IntCounterVec,
}

impl RpcMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            request_latency: register_histogram_vec_with_registry!(
                "jsonrpc_request_latency",
                "Time taken to respond to JSON-RPC requests, by method",
                &["method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            requests_received: register_int_counter_vec_with_registry!(
                "jsonrpc_requests_received",
                "Number of requests initiated for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_succeeded: register_int_counter_vec_with_registry!(
                "jsonrpc_requests_succeeded",
                "Number of requests that completed successfully for each JSON-RPC method",
                &["method"],
                registry
            )
            .unwrap(),

            requests_failed: register_int_counter_vec_with_registry!(
                "jsonrpc_requests_failed",
                "Number of requests that completed with an error for each JSON-RPC method, by error code",
                &["method", "code"],
                registry
            )
            .unwrap(),

            owned_objects_filter_scans: register_histogram_with_registry!(
                "jsonrpc_owned_objects_filter_scans",
                "Number of pages of owned objects scanned in response to compound owned object filters",
                PAGE_SCAN_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            read_retries: register_int_counter_vec_with_registry!(
                "read_retries",
                "Number of retries for reads from Bigtable or Postgres tables",
                &["table"],
                registry
            )
            .unwrap(),
        })
    }
}

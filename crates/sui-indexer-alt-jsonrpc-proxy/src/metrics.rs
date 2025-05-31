// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, GaugeVec, HistogramVec, IntCounterVec, Registry,
};

#[derive(Clone)]
pub struct AppMetrics {
    pub backend_up: GaugeVec,
    pub requests_total: IntCounterVec,
    pub request_latency: HistogramVec,
    pub upstream_response_latency: HistogramVec,
    pub response_size_bytes: HistogramVec,
    pub request_size_bytes: HistogramVec,
    pub timeouts_total: IntCounterVec,
    pub error_counts: IntCounterVec,
}

impl AppMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            backend_up: register_gauge_vec_with_registry!(
                "proxy_backend_up",
                "Indicates if the backend is up (1) or down (0)",
                &["method"],
                registry
            )
            .unwrap(),
            requests_total: register_int_counter_vec_with_registry!(
                "proxy_requests_total",
                "Total number of requests processed by the edge proxy",
                &["method", "status"],
                registry
            )
            .unwrap(),
            request_latency: register_histogram_vec_with_registry!(
                "proxy_request_latency",
                "Request latency in seconds",
                &["method"],
                registry
            )
            .unwrap(),
            upstream_response_latency: register_histogram_vec_with_registry!(
                "proxy_upstream_response_latency",
                "Upstream response latency in seconds",
                &["method", "status"],
                registry
            )
            .unwrap(),
            response_size_bytes: register_histogram_vec_with_registry!(
                "proxy_response_size_bytes",
                "Size of responses in bytes",
                &["method"],
                registry
            )
            .unwrap(),
            request_size_bytes: register_histogram_vec_with_registry!(
                "proxy_request_size_bytes",
                "Size of incoming requests in bytes",
                &["method"],
                registry
            )
            .unwrap(),
            timeouts_total: register_int_counter_vec_with_registry!(
                "proxy_timeouts_total",
                "Total number of timed-out requests",
                &["method"],
                registry
            )
            .unwrap(),
            error_counts: register_int_counter_vec_with_registry!(
                "proxy_error_counts",
                "Total number of errors encountered by the edge proxy",
                &["method", "error_type"],
                registry
            )
            .unwrap(),
        }
    }
}

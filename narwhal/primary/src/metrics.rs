// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::EndpointMetrics;
use mysten_network::metrics::MetricsCallbackProvider;
use prometheus::{
    default_registry, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};
use std::time::Duration;
use tonic::Code;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub(crate) endpoint_metrics: Option<EndpointMetrics>,
    pub(crate) primary_endpoint_metrics: Option<PrimaryEndpointMetrics>,
    pub(crate) node_metrics: Option<PrimaryMetrics>,
}

/// Initialises the metrics. Should be called only once when the primary
/// node is initialised, otherwise it will lead to erroneously creating
/// multiple registries.
pub(crate) fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    // The metrics used for the gRPC primary node endpoints we expose to the external consensus
    let endpoint_metrics = EndpointMetrics::new(metrics_registry);

    // The metrics used for the primary-to-primary communication node endpoints
    let primary_endpoint_metrics = PrimaryEndpointMetrics::new(metrics_registry);

    // Essential/core metrics across the primary node
    let node_metrics = PrimaryMetrics::new(metrics_registry);

    Metrics {
        node_metrics: Some(node_metrics),
        endpoint_metrics: Some(endpoint_metrics),
        primary_endpoint_metrics: Some(primary_endpoint_metrics),
    }
}

#[derive(Clone)]
pub struct PrimaryMetrics {
    /// count number of headers that the node processed (others + own)
    pub headers_processed: IntCounterVec,
    /// count unique number of headers that we have received for processing (others + own)
    pub unique_headers_received: IntCounterVec,
    /// count number of headers that we suspended their processing
    pub headers_suspended: IntCounterVec,
    /// count number of certificates that the node created
    pub certificates_created: IntCounterVec,
    /// count number of certificates that the node processed (others + own)
    pub certificates_processed: IntCounterVec,
    /// count number of certificates that the node suspended their processing
    pub certificates_suspended: IntCounterVec,
    /// Batch digests received
    pub batches_received: IntCounterVec,
    /// Latency to perform a garbage collection in core module
    pub gc_core_latency: HistogramVec,
    /// Number of cancel handlers for core module
    pub core_cancel_handlers_total: IntGaugeVec,
    /// The current Narwhal round
    pub current_round: IntGaugeVec,
    /// Latency to perform a garbage collection in header_waiter
    pub gc_header_waiter_latency: HistogramVec,
    /// Number of elements in pending list of header_waiter
    pub pending_elements_header_waiter: IntGaugeVec,
    /// Number of parent requests list of header_waiter
    pub parent_requests_header_waiter: IntGaugeVec,
    /// Number of elements in pending list of certificate_waiter
    pub pending_elements_certificate_waiter: IntGaugeVec,
}

impl PrimaryMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            headers_processed: register_int_counter_vec_with_registry!(
                "headers_processed",
                "Number of headers that node processed (others + own)",
                &["epoch", "source"],
                registry
            )
            .unwrap(),
            unique_headers_received: register_int_counter_vec_with_registry!(
                "unique_headers_received",
                "Number of unique headers that received for processing (others + own)",
                &["epoch", "source"],
                registry
            )
            .unwrap(),
            headers_suspended: register_int_counter_vec_with_registry!(
                "headers_suspended",
                "Number of headers that node suspended processing for",
                &["epoch", "reason"],
                registry
            )
            .unwrap(),
            certificates_created: register_int_counter_vec_with_registry!(
                "certificates_created",
                "Number of certificates that node created",
                &["epoch"],
                registry
            )
            .unwrap(),
            certificates_processed: register_int_counter_vec_with_registry!(
                "certificates_processed",
                "Number of certificates that node processed (others + own)",
                &["epoch", "source"],
                registry
            )
            .unwrap(),
            certificates_suspended: register_int_counter_vec_with_registry!(
                "certificates_suspended",
                "Number of certificates that node suspended processing of",
                &["epoch", "reason"],
                registry
            )
            .unwrap(),
            batches_received: register_int_counter_vec_with_registry!(
                "batches_received",
                "Number of batches received - either own or others",
                &["worker_id", "source"],
                registry
            )
            .unwrap(),
            gc_core_latency: register_histogram_vec_with_registry!(
                "gc_core_latency",
                "Latency of a the garbage collection process for core module",
                &["epoch"],
                registry
            )
            .unwrap(),
            core_cancel_handlers_total: register_int_gauge_vec_with_registry!(
                "core_cancel_handlers_total",
                "Number of cancel handlers in the core module",
                &["epoch"],
                registry
            )
            .unwrap(),
            current_round: register_int_gauge_vec_with_registry!(
                "current_round",
                "Current round the node is in",
                &["epoch"],
                registry
            )
            .unwrap(),
            gc_header_waiter_latency: register_histogram_vec_with_registry!(
                "gc_header_waiter_latency",
                "Latency of a the garbage collection process for header module",
                &["epoch"],
                registry
            )
            .unwrap(),
            pending_elements_header_waiter: register_int_gauge_vec_with_registry!(
                "pending_elements_header_waiter",
                "Number of pending elements in header waiter",
                &["epoch"],
                registry
            )
            .unwrap(),
            parent_requests_header_waiter: register_int_gauge_vec_with_registry!(
                "parent_requests_header_waiter",
                "Number of parent requests in header waiter",
                &["epoch"],
                registry
            )
            .unwrap(),
            pending_elements_certificate_waiter: register_int_gauge_vec_with_registry!(
                "pending_elements_certificate_waiter",
                "Number of pending elements in certificate waiter",
                &["epoch"],
                registry
            )
            .unwrap(),
        }
    }
}

impl Default for PrimaryMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

#[derive(Clone)]
pub struct PrimaryEndpointMetrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
}

impl PrimaryEndpointMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            requests_by_route: register_int_counter_vec_with_registry!(
                "primary_requests_by_route",
                "Number of requests by route",
                &["route", "status", "grpc_status_code"],
                registry
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec_with_registry!(
                "primary_req_latency_by_route",
                "Latency of a request by route",
                &["route", "status", "grpc_status_code"],
                registry
            )
            .unwrap(),
        }
    }
}

impl MetricsCallbackProvider for PrimaryEndpointMetrics {
    fn on_request(&self, _path: String) {
        // For now we just do nothing
    }

    fn on_response(&self, path: String, latency: Duration, status: u16, grpc_status_code: Code) {
        let code: i32 = grpc_status_code.into();
        let labels = [path.as_str(), &status.to_string(), &code.to_string()];

        self.requests_by_route.with_label_values(&labels).inc();

        let req_latency_secs = latency.as_secs_f64();
        self.req_latency_by_route
            .with_label_values(&labels)
            .observe(req_latency_secs);
    }
}

impl Default for PrimaryEndpointMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

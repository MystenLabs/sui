// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::EndpointMetrics;
use prometheus::{
    default_registry, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};
use std::sync::Once;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub(crate) endpoint_metrics: Option<EndpointMetrics>,
    pub(crate) node_metrics: Option<PrimaryMetrics>,
}

static mut METRICS: Metrics = Metrics {
    endpoint_metrics: None,
    node_metrics: None,
};
static INIT: Once = Once::new();

/// Initialises the metrics. Should be called only once when the primary
/// node is initialised, otherwise it will lead to erroneously creating
/// multiple registries.
pub(crate) fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    unsafe {
        INIT.call_once(|| {
            // The metrics used for the primary node endpoints
            let endpoint_metrics = EndpointMetrics::new(metrics_registry);

            // Essential/core metrics across the primary node
            let node_metrics = PrimaryMetrics::new(metrics_registry);

            METRICS = Metrics {
                node_metrics: Some(node_metrics),
                endpoint_metrics: Some(endpoint_metrics),
            }
        });
        METRICS.clone()
    }
}

#[derive(Clone)]
pub struct PrimaryMetrics {
    /// count number of headers that the node processed (others + own)
    pub headers_processed: IntCounterVec,
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
    /// The current Narwhal round
    pub current_round: IntGaugeVec,
    /// Latency to perform a garbage collection in header_waiter
    pub gc_header_waiter_latency: HistogramVec,
    /// Number of elements in pending list of header_waiter
    pub pending_elements_header_waiter: IntGaugeVec,
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
        }
    }
}

impl Default for PrimaryMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

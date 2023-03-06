// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use mysten_network::metrics::MetricsCallbackProvider;
use prometheus::{
    default_registry, register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    Registry,
};
use std::time::Duration;
use tonic::Code;

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4,
    1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.,
    12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

#[derive(Clone)]
pub struct Metrics {
    pub worker_metrics: Option<WorkerMetrics>,
    pub channel_metrics: Option<WorkerChannelMetrics>,
    pub endpoint_metrics: Option<WorkerEndpointMetrics>,
}

/// Initialises the metrics
pub fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    // Essential/core metrics across the worker node
    let node_metrics = WorkerMetrics::new(metrics_registry);

    // Channel metrics
    let channel_metrics = WorkerChannelMetrics::new(metrics_registry);

    // Endpoint metrics
    let endpoint_metrics = WorkerEndpointMetrics::new(metrics_registry);

    Metrics {
        worker_metrics: Some(node_metrics),
        channel_metrics: Some(channel_metrics),
        endpoint_metrics: Some(endpoint_metrics),
    }
}

#[derive(Clone)]
pub struct WorkerMetrics {
    /// Number of created batches from the batch_maker
    pub created_batch_size: HistogramVec,
    /// Time taken to create a batch
    pub created_batch_latency: HistogramVec,
    /// The number of parallel worker batches currently processed by the worker
    pub parallel_worker_batches: IntGauge,
    /// Counter of remote/local batch fetch statuses.
    pub worker_batch_fetch: IntCounterVec,
    /// Time it takes to download a payload from local worker peer
    pub worker_local_fetch_latency: Histogram,
    /// Time it takes to download a payload from remote peer
    pub worker_remote_fetch_latency: Histogram,
    /// The number of pending remote calls to request_batch
    pub pending_remote_request_batch: IntGauge,
}

impl WorkerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            created_batch_size: register_histogram_vec_with_registry!(
                "created_batch_size",
                "Size in bytes of the created batches",
                &["reason"],
                // buckets with size in bytes
                vec![
                    100.0,
                    500.0,
                    1_000.0,
                    5_000.0,
                    10_000.0,
                    20_000.0,
                    50_000.0,
                    100_000.0,
                    250_000.0,
                    500_000.0,
                    1_000_000.0
                ],
                registry
            )
            .unwrap(),
            created_batch_latency: register_histogram_vec_with_registry!(
                "created_batch_latency",
                "The latency of creating (sealing) a batch",
                &["reason"],
                // buckets in seconds
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            parallel_worker_batches: register_int_gauge_with_registry!(
                "parallel_worker_batches",
                "The number of parallel worker batches currently processed by the worker",
                registry
            )
            .unwrap(),
            worker_batch_fetch: register_int_counter_vec_with_registry!(
                "worker_batch_fetch",
                "Counter of remote/local batch fetch statuses",
                &["source", "status"],
                registry
            )
            .unwrap(),
            worker_local_fetch_latency: register_histogram_with_registry!(
                "worker_local_fetch_latency",
                "Time it takes to download a payload from local storage",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            worker_remote_fetch_latency: register_histogram_with_registry!(
                "worker_remote_fetch_latency",
                "Time it takes to download a payload from remote worker peer",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            pending_remote_request_batch: register_int_gauge_with_registry!(
                "pending_remote_request_batch",
                "The number of pending remote calls to request_batch",
                registry
            )
            .unwrap(),
        }
    }
}

impl Default for WorkerMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

#[derive(Clone)]
pub struct WorkerChannelMetrics {
    /// occupancy of the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker: IntGauge,
    /// occupancy of the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter: IntGauge,
    /// total received from the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker_total: IntCounter,
    /// total received from the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter_total: IntCounter,
}

impl WorkerChannelMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_batch_maker: register_int_gauge_with_registry!(
                "tx_batch_maker",
                "occupancy of the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`",
                registry
            ).unwrap(),
            tx_quorum_waiter: register_int_gauge_with_registry!(
                "tx_quorum_waiter",
                "occupancy of the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`",
                registry
            ).unwrap(),

            // Totals:
            tx_batch_maker_total: register_int_counter_with_registry!(
                "tx_batch_maker_total",
                "total received from the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`",
                registry
            ).unwrap(),
            tx_quorum_waiter_total: register_int_counter_with_registry!(
                "tx_quorum_waiter_total",
                "total received from the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`",
                registry
            ).unwrap(),
        }
    }
}

#[derive(Clone)]
pub struct WorkerEndpointMetrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
}

impl WorkerEndpointMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            requests_by_route: register_int_counter_vec_with_registry!(
                "worker_requests_by_route",
                "Number of requests by route",
                &["route", "status", "grpc_status_code"],
                registry
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec_with_registry!(
                "worker_req_latency_by_route",
                "Latency of a request by route",
                &["route", "status", "grpc_status_code"],
                registry
            )
            .unwrap(),
        }
    }
}

impl MetricsCallbackProvider for WorkerEndpointMetrics {
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

impl Default for WorkerEndpointMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

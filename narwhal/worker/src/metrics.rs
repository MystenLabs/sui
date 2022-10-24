// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use mysten_network::metrics::MetricsCallbackProvider;
use network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use prometheus::{
    default_registry, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use std::time::Duration;
use tonic::Code;

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.01, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 50.0, 100.0,
    200.0,
];

#[derive(Clone)]
pub struct Metrics {
    pub worker_metrics: Option<WorkerMetrics>,
    pub channel_metrics: Option<WorkerChannelMetrics>,
    pub endpoint_metrics: Option<WorkerEndpointMetrics>,
    pub inbound_network_metrics: Option<NetworkMetrics>,
    pub outbound_network_metrics: Option<NetworkMetrics>,
    pub network_connection_metrics: Option<NetworkConnectionMetrics>,
}

/// Initialises the metrics
pub fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    // Essential/core metrics across the worker node
    let node_metrics = WorkerMetrics::new(metrics_registry);

    // Channel metrics
    let channel_metrics = WorkerChannelMetrics::new(metrics_registry);

    // Endpoint metrics
    let endpoint_metrics = WorkerEndpointMetrics::new(metrics_registry);

    // The metrics used for communicating over the network
    let inbound_network_metrics = NetworkMetrics::new("worker", "inbound", metrics_registry);
    let outbound_network_metrics = NetworkMetrics::new("worker", "outbound", metrics_registry);

    // Network metrics for the worker connection
    let network_connection_metrics = NetworkConnectionMetrics::new("worker", metrics_registry);

    Metrics {
        worker_metrics: Some(node_metrics),
        channel_metrics: Some(channel_metrics),
        endpoint_metrics: Some(endpoint_metrics),
        inbound_network_metrics: Some(inbound_network_metrics),
        outbound_network_metrics: Some(outbound_network_metrics),
        network_connection_metrics: Some(network_connection_metrics),
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
}

impl WorkerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            created_batch_size: register_histogram_vec_with_registry!(
                "created_batch_size",
                "Size in bytes of the created batches",
                &["epoch", "reason"],
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
                &["epoch", "reason"],
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
    /// occupancy of the channel from various handlers to the `worker::PrimaryConnector`
    pub tx_our_batch: IntGauge,
    /// occupancy of the channel from various handlers to the `worker::PrimaryConnector`
    pub tx_others_batch: IntGauge,
    /// occupancy of the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker: IntGauge,
    /// occupancy of the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter: IntGauge,

    // Record the total events received to infer progress rates
    /// total received from the channel from various handlers to the `worker::PrimaryConnector`
    pub tx_our_batch_total: IntCounter,
    /// total received from the channel from various handlers to the `worker::PrimaryConnector`
    pub tx_others_batch_total: IntCounter,
    /// total received from the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker_total: IntCounter,
    /// total received from the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter_total: IntCounter,
}

impl WorkerChannelMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_our_batch: register_int_gauge_with_registry!(
                "tx_our_batch",
                "occupancy of the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
            tx_others_batch: register_int_gauge_with_registry!(
                "tx_others_batch",
                "occupancy of the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
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

            tx_our_batch_total: register_int_counter_with_registry!(
                "tx_our_batch_total",
                "total received from the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
            tx_others_batch_total: register_int_counter_with_registry!(
                "tx_others_batch_total",
                "total received from the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
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

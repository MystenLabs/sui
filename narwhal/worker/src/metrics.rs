// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use mysten_network::metrics::MetricsCallbackProvider;
use network::metrics::NetworkMetrics;
use prometheus::{
    default_registry, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Registry,
};
use std::time::Duration;
use tonic::Code;

#[derive(Clone)]
pub struct Metrics {
    pub worker_metrics: Option<WorkerMetrics>,
    pub channel_metrics: Option<WorkerChannelMetrics>,
    pub endpoint_metrics: Option<WorkerEndpointMetrics>,
    pub inbound_network_metrics: Option<NetworkMetrics>,
    pub outbound_network_metrics: Option<NetworkMetrics>,
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

    Metrics {
        worker_metrics: Some(node_metrics),
        channel_metrics: Some(channel_metrics),
        endpoint_metrics: Some(endpoint_metrics),
        inbound_network_metrics: Some(inbound_network_metrics),
        outbound_network_metrics: Some(outbound_network_metrics),
    }
}

#[derive(Clone)]
pub struct WorkerMetrics {
    /// Number of elements pending elements in the worker synchronizer
    pub pending_elements_worker_synchronizer: IntGaugeVec,
    /// Number of created batches from the batch_maker
    pub created_batch_size: HistogramVec,
}

impl WorkerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            pending_elements_worker_synchronizer: register_int_gauge_vec_with_registry!(
                "pending_elements_worker_synchronizer",
                "Number of pending elements in worker block synchronizer",
                &["epoch"],
                registry
            )
            .unwrap(),
            created_batch_size: register_histogram_vec_with_registry!(
                "created_batch_size",
                "Size in bytes of the created batches",
                &["epoch", "reason"],
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
    pub tx_primary: IntGauge,
    /// occupancy of the channel from the `handlers::PrimaryReceiverHandler` to the `worker::Synchronizer`
    pub tx_synchronizer: IntGauge,
    /// occupancy of the channel from the `handlers::PrimaryReceiverHandler` to the `handlers::ChildRpcSender`
    pub tx_request_batches_rpc: IntGauge,
    /// occupancy of the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker: IntGauge,
    /// occupancy of the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter: IntGauge,
    /// occupancy of the channel from the `worker::WorkerReceiverHandler` to the `worker::Processor`
    pub tx_worker_processor: IntGauge,
    /// occupancy of the channel from the `worker::QuorumWaiter` to the `worker::Processor`
    pub tx_client_processor: IntGauge,
    /// occupancy of the channel from the `worker::WorkerReceiverHandler` to the `worker::Helper` (carrying worker requests)
    pub tx_worker_helper: IntGauge,

    // Record the total events received to infer progress rates
    /// total received from the channel from various handlers to the `worker::PrimaryConnector`
    pub tx_primary_total: IntCounter,
    /// total received from the channel from the `handlers::PrimaryReceiverHandler` to the `worker::Synchronizer`
    pub tx_synchronizer_total: IntCounter,
    /// total received from the channel from the `handlers::PrimaryReceiverHandler` to the `handlers::ChildRpcSender`
    pub tx_request_batches_rpc_total: IntCounter,
    /// total received from the channel from the `worker::TxReceiverhandler` to the `worker::BatchMaker`
    pub tx_batch_maker_total: IntCounter,
    /// total received from the channel from the `worker::BatchMaker` to the `worker::QuorumWaiter`
    pub tx_quorum_waiter_total: IntCounter,
    /// total received from the channel from the `worker::WorkerReceiverHandler` to the `worker::Processor`
    pub tx_worker_processor_total: IntCounter,
    /// total received from the channel from the `worker::QuorumWaiter` to the `worker::Processor`
    pub tx_client_processor_total: IntCounter,
    /// total received from the channel from the `worker::WorkerReceiverHandler` to the `worker::Helper` (carrying worker requests)
    pub tx_worker_helper_total: IntCounter,
}

impl WorkerChannelMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_primary: register_int_gauge_with_registry!(
                "tx_primary",
                "occupancy of the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
            tx_synchronizer: register_int_gauge_with_registry!(
                "tx_synchronizer",
                "occupancy of the channel from the `worker::PrimaryReceiverHandler` to the `worker::Synchronizer`",
                registry
            ).unwrap(),
            tx_request_batches_rpc: register_int_gauge_with_registry!(
                "tx_request_batches_rpc",
                "occupancy of the channel from the `handlers::PrimaryReceiverHandler` to the `handlers::ChildRpcSender`",
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
            tx_worker_processor: register_int_gauge_with_registry!(
                "tx_worker_processor",
                "occupancy of the channel from the `worker::WorkerReceiverHandler` to the `worker::Processor`",
                registry
            ).unwrap(),
            tx_client_processor: register_int_gauge_with_registry!(
                "tx_client_processor",
                "occupancy of the channel from the `worker::QuorumWaiter` to the `worker::Processor`",
                registry
            ).unwrap(),
            tx_worker_helper: register_int_gauge_with_registry!(
                "tx_worker_helper",
                "occupancy of the channel from the `worker::WorkerReceiverHandler` to the `worker::Helper` (carrying worker requests)",
                registry
            ).unwrap(),

            // Totals:

            tx_primary_total: register_int_counter_with_registry!(
                "tx_primary_total",
                "total received from the channel from various handlers to the `worker::PrimaryConnector`",
                registry
            ).unwrap(),
            tx_synchronizer_total: register_int_counter_with_registry!(
                "tx_synchronizer_total",
                "total received from the channel from the `worker::PrimaryReceiverHandler` to the `worker::Synchronizer`",
                registry
            ).unwrap(),
            tx_request_batches_rpc_total: register_int_counter_with_registry!(
                "tx_request_batches_rpc_total",
                "total received from the channel from the `handlers::PrimaryReceiverHandler` to the `handlers::ChildRpcSender`",
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
            tx_worker_processor_total: register_int_counter_with_registry!(
                "tx_worker_processor_total",
                "total received from the channel from the `worker::WorkerReceiverHandler` to the `worker::Processor`",
                registry
            ).unwrap(),
            tx_client_processor_total: register_int_counter_with_registry!(
                "tx_client_processor_total",
                "total received from the channel from the `worker::QuorumWaiter` to the `worker::Processor`",
                registry
            ).unwrap(),
            tx_worker_helper_total: register_int_counter_with_registry!(
                "tx_worker_helper_total",
                "total received from the channel from the `worker::WorkerReceiverHandler` to the `worker::Helper` (carrying worker requests)",
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

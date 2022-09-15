// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::EndpointMetrics;
use mysten_network::metrics::MetricsCallbackProvider;
use network::{metrics, metrics::PrimaryNetworkMetrics};
use prometheus::{
    core::{AtomicI64, GenericGauge},
    default_registry, register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, HistogramVec,
    IntCounterVec, IntGauge, IntGaugeVec, Registry,
};
use std::time::Duration;
use tonic::Code;

#[derive(Clone)]
pub(crate) struct Metrics {
    pub(crate) endpoint_metrics: Option<EndpointMetrics>,
    pub(crate) primary_endpoint_metrics: Option<PrimaryEndpointMetrics>,
    pub(crate) primary_channel_metrics: Option<PrimaryChannelMetrics>,
    pub(crate) node_metrics: Option<PrimaryMetrics>,
    pub(crate) network_metrics: Option<PrimaryNetworkMetrics>,
}

/// Initialises the metrics
pub(crate) fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    // The metrics used for the gRPC primary node endpoints we expose to the external consensus
    let endpoint_metrics = EndpointMetrics::new(metrics_registry);

    // The metrics used for the primary-to-primary communication node endpoints
    let primary_endpoint_metrics = PrimaryEndpointMetrics::new(metrics_registry);

    // The metrics used for measuring the occupancy of the channels in the primary
    let primary_channel_metrics = PrimaryChannelMetrics::new(metrics_registry);

    // Essential/core metrics across the primary node
    let node_metrics = PrimaryMetrics::new(metrics_registry);

    // Network metrics for the primary to primary comms
    let network_metrics = metrics::PrimaryNetworkMetrics::new(metrics_registry);

    Metrics {
        node_metrics: Some(node_metrics),
        endpoint_metrics: Some(endpoint_metrics),
        primary_channel_metrics: Some(primary_channel_metrics),
        primary_endpoint_metrics: Some(primary_endpoint_metrics),
        network_metrics: Some(network_metrics),
    }
}

#[derive(Clone)]
pub struct PrimaryChannelMetrics {
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::PayloadReceiver`
    pub tx_others_digests: IntGauge,
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::Proposer`
    pub tx_our_digests: IntGauge,
    /// occupancy of the channel from the `primary::Core` to the `primary::Proposer`
    pub tx_parents: IntGauge,
    /// occupancy of the channel from the `primary::Proposer` to the `primary::Core`
    pub tx_headers: IntGauge,
    /// occupancy of the channel from the `primary::Synchronizer` to the `primary::HeaderWaiter`
    pub tx_sync_headers: IntGauge,
    /// occupancy of the channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`
    pub tx_sync_certificates: IntGauge,
    /// occupancy of the channel from the `primary::HeaderWaiter` to the `primary::Core`
    pub tx_headers_loopback: IntGauge,
    /// occupancy of the channel from the `primary::CertificateWaiter` to the `primary::Core`
    pub tx_certificates_loopback: IntGauge,
    /// occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::Core`
    pub tx_primary_messages: IntGauge,
    /// occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::Helper`
    pub tx_helper_requests: IntGauge,
    /// occupancy of the channel from the `primary::ConsensusAPIGrpc` (when external consensus is being
    /// used) & `executor::Subscriber` (when internal consensus, ex Bullshark, is being used)  to
    /// the `primary::BlockWaiter`.
    pub tx_get_block_commands: IntGauge,
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::BlockWaiter`
    pub tx_batches: IntGauge,
    /// occupancy of the channel from the `primary::ConsensusAPIGrpc` to the `primary::BlockRemover`
    pub tx_block_removal_commands: IntGauge,
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::BlockRemover`
    pub tx_batch_removal: IntGauge,
    /// occupancy of the channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`
    pub tx_block_synchronizer_commands: IntGauge,
    /// occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::BlockSynchronizer`
    pub tx_availability_responses: IntGauge,
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::StateHandler`
    pub tx_state_handler: IntGauge,
    /// occupancy of the channel from the reconfigure notification to most components.
    pub tx_reconfigure: IntGauge,
    /// occupancy of the channel from the `Consensus` to the `primary::Core`
    pub tx_committed_certificates: IntGauge,
    /// occupancy of the channel from the `primary::Core` to the `Consensus`
    pub tx_new_certificates: IntGauge,
}

impl PrimaryChannelMetrics {
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_committed_certificates_metric`.
    pub const NAME_COMMITTED_CERTS: &'static str = "tx_committed_certificates";
    pub const DESC_COMMITTED_CERTS: &'static str =
        "occupancy of the channel from the `Consensus` to the `primary::Core`";
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_new_certificates_metric`.
    pub const NAME_NEW_CERTS: &'static str = "tx_new_certificates";
    pub const DESC_NEW_CERTS: &'static str =
        "occupancy of the channel from the `primary::Core` to the `Consensus`";
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_tx_get_block_commands_metric`.
    pub const NAME_GET_BLOCK_COMMANDS: &'static str = "tx_get_block_commands";
    pub const DESC_GET_BLOCK_COMMANDS: &'static str =
        "occupancy of the channel from the `primary::ConsensusAPIGrpc` & `executor::Subscriber` to the `primary::BlockWaiter`";

    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_others_digests: register_int_gauge_with_registry!(
                "tx_others_digests",
                "occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::PayloadReceiver`",
                registry
            ).unwrap(),
            tx_our_digests: register_int_gauge_with_registry!(
                "tx_our_digests",
                "occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::Proposer`",
                registry
            ).unwrap(),
            tx_parents: register_int_gauge_with_registry!(
                "tx_parents",
                "occupancy of the channel from the `primary::Core` to the `primary::Proposer`",
                registry
            ).unwrap(),
            tx_headers: register_int_gauge_with_registry!(
                "tx_headers",
                "occupancy of the channel from the `primary::Proposer` to the `primary::Core`",
                registry
            ).unwrap(),
            tx_sync_headers: register_int_gauge_with_registry!(
                "tx_sync_headers",
                "occupancy of the channel from the `primary::Synchronizer` to the `primary::HeaderWaiter`",
                registry
            ).unwrap(),
            tx_sync_certificates: register_int_gauge_with_registry!(
                "tx_sync_certificates",
                "occupancy of the channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`",
                registry
            ).unwrap(),
            tx_headers_loopback: register_int_gauge_with_registry!(
                "tx_headers_loopback",
                "occupancy of the channel from the `primary::HeaderWaiter` to the `primary::Core`",
                registry
            ).unwrap(),
            tx_certificates_loopback: register_int_gauge_with_registry!(
                "tx_certificates_loopback",
                "occupancy of the channel from the `primary::CertificateWaiter` to the `primary::Core`",
                registry
            ).unwrap(),
            tx_primary_messages: register_int_gauge_with_registry!(
                "tx_primary_messages",
                "occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::Core`",
                registry
            ).unwrap(),
            tx_helper_requests: register_int_gauge_with_registry!(
                "tx_helper_requests",
                "occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::Helper`",
                registry
            ).unwrap(),
            tx_get_block_commands: register_int_gauge_with_registry!(
                "tx_get_block_commands",
                "occupancy of the channel from the `primary::ConsensusAPIGrpc` & `executor::Subscriber` to the `primary::BlockWaiter`",
                registry
            ).unwrap(),
            tx_batches: register_int_gauge_with_registry!(
                "tx_batches",
                "occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::BlockWaiter`",
                registry
            ).unwrap(),
            tx_block_removal_commands: register_int_gauge_with_registry!(
                "tx_block_removal_commands",
                "occupancy of the channel from the `primary::ConsensusAPIGrpc` to the `primary::BlockRemover`",
                registry
            ).unwrap(),
            tx_batch_removal: register_int_gauge_with_registry!(
                "tx_batch_removal",
                "occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::BlockRemover`",
                registry
            ).unwrap(),
            tx_block_synchronizer_commands: register_int_gauge_with_registry!(
                "tx_block_synchronizer_commands",
                "occupancy of the channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`",
                registry
            ).unwrap(),
            tx_availability_responses: register_int_gauge_with_registry!(
                "tx_availability_responses",
                "occupancy of the channel from the `primary::PrimaryReceiverHandler` to the `primary::BlockSynchronizer`",
                registry
            ).unwrap(),
            tx_state_handler: register_int_gauge_with_registry!(
                "tx_state_handler",
                "occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::StateHandler`",
                registry
            ).unwrap(),
            tx_reconfigure: register_int_gauge_with_registry!(
                "tx_reconfigure",
                "occupancy of the channel from the reconfigure notification to most components.",
                registry
            ).unwrap(),
            tx_committed_certificates: register_int_gauge_with_registry!(
                Self::NAME_COMMITTED_CERTS,
                Self::DESC_COMMITTED_CERTS,
                registry
            ).unwrap(),
            tx_new_certificates: register_int_gauge_with_registry!(
                Self::NAME_NEW_CERTS,
                Self::DESC_NEW_CERTS,
                registry
            ).unwrap(),
        }
    }

    pub fn replace_registered_new_certificates_metric(
        &mut self,
        registry: &Registry,
        collector: Box<GenericGauge<AtomicI64>>,
    ) {
        let new_certificates_counter =
            IntGauge::new(Self::NAME_NEW_CERTS, Self::DESC_NEW_CERTS).unwrap();
        // TODO: Sanity-check by hashing the descs against one another
        registry
            .unregister(Box::new(new_certificates_counter.clone()))
            .unwrap();
        registry.register(collector).unwrap();
        self.tx_new_certificates = new_certificates_counter;
    }

    pub fn replace_registered_committed_certificates_metric(
        &mut self,
        registry: &Registry,
        collector: Box<GenericGauge<AtomicI64>>,
    ) {
        let committed_certificates_counter =
            IntGauge::new(Self::NAME_COMMITTED_CERTS, Self::DESC_COMMITTED_CERTS).unwrap();
        // TODO: Sanity-check by hashing the descs against one another
        registry
            .unregister(Box::new(committed_certificates_counter.clone()))
            .unwrap();
        registry.register(collector).unwrap();
        self.tx_committed_certificates = committed_certificates_counter;
    }

    pub fn replace_registered_get_block_commands_metric(
        &mut self,
        registry: &Registry,
        collector: Box<GenericGauge<AtomicI64>>,
    ) {
        let tx_get_block_commands_counter =
            IntGauge::new(Self::NAME_GET_BLOCK_COMMANDS, Self::DESC_GET_BLOCK_COMMANDS).unwrap();
        // TODO: Sanity-check by hashing the descs against one another
        registry
            .unregister(Box::new(tx_get_block_commands_counter.clone()))
            .unwrap();
        registry.register(collector).unwrap();
        self.tx_get_block_commands = tx_get_block_commands_counter;
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
    /// Number of elements in the waiting (ready-to-deliver) list of header_waiter
    pub waiting_elements_header_waiter: IntGaugeVec,
    /// Number of elements in pending list of certificate_waiter
    pub pending_elements_certificate_waiter: IntGaugeVec,
    /// Number of elements in the waiting (ready-to-deliver) list of certificate_waiter
    pub waiting_elements_certificate_waiter: IntGaugeVec,
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
            waiting_elements_header_waiter: register_int_gauge_vec_with_registry!(
                "waiting_elements_header_waiter",
                "Number of waiting elements in header waiter",
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
            waiting_elements_certificate_waiter: register_int_gauge_vec_with_registry!(
                "waiting_elements_certificate_waiter",
                "Number of waiting elements in certificate waiter",
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

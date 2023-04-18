// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::EndpointMetrics;
use mysten_network::metrics::MetricsCallbackProvider;
use network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use prometheus::{
    core::{AtomicI64, GenericGauge},
    default_registry, linear_buckets, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Registry,
};
use std::time::Duration;
use tonic::Code;

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4,
    1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.,
    12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

#[derive(Clone)]
pub(crate) struct Metrics {
    pub(crate) endpoint_metrics: Option<EndpointMetrics>,
    pub(crate) inbound_network_metrics: Option<NetworkMetrics>,
    pub(crate) outbound_network_metrics: Option<NetworkMetrics>,
    pub(crate) primary_channel_metrics: Option<PrimaryChannelMetrics>,
    pub(crate) node_metrics: Option<PrimaryMetrics>,
    pub(crate) network_connection_metrics: Option<NetworkConnectionMetrics>,
}

/// Initialises the metrics
pub(crate) fn initialise_metrics(metrics_registry: &Registry) -> Metrics {
    // The metrics used for the gRPC primary node endpoints we expose to the external consensus
    let endpoint_metrics = EndpointMetrics::new(metrics_registry);

    // The metrics used for communicating over the network
    let inbound_network_metrics = NetworkMetrics::new("primary", "inbound", metrics_registry);
    let outbound_network_metrics = NetworkMetrics::new("primary", "outbound", metrics_registry);

    // The metrics used for measuring the occupancy of the channels in the primary
    let primary_channel_metrics = PrimaryChannelMetrics::new(metrics_registry);

    // Essential/core metrics across the primary node
    let node_metrics = PrimaryMetrics::new(metrics_registry);

    // Network metrics for the primary connection
    let network_connection_metrics = NetworkConnectionMetrics::new("primary", metrics_registry);

    Metrics {
        node_metrics: Some(node_metrics),
        endpoint_metrics: Some(endpoint_metrics),
        primary_channel_metrics: Some(primary_channel_metrics),
        inbound_network_metrics: Some(inbound_network_metrics),
        outbound_network_metrics: Some(outbound_network_metrics),
        network_connection_metrics: Some(network_connection_metrics),
    }
}

#[derive(Clone)]
pub struct PrimaryChannelMetrics {
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::PayloadReceiver`
    pub tx_others_digests: IntGauge,
    /// occupancy of the channel from the `primary::WorkerReceiverHandler` to the `primary::Proposer`
    pub tx_our_digests: IntGauge,
    /// occupancy of the channel from the `primary::Synchronizer` to the `primary::Proposer`
    pub tx_parents: IntGauge,
    /// occupancy of the channel from the `primary::Proposer` to the `primary::Certifier`
    pub tx_headers: IntGauge,
    /// occupancy of the channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`
    pub tx_certificate_fetcher: IntGauge,
    /// occupancy of the channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`
    pub tx_block_synchronizer_commands: IntGauge,
    /// occupancy of the channel from the `Consensus` to the `primary::StateHandler`
    pub tx_committed_certificates: IntGauge,
    /// occupancy of the channel from the `primary::Synchronizer` to the `Consensus`
    pub tx_new_certificates: IntGauge,
    /// occupancy of the channel signaling own committed headers
    pub tx_committed_own_headers: IntGauge,

    // totals
    /// total received on channel from the `primary::WorkerReceiverHandler` to the `primary::PayloadReceiver`
    pub tx_others_digests_total: IntCounter,
    /// total received on channel from the `primary::WorkerReceiverHandler` to the `primary::Proposer`
    pub tx_our_digests_total: IntCounter,
    /// total received on channel from the `primary::Synchronizer` to the `primary::Proposer`
    pub tx_parents_total: IntCounter,
    /// total received on channel from the `primary::Proposer` to the `primary::Certifier`
    pub tx_headers_total: IntCounter,
    /// total received on channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`
    pub tx_certificate_fetcher_total: IntCounter,
    /// total received on channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`
    pub tx_block_synchronizer_commands_total: IntCounter,
    /// total received on channel from the `primary::WorkerReceiverHandler` to the `primary::StateHandler`
    pub tx_state_handler_total: IntCounter,
    /// total received on channel from the `Consensus` to the `primary::StateHandler`
    pub tx_committed_certificates_total: IntCounter,
    /// total received on channel from the `primary::Synchronizer` to the `Consensus`
    pub tx_new_certificates_total: IntCounter,
    /// total received on the channel signaling own committed headers
    pub tx_committed_own_headers_total: IntCounter,
}

impl PrimaryChannelMetrics {
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_committed_certificates_metric`.
    pub const NAME_COMMITTED_CERTS: &'static str = "tx_committed_certificates";
    pub const DESC_COMMITTED_CERTS: &'static str =
        "occupancy of the channel from the `Consensus` to the `primary::StateHandler`";
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_new_certificates_metric`.
    pub const NAME_NEW_CERTS: &'static str = "tx_new_certificates";
    pub const DESC_NEW_CERTS: &'static str =
        "occupancy of the channel from the `primary::Synchronizer` to the `Consensus`";

    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_committed_certificates_metric`.
    pub const NAME_COMMITTED_CERTS_TOTAL: &'static str = "tx_committed_certificates_total";
    pub const DESC_COMMITTED_CERTS_TOTAL: &'static str =
        "total received on channel from the `Consensus` to the `primary::StateHandler`";
    // The consistent use of this constant in the below, as well as in `node::spawn_primary` is
    // load-bearing, see `replace_registered_new_certificates_metric`.
    pub const NAME_NEW_CERTS_TOTAL: &'static str = "tx_new_certificates_total";
    pub const DESC_NEW_CERTS_TOTAL: &'static str =
        "total received on channel from the `primary::Synchronizer` to the `Consensus`";

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
                "occupancy of the channel from the `primary::Synchronizer` to the `primary::Proposer`",
                registry
            ).unwrap(),
            tx_headers: register_int_gauge_with_registry!(
                "tx_headers",
                "occupancy of the channel from the `primary::Proposer` to the `primary::Certifier`",
                registry
            ).unwrap(),
            tx_certificate_fetcher: register_int_gauge_with_registry!(
                "tx_certificate_fetcher",
                "occupancy of the channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`",
                registry
            ).unwrap(),
            tx_block_synchronizer_commands: register_int_gauge_with_registry!(
                "tx_block_synchronizer_commands",
                "occupancy of the channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`",
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
            tx_committed_own_headers: register_int_gauge_with_registry!(
                "tx_committed_own_headers",
                "occupancy of the channel signaling own committed headers.",
                registry
            ).unwrap(),

            // totals
            tx_others_digests_total: register_int_counter_with_registry!(
                "tx_others_digests_total",
                "total received on channel from the `primary::WorkerReceiverHandler` to the `primary::PayloadReceiver`",
                registry
            ).unwrap(),
            tx_our_digests_total: register_int_counter_with_registry!(
                "tx_our_digests_total",
                "total received on channel from the `primary::WorkerReceiverHandler` to the `primary::Proposer`",
                registry
            ).unwrap(),
            tx_parents_total: register_int_counter_with_registry!(
                "tx_parents_total",
                "total received on channel from the `primary::Synchronizer` to the `primary::Proposer`",
                registry
            ).unwrap(),
            tx_headers_total: register_int_counter_with_registry!(
                "tx_headers_total",
                "total received on channel from the `primary::Proposer` to the `primary::Certifier`",
                registry
            ).unwrap(),
            tx_certificate_fetcher_total: register_int_counter_with_registry!(
                "tx_certificate_fetcher_total",
                "total received on channel from the `primary::Synchronizer` to the `primary::CertificaterWaiter`",
                registry
            ).unwrap(),
            tx_block_synchronizer_commands_total: register_int_counter_with_registry!(
                "tx_block_synchronizer_commands_total",
                "total received on channel from the `primary::BlockSynchronizerHandler` to the `primary::BlockSynchronizer`",
                registry
            ).unwrap(),
            tx_state_handler_total: register_int_counter_with_registry!(
                "tx_state_handler_total",
                "total received on channel from the `primary::WorkerReceiverHandler` to the `primary::StateHandler`",
                registry
            ).unwrap(),
            tx_committed_certificates_total: register_int_counter_with_registry!(
                Self::NAME_COMMITTED_CERTS_TOTAL,
                Self::DESC_COMMITTED_CERTS_TOTAL,
                registry
            ).unwrap(),
            tx_new_certificates_total: register_int_counter_with_registry!(
                Self::NAME_NEW_CERTS_TOTAL,
                Self::DESC_NEW_CERTS_TOTAL,
                registry
            ).unwrap(),
            tx_committed_own_headers_total: register_int_counter_with_registry!(
                "tx_committed_own_headers_total",
                "total received on channel signaling own committed headers.",
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
}

#[derive(Clone)]
pub struct PrimaryMetrics {
    /// count number of headers that the node proposed
    pub headers_proposed: IntCounterVec,
    // total number of parents in all proposed headers, for calculating average number of parents
    // per header.
    pub header_parents: Histogram,
    /// the current proposed header round
    pub proposed_header_round: IntGauge,
    /// The number of received votes for the proposed last round
    pub votes_received_last_round: IntGauge,
    // total number of parent certificates included in votes.
    pub certificates_in_votes: IntCounter,
    /// The round of the latest created certificate by our node
    pub certificate_created_round: IntGauge,
    /// count number of certificates that the node created
    pub certificates_created: IntCounter,
    /// count number of certificates that the node processed (others + own)
    pub certificates_processed: IntCounterVec,
    /// count number of certificates that the node suspended their processing
    pub certificates_suspended: IntCounterVec,
    /// number of certificates that are currently suspended.
    pub certificates_currently_suspended: IntGauge,
    /// count number of duplicate certificates that the node processed (others + own)
    pub duplicate_certificates_processed: IntCounter,
    /// The current Narwhal round in proposer
    pub current_round: IntGauge,
    /// Latency distribution for generating proposals
    pub proposal_latency: HistogramVec,
    /// The highest Narwhal round of certificates that have been accepted.
    pub highest_processed_round: IntGaugeVec,
    /// The highest Narwhal round that has been received.
    pub highest_received_round: IntGaugeVec,
    /// 0 if there is no inflight certificates fetching, 1 otherwise.
    pub certificate_fetcher_inflight_fetch: IntGauge,
    /// Number of fetched certificates successfully processed by core.
    pub certificate_fetcher_num_certificates_processed: IntCounter,
    /// Total time spent in certificate verifications, in microseconds.
    pub certificate_fetcher_total_verification_us: IntCounter,
    /// Total time spent to accept certificates via Synchronizer, in microseconds.
    pub certificate_fetcher_total_accept_us: IntCounter,
    /// Number of votes that were requested but not sent due to previously having voted differently
    pub votes_dropped_equivocation_protection: IntCounter,
    /// Number of pending batches in proposer
    pub num_of_pending_batches_in_proposer: IntGauge,
    /// A histogram to track the number of batches included
    /// per header.
    pub num_of_batch_digests_in_header: HistogramVec,
    /// A counter that keeps the number of instances where the proposer
    /// is ready/not ready to advance.
    pub proposer_ready_to_advance: IntCounterVec,
    /// The latency of a batch between the time it has been
    /// created and until it has been included to a header proposal.
    pub proposer_batch_latency: Histogram,
    /// The number of headers being resent because they will not get committed.
    pub proposer_resend_headers: IntCounter,
    /// The number of batches being resent because they will not get committed.
    pub proposer_resend_batches: IntCounter,
    /// Time it takes for a header to be materialised to a certificate
    pub header_to_certificate_latency: Histogram,
    /// Millisecs taken to wait for max parent time, when proposing headers.
    pub header_max_parent_wait_ms: IntCounter,
}

impl PrimaryMetrics {
    pub fn new(registry: &Registry) -> Self {
        let parents_buckets = [
            linear_buckets(1.0, 1.0, 20).unwrap().as_slice(),
            linear_buckets(21.0, 2.0, 20).unwrap().as_slice(),
            linear_buckets(61.0, 3.0, 20).unwrap().as_slice(),
        ]
        .concat();
        Self {
            headers_proposed: register_int_counter_vec_with_registry!(
                "headers_proposed",
                "Number of headers that node proposed",
                &["leader_support"],
                registry
            )
            .unwrap(),
            header_parents: register_histogram_with_registry!(
                "header_parents",
                "Number of parents included in proposed headers",
                parents_buckets,
                registry
            )
            .unwrap(),
            proposed_header_round: register_int_gauge_with_registry!(
                "proposed_header_round",
                "The current proposed header round",
                registry
            ).unwrap(),
            votes_received_last_round: register_int_gauge_with_registry!(
                "votes_received_last_round",
                "The number of received votes for the proposed last round",
                registry
            ).unwrap(),
            certificates_in_votes: register_int_counter_with_registry!(
                "certificates_in_votes",
                "Total number of parent certificates included in votes.",
                registry
            ).unwrap(),
            certificate_created_round: register_int_gauge_with_registry!(
                "certificate_created_round",
                "The round of the latest created certificate by our node",
                registry
            ).unwrap(),
            certificates_created: register_int_counter_with_registry!(
                "certificates_created",
                "Number of certificates that node created",
                registry
            )
            .unwrap(),
            certificates_processed: register_int_counter_vec_with_registry!(
                "certificates_processed",
                "Number of certificates that node processed (others + own)",
                &["source"],
                registry
            )
            .unwrap(),
            certificates_suspended: register_int_counter_vec_with_registry!(
                "certificates_suspended",
                "Number of certificates that node suspended processing of",
                &["reason"],
                registry
            )
            .unwrap(),
            certificates_currently_suspended: register_int_gauge_with_registry!(
                "certificates_currently_suspended",
                "Number of certificates that are suspended in memory",
                registry
            )
            .unwrap(),
            duplicate_certificates_processed: register_int_counter_with_registry!(
                "duplicate_certificates_processed",
                "Number of certificates that node processed (others + own)",
                registry
            )
            .unwrap(),
            current_round: register_int_gauge_with_registry!(
                "current_round",
                "Current round the node will propose",
                registry
            )
            .unwrap(),
            proposal_latency: register_histogram_vec_with_registry!(
                "proposal_latency",
                "Time distribution between node proposals",
                &["reason"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            highest_received_round: register_int_gauge_vec_with_registry!(
                "highest_received_round",
                "Highest round received by the primary",
                &["source"],
                registry
            )
            .unwrap(),
            highest_processed_round: register_int_gauge_vec_with_registry!(
                "highest_processed_round",
                "Highest round processed (stored) by the primary",
                &["source"],
                registry
            )
            .unwrap(),
            certificate_fetcher_inflight_fetch: register_int_gauge_with_registry!(
                "certificate_fetcher_inflight_fetch",
                "0 if there is no inflight certificates fetching, 1 otherwise.",
                registry
            )
            .unwrap(),
            certificate_fetcher_num_certificates_processed: register_int_counter_with_registry!(
                "certificate_fetcher_num_certificates_processed",
                "Number of fetched certificates successfully processed by core.",
                registry
            )
            .unwrap(),
            certificate_fetcher_total_verification_us: register_int_counter_with_registry!(
                "certificate_fetcher_total_verification_us",
                "Total time spent in certificate verifications, in microseconds.",
                registry
            )
            .unwrap(),
            certificate_fetcher_total_accept_us: register_int_counter_with_registry!(
                "certificate_fetcher_total_accept_us",
                "Total time spent to accept certificates via Synchronizer, in microseconds.",
                registry
            )
            .unwrap(),
            votes_dropped_equivocation_protection: register_int_counter_with_registry!(
                "votes_dropped_equivocation_protection",
                "Number of votes that were requested but not sent due to previously having voted differently",
                registry
            )
            .unwrap(),
            num_of_pending_batches_in_proposer: register_int_gauge_with_registry!(
                "num_of_pending_batches_in_proposer",
                "Number of batch digests pending in proposer for next header proposal",
                registry
            ).unwrap(),
            num_of_batch_digests_in_header: register_histogram_vec_with_registry!(
                "num_of_batch_digests_in_header",
                "The number of batch digests included in a proposed header. A reason label is included.",
                &["reason"],
                // buckets in number of digests
                vec![0.0, 5.0, 10.0, 15.0, 32.0, 50.0, 100.0, 200.0, 500.0, 1000.0],
                registry
            ).unwrap(),
            proposer_ready_to_advance: register_int_counter_vec_with_registry!(
                "proposer_ready_to_advance",
                "The number of times where the proposer is ready/not ready to advance.",
                &["ready", "round"],
                registry
            ).unwrap(),
            proposer_batch_latency: register_histogram_with_registry!(
                "proposer_batch_latency",
                "The latency of a batch between the time it has been created and until it has been included to a header proposal.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            proposer_resend_headers: register_int_counter_with_registry!(
                "proposer_resend_headers",
                "The number of headers being resent because they will not get committed.",
                registry
            ).unwrap(),
            proposer_resend_batches: register_int_counter_with_registry!(
                "proposer_resend_batches",
                "The number of batches being resent because they will not get committed.",
                registry
            ).unwrap(),
            header_to_certificate_latency: register_histogram_with_registry!(
                "header_to_certificate_latency",
                "Time it takes for a header to be materialised to a certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            header_max_parent_wait_ms: register_int_counter_with_registry!(
                "header_max_parent_wait_ms",
                "Millisecs taken to wait for max parent time, when proposing headers.",
                registry
            ).unwrap(),
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

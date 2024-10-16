// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, HistogramTimer,
    HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Registry,
};
use tracing::warn;

// Fields for network-agnostic metrics can be added here
pub(crate) struct NetworkMetrics {
    pub(crate) network_type: IntGaugeVec,
    pub(crate) inbound: Arc<NetworkRouteMetrics>,
    pub(crate) outbound: Arc<NetworkRouteMetrics>,
    #[cfg_attr(msim, allow(dead_code))]
    pub(crate) tcp_connection_metrics: Arc<TcpConnectionMetrics>,
    pub(crate) quinn_connection_metrics: Arc<QuinnConnectionMetrics>,
}

impl NetworkMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            network_type: register_int_gauge_vec_with_registry!(
                "network_type",
                "Type of the network used: anemo or tonic",
                &["type"],
                registry
            )
            .unwrap(),
            inbound: Arc::new(NetworkRouteMetrics::new("", "inbound", registry)),
            outbound: Arc::new(NetworkRouteMetrics::new("", "outbound", registry)),
            tcp_connection_metrics: Arc::new(TcpConnectionMetrics::new(registry)),
            quinn_connection_metrics: Arc::new(QuinnConnectionMetrics::new("", registry)),
        }
    }
}

#[cfg_attr(msim, allow(dead_code))]
pub(crate) struct TcpConnectionMetrics {
    /// Send buffer size of consensus TCP socket.
    pub(crate) socket_send_buffer_size: IntGauge,
    /// Receive buffer size of consensus TCP socket.
    pub(crate) socket_recv_buffer_size: IntGauge,
    /// Max send buffer size of TCP socket.
    pub(crate) socket_send_buffer_max_size: IntGauge,
    /// Max receive buffer size of TCP socket.
    pub(crate) socket_recv_buffer_max_size: IntGauge,
}

impl TcpConnectionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            socket_send_buffer_size: register_int_gauge_with_registry!(
                "tcp_socket_send_buffer_size",
                "Send buffer size of consensus TCP socket.",
                registry
            )
            .unwrap(),
            socket_recv_buffer_size: register_int_gauge_with_registry!(
                "tcp_socket_recv_buffer_size",
                "Receive buffer size of consensus TCP socket.",
                registry
            )
            .unwrap(),
            socket_send_buffer_max_size: register_int_gauge_with_registry!(
                "tcp_socket_send_buffer_max_size",
                "Max send buffer size of TCP socket.",
                registry
            )
            .unwrap(),
            socket_recv_buffer_max_size: register_int_gauge_with_registry!(
                "tcp_socket_recv_buffer_max_size",
                "Max receive buffer size of TCP socket.",
                registry
            )
            .unwrap(),
        }
    }
}

pub struct QuinnConnectionMetrics {
    /// The connection status of known peers. 0 if not connected, 1 if connected.
    pub network_peer_connected: IntGaugeVec,
    /// The number of connected peers
    pub network_peers: IntGauge,
    /// Number of disconnect events per peer.
    pub network_peer_disconnects: IntCounterVec,
    /// Receive buffer size of Anemo socket.
    pub socket_receive_buffer_size: IntGauge,
    /// Send buffer size of Anemo socket.
    pub socket_send_buffer_size: IntGauge,

    /// PathStats
    /// The rtt for a peer connection in ms.
    pub network_peer_rtt: IntGaugeVec,
    /// The total number of lost packets for a peer connection.
    pub network_peer_lost_packets: IntGaugeVec,
    /// The total number of lost bytes for a peer connection.
    pub network_peer_lost_bytes: IntGaugeVec,
    /// The total number of packets sent for a peer connection.
    pub network_peer_sent_packets: IntGaugeVec,
    /// The total number of congestion events for a peer connection.
    pub network_peer_congestion_events: IntGaugeVec,
    /// The congestion window for a peer connection.
    pub network_peer_congestion_window: IntGaugeVec,

    /// FrameStats
    /// The number of max data frames for a peer connection.
    pub network_peer_max_data: IntGaugeVec,
    /// The number of closed connections frames for a peer connection.
    pub network_peer_closed_connections: IntGaugeVec,
    /// The number of data blocked frames for a peer connection.
    pub network_peer_data_blocked: IntGaugeVec,

    /// UDPStats
    /// The total number datagrams observed by the UDP peer connection.
    pub network_peer_udp_datagrams: IntGaugeVec,
    /// The total number bytes observed by the UDP peer connection.
    pub network_peer_udp_bytes: IntGaugeVec,
    /// The total number transmits observed by the UDP peer connection.
    pub network_peer_udp_transmits: IntGaugeVec,
}

impl QuinnConnectionMetrics {
    pub fn new(node: &'static str, registry: &Registry) -> Self {
        Self {
            network_peer_connected: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_connected"),
                "The connection status of a peer. 0 if not connected, 1 if connected",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peers: register_int_gauge_with_registry!(
                format!("{node}_quinn_network_peers"),
                "The number of connected peers.",
                registry
            )
            .unwrap(),
            network_peer_disconnects: register_int_counter_vec_with_registry!(
                format!("{node}_quinn_network_peer_disconnects"),
                "Number of disconnect events per peer.",
                &["peer_id", "peer_label", "reason"],
                registry
            )
            .unwrap(),
            socket_receive_buffer_size: register_int_gauge_with_registry!(
                format!("{node}_quinn_socket_receive_buffer_size"),
                "Receive buffer size of Anemo socket.",
                registry
            )
            .unwrap(),
            socket_send_buffer_size: register_int_gauge_with_registry!(
                format!("{node}_quinn_socket_send_buffer_size"),
                "Send buffer size of Anemo socket.",
                registry
            )
            .unwrap(),

            // PathStats
            network_peer_rtt: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_rtt"),
                "The rtt for a peer connection in ms.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peer_lost_packets: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_lost_packets"),
                "The total number of lost packets for a peer connection.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peer_lost_bytes: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_lost_bytes"),
                "The total number of lost bytes for a peer connection.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peer_sent_packets: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_sent_packets"),
                "The total number of sent packets for a peer connection.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peer_congestion_events: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_congestion_events"),
                "The total number of congestion events for a peer connection.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),
            network_peer_congestion_window: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_congestion_window"),
                "The congestion window for a peer connection.",
                &["peer_id", "peer_label"],
                registry
            )
            .unwrap(),

            // FrameStats
            network_peer_closed_connections: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_closed_connections"),
                "The number of closed connections for a peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),
            network_peer_max_data: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_max_data"),
                "The number of max data frames for a peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),
            network_peer_data_blocked: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_data_blocked"),
                "The number of data blocked frames for a peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),

            // UDPStats
            network_peer_udp_datagrams: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_udp_datagrams"),
                "The total number datagrams observed by the UDP peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),
            network_peer_udp_bytes: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_udp_bytes"),
                "The total number bytes observed by the UDP peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),
            network_peer_udp_transmits: register_int_gauge_vec_with_registry!(
                format!("{node}_quinn_network_peer_udp_transmits"),
                "The total number transmits observed by the UDP peer connection.",
                &["peer_id", "peer_label", "direction"],
                registry
            )
            .unwrap(),
        }
    }
}

#[derive(Clone)]
pub struct NetworkRouteMetrics {
    /// Counter of requests by route
    pub requests: IntCounterVec,
    /// Request latency by route
    pub request_latency: HistogramVec,
    /// Request size by route
    pub request_size: HistogramVec,
    /// Response size by route
    pub response_size: HistogramVec,
    /// Counter of requests exceeding the "excessive" size limit
    pub excessive_size_requests: IntCounterVec,
    /// Counter of responses exceeding the "excessive" size limit
    pub excessive_size_responses: IntCounterVec,
    /// Gauge of the number of inflight requests at any given time by route
    pub inflight_requests: IntGaugeVec,
    /// Failed requests by route
    pub errors: IntCounterVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

// Arbitrarily chosen buckets for message size, with gradually-lowering exponent to give us
// better resolution at high sizes.
const SIZE_BYTE_BUCKETS: &[f64] = &[
    2048., 8192., // *4
    16384., 32768., 65536., 131072., 262144., 524288., 1048576., // *2
    1572864., 2359256., 3538944., // *1.5
    4600627., 5980815., 7775060., 10107578., 13139851., 17081807., 22206349., 28868253., 37528729.,
    48787348., 63423553., // *1.3
];

impl NetworkRouteMetrics {
    pub fn new(node: &'static str, direction: &'static str, registry: &Registry) -> Self {
        let requests = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_requests"),
            "The number of requests made on the network",
            &["route"],
            registry
        )
        .unwrap();

        let request_latency = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_request_latency"),
            "Latency of a request by route",
            &["route"],
            LATENCY_SEC_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let request_size = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_request_size"),
            "Size of a request by route",
            &["route"],
            SIZE_BYTE_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let response_size = register_histogram_vec_with_registry!(
            format!("{node}_{direction}_response_size"),
            "Size of a response by route",
            &["route"],
            SIZE_BYTE_BUCKETS.to_vec(),
            registry,
        )
        .unwrap();

        let excessive_size_requests = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_excessive_size_requests"),
            "The number of excessively large request messages sent",
            &["route"],
            registry
        )
        .unwrap();

        let excessive_size_responses = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_excessive_size_responses"),
            "The number of excessively large response messages seen",
            &["route"],
            registry
        )
        .unwrap();

        let inflight_requests = register_int_gauge_vec_with_registry!(
            format!("{node}_{direction}_inflight_requests"),
            "The number of inflight network requests",
            &["route"],
            registry
        )
        .unwrap();

        let errors = register_int_counter_vec_with_registry!(
            format!("{node}_{direction}_request_errors"),
            "Number of errors by route",
            &["route", "status"],
            registry,
        )
        .unwrap();

        Self {
            requests,
            request_latency,
            request_size,
            response_size,
            excessive_size_requests,
            excessive_size_responses,
            inflight_requests,
            errors,
        }
    }
}

#[derive(Clone)]
pub struct MetricsMakeCallbackHandler {
    metrics: Arc<NetworkRouteMetrics>,
    /// Size in bytes above which a request or response message is considered excessively large
    excessive_message_size: usize,
}

impl MetricsMakeCallbackHandler {
    pub fn new(metrics: Arc<NetworkRouteMetrics>, excessive_message_size: usize) -> Self {
        Self {
            metrics,
            excessive_message_size,
        }
    }
}

impl MakeCallbackHandler for MetricsMakeCallbackHandler {
    type Handler = MetricsResponseHandler;

    fn make_handler(&self, request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        let route = request.route().to_owned();

        self.metrics.requests.with_label_values(&[&route]).inc();
        self.metrics
            .inflight_requests
            .with_label_values(&[&route])
            .inc();
        let body_len = request.body().len();
        self.metrics
            .request_size
            .with_label_values(&[&route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large request with size {body_len} for {route} with peer {:?}",
                request.peer_id()
            );
            self.metrics
                .excessive_size_requests
                .with_label_values(&[&route])
                .inc();
        }

        let timer = self
            .metrics
            .request_latency
            .with_label_values(&[&route])
            .start_timer();

        MetricsResponseHandler {
            metrics: self.metrics.clone(),
            timer,
            route,
            excessive_message_size: self.excessive_message_size,
        }
    }
}

pub struct MetricsResponseHandler {
    metrics: Arc<NetworkRouteMetrics>,
    // The timer is held on to and "observed" once dropped
    #[allow(unused)]
    timer: HistogramTimer,
    route: String,
    excessive_message_size: usize,
}

impl ResponseHandler for MetricsResponseHandler {
    fn on_response(self, response: &anemo::Response<bytes::Bytes>) {
        let body_len = response.body().len();
        self.metrics
            .response_size
            .with_label_values(&[&self.route])
            .observe(body_len as f64);
        if body_len > self.excessive_message_size {
            warn!(
                "Saw excessively large response with size {body_len} for {} with peer {:?}",
                self.route,
                response.peer_id()
            );
            self.metrics
                .excessive_size_responses
                .with_label_values(&[&self.route])
                .inc();
        }

        if !response.status().is_success() {
            let status = response.status().to_u16().to_string();
            self.metrics
                .errors
                .with_label_values(&[&self.route, &status])
                .inc();
        }
    }

    fn on_error<E>(self, _error: &E) {
        self.metrics
            .errors
            .with_label_values(&[&self.route, "unknown"])
            .inc();
    }
}

impl Drop for MetricsResponseHandler {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[&self.route])
            .dec();
    }
}

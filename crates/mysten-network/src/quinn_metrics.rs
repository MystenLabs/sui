// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    IntCounterVec, IntGauge, IntGaugeVec, Registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};

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

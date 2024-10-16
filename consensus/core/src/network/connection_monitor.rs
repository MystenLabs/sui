// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use anemo::{types::PeerEvent, PeerId};
use dashmap::DashMap;
use mysten_metrics::spawn_logged_monitored_task;
use quinn_proto::ConnectionStats;
use tokio::{
    sync::oneshot::{Receiver, Sender},
    task::JoinHandle,
    time,
};

use super::metrics::QuinnConnectionMetrics;

const CONNECTION_STAT_COLLECTION_INTERVAL: Duration = Duration::from_secs(60);

pub struct ConnectionMonitorHandle {
    handle: JoinHandle<()>,
    stop: Sender<()>,
    connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
}

impl ConnectionMonitorHandle {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.handle.await.ok();
    }

    pub fn connection_statuses(&self) -> Arc<DashMap<PeerId, ConnectionStatus>> {
        self.connection_statuses.clone()
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

pub struct AnemoConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: Arc<QuinnConnectionMetrics>,
    known_peers: HashMap<PeerId, String>,
    connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
    stop: Receiver<()>,
}

impl AnemoConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: Arc<QuinnConnectionMetrics>,
        known_peers: HashMap<PeerId, String>,
    ) -> ConnectionMonitorHandle {
        let connection_statuses_outer = Arc::new(DashMap::new());
        let connection_statuses = connection_statuses_outer.clone();
        let (stop_sender, stop) = tokio::sync::oneshot::channel();
        let handle = spawn_logged_monitored_task!(
            Self {
                network,
                connection_metrics,
                known_peers,
                connection_statuses,
                stop
            }
            .run(),
            "AnemoConnectionMonitor"
        );

        ConnectionMonitorHandle {
            handle,
            stop: stop_sender,
            connection_statuses: connection_statuses_outer,
        }
    }

    async fn run(mut self) {
        let (mut subscriber, connected_peers) = {
            if let Some(network) = self.network.upgrade() {
                let Ok((subscriber, active_peers)) = network.subscribe() else {
                    return;
                };
                (subscriber, active_peers)
            } else {
                return;
            }
        };

        // we report first all the known peers as disconnected - so we can see
        // their labels in the metrics reporting tool
        for (peer_id, peer_label) in &self.known_peers {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), peer_label])
                .set(0)
        }

        // now report the connected peers
        for peer_id in connected_peers.iter() {
            self.handle_peer_event(PeerEvent::NewPeer(*peer_id)).await;
        }

        let mut connection_stat_collection_interval =
            time::interval(CONNECTION_STAT_COLLECTION_INTERVAL);

        loop {
            tokio::select! {
                _ = connection_stat_collection_interval.tick() => {
                    if let Some(network) = self.network.upgrade() {
                        self.connection_metrics.socket_receive_buffer_size.set(
                            network.socket_receive_buf_size() as i64
                        );
                        self.connection_metrics.socket_send_buffer_size.set(
                            network.socket_send_buf_size() as i64
                        );
                        for (peer_id, peer_label) in &self.known_peers {
                            if let Some(connection) = network.peer(*peer_id) {
                                let stats = connection.connection_stats();
                                self.update_quinn_metrics_for_peer(&format!("{peer_id}"), peer_label, &stats);
                            }
                        }
                    } else {
                        continue;
                    }
                }
                Ok(event) = subscriber.recv() => {
                    self.handle_peer_event(event).await;
                }
                _ = &mut self.stop => {
                    tracing::debug!("Stop signal has been received, now shutting down");
                    return;
                }
            }
        }
    }

    async fn handle_peer_event(&self, peer_event: PeerEvent) {
        if let Some(network) = self.network.upgrade() {
            self.connection_metrics
                .network_peers
                .set(network.peers().len() as i64);
        } else {
            return;
        }

        let (peer_id, status, int_status) = match peer_event {
            PeerEvent::NewPeer(peer_id) => (peer_id, ConnectionStatus::Connected, 1),
            PeerEvent::LostPeer(peer_id, _) => (peer_id, ConnectionStatus::Disconnected, 0),
        };
        self.connection_statuses.insert(peer_id, status);

        // Only report peer IDs for known peers to prevent unlimited cardinality.
        if self.known_peers.contains_key(&peer_id) {
            let peer_id_str = format!("{peer_id}");
            let peer_label = self.known_peers.get(&peer_id).unwrap();

            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&peer_id_str, peer_label])
                .set(int_status);

            if let PeerEvent::LostPeer(_, reason) = peer_event {
                self.connection_metrics
                    .network_peer_disconnects
                    .with_label_values(&[&peer_id_str, peer_label, &format!("{reason:?}")])
                    .inc();
            }
        }
    }

    // TODO: Replace this with ClosureMetric
    fn update_quinn_metrics_for_peer(
        &self,
        peer_id: &str,
        peer_label: &str,
        stats: &ConnectionStats,
    ) {
        // Update PathStats
        self.connection_metrics
            .network_peer_rtt
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.rtt.as_millis() as i64);
        self.connection_metrics
            .network_peer_lost_packets
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.lost_packets as i64);
        self.connection_metrics
            .network_peer_lost_bytes
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.lost_bytes as i64);
        self.connection_metrics
            .network_peer_sent_packets
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.sent_packets as i64);
        self.connection_metrics
            .network_peer_congestion_events
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.congestion_events as i64);
        self.connection_metrics
            .network_peer_congestion_window
            .with_label_values(&[peer_id, peer_label])
            .set(stats.path.cwnd as i64);

        // Update FrameStats
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.frame_tx.max_data as i64);
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.frame_rx.max_data as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.frame_tx.connection_close as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.frame_rx.connection_close as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.frame_tx.data_blocked as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.frame_rx.data_blocked as i64);

        // Update UDPStats
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.udp_tx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.udp_rx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.udp_tx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.udp_rx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[peer_id, peer_label, "transmitted"])
            .set(stats.udp_tx.ios as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[peer_id, peer_label, "received"])
            .set(stats.udp_rx.ios as i64);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, convert::Infallible};

    use anemo::{Network, Request, Response};
    use bytes::Bytes;
    use prometheus::Registry;
    use tokio::time::{sleep, timeout};
    use tower::util::BoxCloneService;

    use super::*;

    #[tokio::test]
    async fn test_connectivity() {
        // GIVEN
        let network_1 = build_network().unwrap();
        let network_2 = build_network().unwrap();
        let network_3 = build_network().unwrap();

        let registry = Registry::new();
        let metrics = Arc::new(QuinnConnectionMetrics::new("consensus", &registry));

        // AND we connect to peer 2
        let peer_2 = network_1.connect(network_2.local_addr()).await.unwrap();

        let mut known_peers = HashMap::new();
        known_peers.insert(network_2.peer_id(), "peer_2".to_string());
        known_peers.insert(network_3.peer_id(), "peer_3".to_string());

        // WHEN bring up the monitor
        let handle =
            AnemoConnectionMonitor::spawn(network_1.downgrade(), metrics.clone(), known_peers);
        let connection_statuses = handle.connection_statuses();

        // THEN peer 2 should be already connected
        assert_network_peers(&metrics, 1).await;

        // AND we should have collected connection stats
        let mut labels = HashMap::new();
        let peer_2_str = format!("{peer_2}");
        labels.insert("peer_id", peer_2_str.as_str());
        labels.insert("peer_label", "peer_2");
        assert_ne!(
            metrics
                .network_peer_rtt
                .get_metric_with(&labels)
                .unwrap()
                .get(),
            0
        );
        assert_eq!(
            *connection_statuses.get(&peer_2).unwrap().value(),
            ConnectionStatus::Connected
        );

        // WHEN connect to peer 3
        let peer_3 = network_1.connect(network_3.local_addr()).await.unwrap();

        // THEN
        assert_network_peers(&metrics, 2).await;
        assert_eq!(
            *connection_statuses.get(&peer_3).unwrap().value(),
            ConnectionStatus::Connected
        );

        // AND disconnect peer 2
        network_1.disconnect(peer_2).unwrap();

        // THEN
        assert_network_peers(&metrics, 1).await;
        assert_eq!(
            *connection_statuses.get(&peer_2).unwrap().value(),
            ConnectionStatus::Disconnected
        );

        // AND disconnect peer 3
        network_1.disconnect(peer_3).unwrap();

        // THEN
        assert_network_peers(&metrics, 0).await;
        assert_eq!(
            *connection_statuses.get(&peer_3).unwrap().value(),
            ConnectionStatus::Disconnected
        );
    }

    async fn assert_network_peers(metrics: &QuinnConnectionMetrics, value: i64) {
        timeout(Duration::from_secs(5), async move {
            while metrics.network_peers.get() != value {
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Timeout while waiting for connectivity results for value {}",
                value
            )
        });

        assert_eq!(metrics.network_peers.get(), value);
    }

    fn build_network() -> anyhow::Result<Network> {
        let network = Network::bind("localhost:0")
            .private_key(random_private_key())
            .server_name("test")
            .start(echo_service())?;
        Ok(network)
    }

    fn echo_service() -> BoxCloneService<Request<Bytes>, Response<Bytes>, Infallible> {
        let handle = move |request: Request<Bytes>| async move {
            let response = Response::new(request.into_body());
            Result::<Response<Bytes>, Infallible>::Ok(response)
        };

        tower::ServiceExt::boxed_clone(tower::service_fn(handle))
    }

    fn random_private_key() -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rng, &mut bytes[..]);

        bytes
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::NetworkConnectionMetrics;
use anemo::types::PeerEvent;
use anemo::PeerId;
use dashmap::DashMap;
use mysten_metrics::spawn_logged_monitored_task;
use quinn_proto::ConnectionStats;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time;

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: NetworkConnectionMetrics,
    peer_id_types: HashMap<PeerId, String>,
    connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: NetworkConnectionMetrics,
        peer_id_types: HashMap<PeerId, String>,
    ) -> (JoinHandle<()>, Arc<DashMap<PeerId, ConnectionStatus>>) {
        let connection_statuses_outer = Arc::new(DashMap::new());
        let connection_statuses = connection_statuses_outer.clone();
        (
            spawn_logged_monitored_task!(
                Self {
                    network,
                    connection_metrics,
                    peer_id_types,
                    connection_statuses,
                }
                .run(),
                "ConnectionMonitor"
            ),
            connection_statuses_outer,
        )
    }

    async fn run(self) {
        let mut connected_peers: HashSet<PeerId> = HashSet::new();
        let mut subscriber = {
            if let Some(network) = self.network.upgrade() {
                let Ok((subscriber, active_peers)) = network.subscribe() else {
                    return;
                };

                active_peers.iter().for_each(|peer_id| {
                    connected_peers.insert(*peer_id);
                });
                subscriber
            } else {
                return;
            }
        };

        // we report first all the known peers as disconnected - so we can see
        // their labels in the metrics reporting tool
        for (peer_id, ty) in &self.peer_id_types {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), ty])
                .set(0)
        }

        // now report the connected peers
        for peer_id in connected_peers.iter() {
            self.handle_peer_status_change(*peer_id, ConnectionStatus::Connected)
                .await;
        }

        let mut connection_stat_collection_interval = time::interval(Duration::from_secs(60));

        loop {
            tokio::select! {
                _ = connection_stat_collection_interval.tick() => {
                    if let Some(network) = self.network.upgrade() {
                        for peer_id in connected_peers.iter() {
                            if let Some(connection) = network.peer(*peer_id) {
                                let stats = connection.connection_stats();
                                self.update_quinn_metrics_for_peer(peer_id, &stats);
                            }
                        }
                    } else {
                        continue;
                    }
                }
                Ok(event) = subscriber.recv() => {
                    match event {
                        PeerEvent::NewPeer(peer_id) => {
                            self.handle_peer_status_change(peer_id, ConnectionStatus::Connected).await;
                            connected_peers.insert(peer_id);
                        }
                        PeerEvent::LostPeer(peer_id, _) => {
                            self.handle_peer_status_change(peer_id, ConnectionStatus::Disconnected).await;
                            connected_peers.remove(&peer_id);
                        }
                    }
                }
            }
        }
    }

    async fn handle_peer_status_change(
        &self,
        peer_id: PeerId,
        connection_status: ConnectionStatus,
    ) {
        if let Some(network) = self.network.upgrade() {
            self.connection_metrics
                .network_peers
                .set(network.peers().len() as i64);
        } else {
            return;
        }

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            let int_status = match connection_status {
                ConnectionStatus::Connected => 1,
                ConnectionStatus::Disconnected => 0,
            };

            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), ty])
                .set(int_status);
        }

        self.connection_statuses.insert(peer_id, connection_status);
    }

    // TODO: Replace this with ClosureMetric
    fn update_quinn_metrics_for_peer(&self, peer_id: &PeerId, stats: &ConnectionStats) {
        // Update PathStats
        self.connection_metrics
            .network_peer_rtt
            .with_label_values(&[&format!("{peer_id}")])
            .observe(stats.path.rtt.as_millis() as f64);
        self.connection_metrics
            .network_peer_lost_packets
            .with_label_values(&[&format!("{peer_id}")])
            .set(stats.path.lost_packets as i64);
        self.connection_metrics
            .network_peer_lost_bytes
            .with_label_values(&[&format!("{peer_id}")])
            .set(stats.path.lost_bytes as i64);
        self.connection_metrics
            .network_peer_sent_packets
            .with_label_values(&[&format!("{peer_id}")])
            .set(stats.path.sent_packets as i64);
        self.connection_metrics
            .network_peer_congestion_events
            .with_label_values(&[&format!("{peer_id}")])
            .set(stats.path.congestion_events as i64);
        self.connection_metrics
            .network_peer_congestion_window
            .with_label_values(&[&format!("{peer_id}")])
            .observe(stats.path.cwnd as f64);

        // Update FrameStats
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.frame_tx.max_data as i64);
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.frame_rx.max_data as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.frame_tx.connection_close as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.frame_rx.connection_close as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.frame_tx.data_blocked as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.frame_rx.data_blocked as i64);

        // Update UDPStats
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.udp_tx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.udp_rx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.udp_tx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.udp_rx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[&format!("{peer_id}"), "transmitted"])
            .set(stats.udp_tx.transmits as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[&format!("{peer_id}"), "received"])
            .set(stats.udp_rx.transmits as i64);
    }
}

#[cfg(test)]
mod tests {
    use crate::connectivity::{ConnectionMonitor, ConnectionStatus};
    use crate::metrics::NetworkConnectionMetrics;
    use anemo::{Network, Request, Response};
    use bytes::Bytes;
    use prometheus::Registry;
    use std::collections::HashMap;
    use std::convert::Infallible;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};
    use tower::util::BoxCloneService;

    #[tokio::test]
    async fn test_connectivity() {
        // GIVEN
        let network_1 = build_network().unwrap();
        let network_2 = build_network().unwrap();
        let network_3 = build_network().unwrap();

        let registry = Registry::new();
        let metrics = NetworkConnectionMetrics::new("primary", &registry);

        // AND we connect to peer 2
        let peer_2 = network_1.connect(network_2.local_addr()).await.unwrap();

        // WHEN bring up the monitor
        let (_h, statuses) =
            ConnectionMonitor::spawn(network_1.downgrade(), metrics.clone(), HashMap::new());

        // THEN peer 2 should be already connected
        assert_network_peers(metrics.clone(), 1).await;
        assert_eq!(
            *statuses.get(&peer_2).unwrap().value(),
            ConnectionStatus::Connected
        );

        // WHEN connect to peer 3
        let peer_3 = network_1.connect(network_3.local_addr()).await.unwrap();

        // THEN
        assert_network_peers(metrics.clone(), 2).await;
        assert_eq!(
            *statuses.get(&peer_3).unwrap().value(),
            ConnectionStatus::Connected
        );

        // AND disconnect peer 2
        network_1.disconnect(peer_2).unwrap();

        // THEN
        assert_network_peers(metrics.clone(), 1).await;
        assert_eq!(
            *statuses.get(&peer_2).unwrap().value(),
            ConnectionStatus::Disconnected
        );

        // AND disconnect peer 3
        network_1.disconnect(peer_3).unwrap();

        // THEN
        assert_network_peers(metrics.clone(), 0).await;
        assert_eq!(
            *statuses.get(&peer_3).unwrap().value(),
            ConnectionStatus::Disconnected
        );
    }

    async fn assert_network_peers(metrics: NetworkConnectionMetrics, value: i64) {
        let m = metrics.clone();
        timeout(Duration::from_secs(5), async move {
            while m.network_peers.get() != value {
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

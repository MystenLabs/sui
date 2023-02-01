// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::NetworkConnectionMetrics;
use anemo::PeerId;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tracing::error;

pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: NetworkConnectionMetrics,
    peer_id_types: HashMap<PeerId, String>,
    sender: Option<Sender<(PeerId, ConnectionStatus)>>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: NetworkConnectionMetrics,
        peer_id_types: HashMap<PeerId, String>,
        sender: Option<Sender<(PeerId, ConnectionStatus)>>,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            Self {
                network,
                connection_metrics,
                peer_id_types,
                sender,
            }
            .run(),
            "ConnectionMonitor"
        )
    }

    async fn run(self) {
        let (mut subscriber, connected_peers) = {
            if let Some(network) = self.network.upgrade() {
                let Ok((subscriber, connected_peers)) = network.subscribe() else {
                    return;
                };

                (subscriber, connected_peers)
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

        self.connection_metrics
            .network_peers
            .set(connected_peers.len() as i64);

        // now report the connected peers
        for peer_id in connected_peers {
            self.handle_peer_connect(peer_id).await;
        }

        while let Ok(event) = subscriber.recv().await {
            match event {
                anemo::types::PeerEvent::NewPeer(peer_id) => {
                    self.handle_peer_connect(peer_id).await;
                }
                anemo::types::PeerEvent::LostPeer(peer_id, _) => {
                    self.handle_peer_disconnect(peer_id).await;
                }
            }
        }
    }

    async fn handle_peer_connect(&self, peer_id: PeerId) {
        self.connection_metrics.network_peers.inc();

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), ty])
                .set(1);

            match &self.sender {
                Some(s) => {
                    if let Err(e) = s.send((peer_id, ConnectionStatus::Connected)).await {
                        error!("Error sending connection status {e}");
                    }
                }
                None => {}
            }
        }
    }

    async fn handle_peer_disconnect(&self, peer_id: PeerId) {
        self.connection_metrics.network_peers.dec();

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), ty])
                .set(0);

            match &self.sender {
                Some(s) => {
                    if let Err(e) = s.send((peer_id, ConnectionStatus::Disconnected)).await {
                        error!("Error sending connection status {e}");
                    }
                }
                None => {}
            }
        }
    }
}

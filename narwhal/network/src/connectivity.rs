// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
use metrics::gauge;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time;
use types::ConditionalBroadcastReceiver;

const CONNECTION_STAT_COLLECTION_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    peer_id_types: HashMap<PeerId, String>,
    connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
    rx_shutdown: Option<ConditionalBroadcastReceiver>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        peer_id_types: HashMap<PeerId, String>,
    ) -> JoinHandle<()> {
        tokio::spawn(
            Self {
                network,
                peer_id_types,
            }
            .run(),
        )
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

        /* TODO(metrics)
        // we report first all the known peers as disconnected - so we can see
        // their labels in the metrics reporting tool
        for (_peer_id, _ty) in &self.peer_id_types {
            // TODO(metrics): Set `network_peer_connected` to 0
        }
        */

        // TODO(metrics): Set `network_peers` to `connected_peers.len() as i64`

        // now report the connected peers
        let mut peer_count: usize = connected_peers.len();
        gauge!(snarkos_metrics::network::NETWORK_PEERS, peer_count as f64);
        for peer_id in connected_peers {
            self.handle_peer_connect(peer_id);
        }

        while let Ok(event) = subscriber.recv().await {
            match event {
                anemo::types::PeerEvent::NewPeer(peer_id) => {
                    peer_count += 1;
                    gauge!(snarkos_metrics::network::NETWORK_PEERS, peer_count as f64);
                    self.handle_peer_connect(peer_id);
                }
                anemo::types::PeerEvent::LostPeer(peer_id, _) => {
                    peer_count = peer_count.saturating_sub(1);
                    gauge!(snarkos_metrics::network::NETWORK_PEERS, peer_count as f64);
                    self.handle_peer_disconnect(peer_id);
                }
            }
        }
    }

    fn handle_peer_connect(&self, peer_id: PeerId) {
        use snarkos_metrics::network::labels::PEER_ID;

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            gauge!(snarkos_metrics::network::NETWORK_PEER_CONNECTED, 1.0, PEER_ID => ty.to_string());
        }

        self.connection_statuses.insert(peer_id, connection_status);
    }

    fn handle_peer_disconnect(&self, peer_id: PeerId) {
        use snarkos_metrics::network::labels::PEER_ID;

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            gauge!(snarkos_metrics::network::NETWORK_PEER_CONNECTED, 0.0, PEER_ID => ty.to_string());
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::NetworkConnectionMetrics;
use anemo::PeerId;
use std::collections::HashMap;
use sui_metrics::spawn_monitored_task;
use tokio::task::JoinHandle;

const PEER_TYPE_NONE: &str = "";

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: NetworkConnectionMetrics,
    peer_id_types: HashMap<PeerId, String>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: NetworkConnectionMetrics,
        peer_id_types: HashMap<PeerId, String>,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(Self {
            network,
            connection_metrics,
            peer_id_types
        }
        .run())
    }

    async fn run(self) {
        let ((mut subscriber, connected_peers), all_peers) = {
            if let Some(network) = self.network.upgrade() {
                (network.subscribe(), network.known_peers().get_all())
            } else {
                return;
            }
        };

        // we report first all the known peers as disconnected - so we can see
        // their labels in the metrics reporting tool
        for peer in all_peers.iter().map(|p| p.peer_id) {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer}"), self.peer_type(peer)])
                .set(0)
        }

        // now report the connected peers
        for peer in connected_peers {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer}"), self.peer_type(peer)])
                .set(1)
        }

        while let Ok(event) = subscriber.recv().await {
            match event {
                anemo::types::PeerEvent::NewPeer(peer) => self
                    .connection_metrics
                    .network_peer_connected
                    .with_label_values(&[&format!("{peer}"), self.peer_type(peer)])
                    .set(1),
                anemo::types::PeerEvent::LostPeer(peer, _) => self
                    .connection_metrics
                    .network_peer_connected
                    .with_label_values(&[&format!("{peer}"), self.peer_type(peer)])
                    .set(0),
            }
        }
    }

    fn peer_type(&self, peer_id: PeerId) -> &str {
        if let Some(tp) = self.peer_id_types.get(&peer_id) {
            tp.as_str()
        } else {
            PEER_TYPE_NONE
        }
    }
}

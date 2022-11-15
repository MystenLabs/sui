// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::NetworkConnectionMetrics;
use sui_metrics::spawn_monitored_task;
use tokio::task::JoinHandle;

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: NetworkConnectionMetrics,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: NetworkConnectionMetrics,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(Self {
            network,
            connection_metrics,
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
                .with_label_values(&[&format!("{peer}")])
                .set(0)
        }

        // now report the connected peers
        for peer in connected_peers {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer}")])
                .set(1)
        }

        while let Ok(event) = subscriber.recv().await {
            match event {
                anemo::types::PeerEvent::NewPeer(peer) => self
                    .connection_metrics
                    .network_peer_connected
                    .with_label_values(&[&format!("{peer}")])
                    .set(1),
                anemo::types::PeerEvent::LostPeer(peer, _) => self
                    .connection_metrics
                    .network_peer_connected
                    .with_label_values(&[&format!("{peer}")])
                    .set(0),
            }
        }
    }
}

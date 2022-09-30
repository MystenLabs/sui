// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: A&pache-2.0
use tokio::task::JoinHandle;

use crate::metrics::NetworkConnectionMetrics;

pub struct ConnectionMonitor {
    network: anemo::Network,
    connection_metrics: NetworkConnectionMetrics,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::Network,
        connection_metrics: NetworkConnectionMetrics,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                network,
                connection_metrics,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        let (mut subscriber, peers) = self.network.subscribe();
        for peer in peers.iter() {
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

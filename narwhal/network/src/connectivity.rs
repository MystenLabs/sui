// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use tokio::{sync::watch, task::JoinHandle};
use types::ReconfigureNotification;

use crate::metrics::NetworkConnectionMetrics;

pub struct ConnectionMonitor {
    network: anemo::Network,
    connection_metrics: NetworkConnectionMetrics,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::Network,
        connection_metrics: NetworkConnectionMetrics,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                network,
                connection_metrics,
                rx_reconfigure,
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
        loop {
            tokio::select! {
                Ok(event) = subscriber.recv() => {
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
                },

                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    if let ReconfigureNotification::Shutdown = message {
                        return;
                    }
                }
            }
        }
    }
}

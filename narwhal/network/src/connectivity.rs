// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
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
        for peer_id in connected_peers.iter() {
            self.handle_peer_status_change(*peer_id, ConnectionStatus::Connected)
                .await;
        }

        let mut connection_stat_collection_interval =
            time::interval(CONNECTION_STAT_COLLECTION_INTERVAL);

        async fn wait_for_shutdown(
            rx_shutdown: &mut Option<ConditionalBroadcastReceiver>,
        ) -> Result<(), tokio::sync::broadcast::error::RecvError> {
            if let Some(rx) = rx_shutdown.as_mut() {
                rx.receiver.recv().await
            } else {
                // If no shutdown receiver is provided, wait forever.
                let future = future::pending();
                #[allow(clippy::let_unit_value)]
                let () = future.await;
                Ok(())
            }
        }

        loop {
            tokio::select! {
                _ = connection_stat_collection_interval.tick() => {
                    if let Some(network) = self.network.upgrade() {
                        for peer_id in known_peers.iter() {
                            if let Some(connection) = network.peer(*peer_id) {
                                let stats = connection.connection_stats();
                                self.update_quinn_metrics_for_peer(&format!("{peer_id}"), &stats);
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
                        }
                        PeerEvent::LostPeer(peer_id, _) => {
                            self.handle_peer_status_change(peer_id, ConnectionStatus::Disconnected).await;
                        }
                    }
                }
                _ = wait_for_shutdown(&mut self.rx_shutdown) => {
                    return;
                }
            }
        }
    }

    fn handle_peer_connect(&self, peer_id: PeerId) {
        // TODO(metrics): Increment `network_peers` by 1

        if let Some(_ty) = self.peer_id_types.get(&peer_id) {
            // TODO(metrics): Set `network_peer_connected` to 1
        }

        self.connection_statuses.insert(peer_id, connection_status);
    }

    fn handle_peer_disconnect(&self, peer_id: PeerId) {
        // TODO(metrics): Decrement `network_peers` by 1

        if let Some(_ty) = self.peer_id_types.get(&peer_id) {
            // TODO(metrics): Set `network_peer_connected` to 0
        }
    }
}

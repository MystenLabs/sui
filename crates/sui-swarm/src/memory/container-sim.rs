// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use sui_config::NodeConfig;
use sui_node::{SuiNode, SuiNodeHandle};
use tokio::sync::watch;
use tracing::{info, trace};

use super::node::RuntimeType;

#[derive(Debug)]
pub(crate) struct Container {
    handle: Option<ContainerHandle>,
    cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
    node_watch: watch::Receiver<Weak<SuiNode>>,
}

#[derive(Debug)]
struct ContainerHandle {
    node_id: sui_simulator::task::NodeId,
}

/// When dropped, stop and wait for the node running in this Container to completely shutdown.
impl Drop for Container {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            tracing::info!("shutting down {}", handle.node_id);
            sui_simulator::runtime::Handle::try_current().map(|h| h.delete_node(handle.node_id));
        }
    }
}

impl Container {
    /// Spawn a new Node.
    pub async fn spawn(config: NodeConfig, _runtime: RuntimeType) -> Self {
        let (startup_sender, mut startup_receiver) = tokio::sync::watch::channel(Weak::new());
        let (cancel_sender, cancel_receiver) = tokio::sync::watch::channel(false);

        let handle = sui_simulator::runtime::Handle::current();
        let builder = handle.create_node();

        let socket_addr =
            mysten_network::multiaddr::to_socket_addr(&config.network_address).unwrap();
        let ip = match socket_addr {
            SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
            _ => panic!("unsupported protocol"),
        };

        let config = Arc::new(config);
        let startup_sender = Arc::new(startup_sender);
        let node = builder
            .ip(ip)
            .name(&format!("{:?}", config.protocol_public_key().concise()))
            .init(move || {
                info!("Node restarted");
                let config = config.clone();
                let mut cancel_receiver = cancel_receiver.clone();
                let startup_sender = startup_sender.clone();
                async move {
                    let registry_service = mysten_metrics::RegistryService::new(Registry::new());
                    let server = SuiNode::start(&config, registry_service, None)
                        .await
                        .unwrap();

                    startup_sender.send(Arc::downgrade(&server)).ok();

                    // run until canceled
                    loop {
                        if cancel_receiver.changed().await.is_err() || *cancel_receiver.borrow() {
                            break;
                        }
                    }
                    trace!("cancellation received; shutting down thread");
                }
            })
            .build();

        startup_receiver.changed().await.unwrap();

        Self {
            handle: Some(ContainerHandle { node_id: node.id() }),
            cancel_sender: Some(cancel_sender),
            node_watch: startup_receiver,
        }
    }

    /// Get a SuiNodeHandle to the node owned by the container.
    pub fn get_node_handle(&self) -> Option<SuiNodeHandle> {
        Some(SuiNodeHandle::new(self.node_watch.borrow().upgrade()?))
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
    ///
    pub fn is_alive(&self) -> bool {
        if let Some(cancel_sender) = &self.cancel_sender {
            // unless the node is deleted, it keeps a reference to its start up function, which
            // keeps 1 receiver alive. If the node is actually running, the cloned receiver will
            // also be alive, and receiver count will be 2.
            cancel_sender.receiver_count() > 1
        } else {
            false
        }
    }
}

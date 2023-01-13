// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_node::SuiNode;
use tracing::{info, trace};

use super::node::RuntimeType;

#[derive(Debug)]
pub(crate) struct Container {
    handle: Option<ContainerHandle>,
    cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
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
        let (startup_sender, mut startup_reciever) = tokio::sync::watch::channel(false);
        let (cancel_sender, cancel_reciever) = tokio::sync::watch::channel(false);

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
                let mut cancel_reciever = cancel_reciever.clone();
                let startup_sender = startup_sender.clone();
                async move {
                    let registry_service = mysten_metrics::RegistryService::new(Registry::new());
                    let _server = SuiNode::start(&config, registry_service).await.unwrap();

                    startup_sender.send(true).ok();

                    // run until canceled
                    loop {
                        if cancel_reciever.changed().await.is_err() || *cancel_reciever.borrow() {
                            break;
                        }
                    }
                    trace!("cancellation received; shutting down thread");
                }
            })
            .build();

        startup_reciever.changed().await.unwrap();
        assert!(*startup_reciever.borrow());

        Self {
            handle: Some(ContainerHandle { node_id: node.id() }),
            cancel_sender: Some(cancel_sender),
        }
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
    ///
    pub fn is_alive(&self) -> bool {
        if let Some(cancel_sender) = &self.cancel_sender {
            cancel_sender.receiver_count() > 0
        } else {
            false
        }
    }
}

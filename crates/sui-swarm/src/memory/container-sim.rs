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
    cancel_sender: Option<watch::Sender<bool>>,
}

#[derive(Debug)]
struct ContainerHandle {
    node_id: sui_simulator::task::NodeId,
    node_handle: watch::Receiver<Weak<SuiNodeHandle>>,
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
        let (startup_sender, mut node_handle) = watch::channel(Weak::new());
        let (cancel_sender, cancel_reciever) = watch::channel(false);

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
                    let node = SuiNode::start(&config, registry_service).await.unwrap();
                    let node_handle = Arc::new(SuiNodeHandle::new(node.clone()));
                    let node_handle_weak = Arc::downgrade(&node_handle);

                    startup_sender.send(node_handle_weak.clone()).ok();

                    // run until canceled
                    loop {
                        if cancel_reciever.changed().await.is_err() || *cancel_reciever.borrow() {
                            break;
                        }
                    }
                    trace!("cancellation received; shutting down thread");
                    drop(node_handle);
                    assert!(
                        node_handle_weak.upgrade().is_none(),
                        "strong reference to node_handle detected"
                    );
                }
            })
            .build();

        node_handle.changed().await.unwrap();
        assert!(node_handle.borrow().upgrade().is_some());

        Self {
            handle: Some(ContainerHandle {
                node_id: node.id(),
                node_handle,
            }),
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

    /// Get a weak reference to the current NodeHandle. After upgrading the reference, the caller
    /// must be careful not to hold onto the strong reference for any longer than necessary, as
    /// this would prevent the node from being deallocated if it crashes / is stopped.
    pub fn watch_node_handle(&self) -> watch::Receiver<Weak<SuiNodeHandle>> {
        self.handle.as_ref().unwrap().node_handle.clone()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::FutureExt;
use prometheus::Registry;
use std::net::{IpAddr, SocketAddr};
use sui_config::NodeConfig;
use sui_node::SuiNode;
use tracing::trace;

use super::node::RuntimeType;

#[derive(Debug)]
pub(crate) struct Container {
    join_handle: Option<ContainerJoinHandle>,
    cancel_sender: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug)]
struct ContainerJoinHandle {
    node_id: sui_simulator::task::NodeId,
    task_handle: sui_simulator::task::JoinHandle<()>,
}

/// When dropped, stop and wait for the node running in this Container to completely shutdown.
impl Drop for Container {
    fn drop(&mut self) {
        trace!("dropping Container");

        let handle = self.join_handle.take().unwrap();

        tracing::info!("shutting down {}", handle.node_id);
        handle.task_handle.abort();
        sui_simulator::runtime::Handle::try_current().map(|h| h.kill(handle.node_id));

        trace!("finished dropping Container");
    }
}

impl Container {
    /// Spawn a new Node.
    pub fn spawn(
        config: NodeConfig,
        _runtime: RuntimeType,
    ) -> (tokio::sync::oneshot::Receiver<()>, Self) {
        let (startup_sender, startup_reciever) = tokio::sync::oneshot::channel();
        let (cancel_sender, cancel_reciever) = tokio::sync::oneshot::channel();

        let handle = sui_simulator::runtime::Handle::current();
        let builder = handle.create_node();

        let socket_addr =
            mysten_network::multiaddr::to_socket_addr(&config.network_address).unwrap();
        let ip = match socket_addr {
            SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
            _ => panic!("unsupported protocol"),
        };

        let node = builder
            .ip(ip)
            .name(&format!("{:?}", config.protocol_public_key().concise()))
            .init(|| async {
                tracing::info!("node restarted");
            })
            .build();

        let task_handle = node.spawn(async move {
            let _server = SuiNode::start(&config, Registry::new()).await.unwrap();
            // Notify that we've successfully started the node
            trace!("node started, sending oneshot");
            let _ = startup_sender.send(());
            // run until canceled
            cancel_reciever.map(|_| ()).await;
            trace!("cancellation received; shutting down thread");
        });

        (
            startup_reciever,
            Self {
                join_handle: Some(ContainerJoinHandle {
                    node_id: node.id(),
                    task_handle,
                }),
                cancel_sender: Some(cancel_sender),
            },
        )
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
    ///
    pub fn is_alive(&self) -> bool {
        if let Some(cancel_sender) = &self.cancel_sender {
            !cancel_sender.is_closed()
        } else {
            false
        }
    }
}

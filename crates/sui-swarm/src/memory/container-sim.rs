// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use sui_config::NodeConfig;
use sui_node::{SuiNode, SuiNodeHandle};
use sui_types::base_types::ConciseableName;
use tokio::sync::watch;
use tracing::{info, trace, warn};

use super::node::RuntimeType;

/// Maximum attempts to (re)start a node before giving up. A restarted node
/// reopens the same `db_path`, and the previous instance's RocksDB locks may
/// still be held: a node's teardown (including its background tasks and the
/// real-thread RocksDB close) is asynchronous and not deterministic with
/// respect to the simulated clock, so the locks are released some
/// non-deterministic time after the stop. Retrying the open until they are
/// free is robust where a fixed wait is not.
const MAX_START_ATTEMPTS: usize = 60;

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

        let socket_addr = config.network_address.to_socket_addr().unwrap();
        let ip = match socket_addr {
            SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
            _ => panic!("unsupported protocol"),
        };

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
                    // Retry the open: a node restarted on the same `db_path`
                    // can race the previous instance's still-pending RocksDB
                    // teardown and fail to acquire its file locks. The
                    // teardown completes a non-deterministic time later, so
                    // retry rather than panic on a transient lock conflict.
                    let mut server = None;
                    for attempt in 1..=MAX_START_ATTEMPTS {
                        let registry_service =
                            mysten_metrics::RegistryService::new(Registry::new());
                        match SuiNode::start(config.clone(), registry_service).await {
                            Ok(node) => {
                                server = Some(node);
                                break;
                            }
                            Err(e) if attempt < MAX_START_ATTEMPTS => {
                                warn!("node start attempt {attempt} failed, retrying: {e:?}");
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                            Err(e) => {
                                panic!("node failed to start after {attempt} attempts: {e:?}")
                            }
                        }
                    }
                    let server = server.expect("node start loop must set the node or panic");

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

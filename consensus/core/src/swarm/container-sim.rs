// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwapOption;
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tracing::{info, trace};

use super::node::NodeConfig;
use crate::network::tonic_network::to_socket_addr;
use crate::transaction::NoopTransactionVerifier;
use crate::{CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority};

pub(crate) struct AuthorityNodeContainer {
    handle: Option<ContainerHandle>,
    cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
    consensus_authority: ConsensusAuthority,
    commit_receiver: ArcSwapOption<UnboundedReceiver<CommittedSubDag>>,
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,
}

#[derive(Debug)]
struct ContainerHandle {
    node_id: sui_simulator::task::NodeId,
}

/// When dropped, stop and wait for the node running in this Container to completely shutdown.
impl Drop for AuthorityNodeContainer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            tracing::info!("shutting down {}", handle.node_id);
            sui_simulator::runtime::Handle::try_current().map(|h| h.delete_node(handle.node_id));
        }
    }
}

impl AuthorityNodeContainer {
    /// Spawn a new Node.
    pub async fn spawn(config: super::node::NodeConfig) -> Self {
        let (startup_sender, mut startup_receiver) = tokio::sync::watch::channel(false);
        let (cancel_sender, cancel_receiver) = tokio::sync::watch::channel(false);

        let handle = sui_simulator::runtime::Handle::current();
        let builder = handle.create_node();

        let authority = config.committee.authority(config.authority_index);
        let socket_addr = to_socket_addr(&authority.address).unwrap();
        let ip = match socket_addr {
            SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
            _ => panic!("unsupported protocol"),
        };
        let init_receiver_swap = Arc::new(ArcSwapOption::empty());
        let int_receiver_swap_clone = init_receiver_swap.clone();

        let node = builder
            .ip(ip)
            .name(format!("{}", config.authority_index))
            .init(move || {
                info!("Node restarted");
                let config = config.clone();
                let mut cancel_receiver = cancel_receiver.clone();
                let init_receiver_swap_clone = int_receiver_swap_clone.clone();
                let startup_sender_clone = startup_sender.clone();

                async move {
                    let (consensus_authority, commit_receiver, commit_consumer_monitor) =
                        super::node::make_authority(config).await;

                    startup_sender_clone.send(true).ok();
                    init_receiver_swap_clone.store(Some(Arc::new((
                        consensus_authority,
                        commit_receiver,
                        commit_consumer_monitor,
                    ))));

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

        let Some(init_tuple) = init_receiver_swap.swap(None) else {
            panic!("Components should be initialised by now");
        };

        let Ok((consensus_authority, commit_receiver, commit_consumer_monitor)) =
            Arc::try_unwrap(init_tuple)
        else {
            panic!("commit receiver still in use");
        };

        Self {
            handle: Some(ContainerHandle { node_id: node.id() }),
            cancel_sender: Some(cancel_sender),
            consensus_authority,
            commit_receiver: ArcSwapOption::new(Some(Arc::new(commit_receiver))),
            commit_consumer_monitor,
        }
    }

    /// Check to see that the Node is still alive by checking if the receiving side of the
    /// `cancel_sender` has been dropped.
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

    pub fn take_commit_receiver(&self) -> UnboundedReceiver<CommittedSubDag> {
        if let Some(commit_receiver) = self.commit_receiver.swap(None) {
            let Ok(commit_receiver) = Arc::try_unwrap(commit_receiver) else {
                panic!("commit receiver still in use");
            };

            commit_receiver
        } else {
            panic!("commit receiver already taken");
        }
    }

    pub fn commit_consumer_monitor(&self) -> Arc<CommitConsumerMonitor> {
        self.commit_consumer_monitor.clone()
    }

    pub fn transaction_client(&self) -> Arc<crate::TransactionClient> {
        self.consensus_authority.transaction_client()
    }
}

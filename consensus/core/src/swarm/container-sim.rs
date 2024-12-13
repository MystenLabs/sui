// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwapOption;
use consensus_config::Parameters;
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use prometheus::Registry;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, trace};

use super::node::NodeConfig;
use crate::network::tonic_network::to_socket_addr;
use crate::transaction::NoopTransactionVerifier;
use crate::{CommitConsumer, CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority};

pub(crate) struct AuthorityNodeContainer {
    handle: Option<ContainerHandle>,
    cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
    node_watch: watch::Receiver<Option<ConsensusAuthority>>,
    commit_receiver: Arc<
        ArcSwapOption<(
            UnboundedReceiver<CommittedSubDag>,
            Arc<CommitConsumerMonitor>,
        )>,
    >,
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
    pub async fn spawn(config: NodeConfig) -> Self {
        let (startup_sender, mut startup_receiver) = tokio::sync::watch::channel(None);
        let (cancel_sender, cancel_receiver) = tokio::sync::watch::channel(false);

        let handle = sui_simulator::runtime::Handle::current();
        let builder = handle.create_node();

        let authority = config.committee.authority(config.authority_index);
        let socket_addr = to_socket_addr(&authority.address).unwrap();
        let ip = match socket_addr {
            SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
            _ => panic!("unsupported protocol"),
        };
        let commit_receiver_swap = Arc::new(ArcSwapOption::empty());
        let commit_receiver_swap_clone = commit_receiver_swap.clone();

        let node = builder
            .ip(ip)
            .name(format!("{}", config.authority_index))
            .init(move || {
                info!("Node restarted");
                let config = config.clone();
                let mut cancel_receiver = cancel_receiver.clone();
                let startup_sender = startup_sender.clone();
                let commit_receiver_swap_clone = commit_receiver_swap_clone.clone();

                async move {
                    let (consensus_authority, commit_receiver, commit_consumer_monitor) =
                        AuthorityNodeContainer::make_authority(config).await;

                    startup_sender.send(Some(consensus_authority)).ok();
                    commit_receiver_swap_clone
                        .store(Some(Arc::new((commit_receiver, commit_consumer_monitor))));

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
            commit_receiver: commit_receiver_swap,
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

    pub fn take_commit_receiver(
        &self,
    ) -> (
        UnboundedReceiver<CommittedSubDag>,
        Arc<CommitConsumerMonitor>,
    ) {
        if let Some(tuple) = self.commit_receiver.swap(None) {
            let Ok((commit_receiver, commit_consumer_monitor)) = Arc::try_unwrap(tuple) else {
                panic!("commit receiver still in use");
            };

            (commit_receiver, commit_consumer_monitor)
        } else {
            panic!("commit receiver already taken");
        }
    }

    async fn make_authority(
        config: NodeConfig,
    ) -> (
        ConsensusAuthority,
        UnboundedReceiver<CommittedSubDag>,
        Arc<CommitConsumerMonitor>,
    ) {
        let NodeConfig {
            authority_index,
            db_dir,
            committee,
            keypairs,
            network_type,
            boot_counter,
            protocol_config,
        } = config;

        let registry = Registry::new();

        // Cache less blocks to exercise commit sync.
        let parameters = Parameters {
            db_path: db_dir.path().to_path_buf(),
            dag_state_cached_rounds: 5,
            commit_sync_parallel_fetches: 2,
            commit_sync_batch_size: 3,
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let protocol_keypair = keypairs[authority_index].1.clone();
        let network_keypair = keypairs[authority_index].0.clone();

        let (commit_consumer, commit_receiver, _) = CommitConsumer::new(0);
        let commit_consumer_monitor = commit_consumer.monitor();

        let authority = ConsensusAuthority::start(
            network_type,
            authority_index,
            committee,
            parameters,
            protocol_config,
            protocol_keypair,
            network_keypair,
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            boot_counter,
        )
        .await;

        (authority, commit_receiver, commit_consumer_monitor)
    }
}

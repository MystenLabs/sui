// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwapOption;
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tracing::{info, trace};

use anyhow::Result;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use parking_lot::Mutex;
use prometheus::Registry;
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};
use tempfile::TempDir;

use consensus_core::network::tonic_network::to_socket_addr;
use consensus_core::transaction::NoopTransactionVerifier;
use consensus_core::{
    CommitConsumer, CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority, TransactionClient,
};

#[derive(Clone)]
#[allow(unused)]
pub(crate) struct Config {
    pub authority_index: AuthorityIndex,
    pub db_dir: Arc<TempDir>,
    pub committee: Committee,
    pub keypairs: Vec<(NetworkKeyPair, ProtocolKeyPair)>,
    pub network_type: ConsensusNetwork,
    pub boot_counter: u64,
    pub protocol_config: ProtocolConfig,
}

pub(crate) struct AuthorityNode {
    inner: Mutex<Option<AuthorityNodeInner>>,
    config: Config,
}

impl AuthorityNode {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Default::default(),
            config,
        }
    }

    /// Return the `index` of this Node
    pub fn index(&self) -> AuthorityIndex {
        self.config.authority_index
    }

    /// Start this Node
    pub async fn start(&self) -> Result<()> {
        info!(index =% self.config.authority_index, "starting in-memory node");
        let config = self.config.clone();
        *self.inner.lock() = Some(AuthorityNodeInner::spawn(config).await);
        Ok(())
    }

    pub fn spawn_committed_subdag_consumer(&self) -> Result<()> {
        let authority_index = self.config.authority_index;
        let inner = self.inner.lock();
        if let Some(inner) = inner.as_ref() {
            let mut commit_receiver = inner.take_commit_receiver();
            let commit_consumer_monitor = inner.commit_consumer_monitor();
            let _handle = tokio::spawn(async move {
                while let Some(subdag) = commit_receiver.recv().await {
                    info!(index =% authority_index, "received committed subdag");
                    commit_consumer_monitor.set_highest_handled_commit(subdag.commit_ref.index);
                }
            });
        }
        Ok(())
    }

    pub fn commit_consumer_monitor(&self) -> Arc<CommitConsumerMonitor> {
        let inner = self.inner.lock();
        if let Some(inner) = inner.as_ref() {
            inner.commit_consumer_monitor()
        } else {
            panic!("Node not initialised");
        }
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        let inner = self.inner.lock();
        if let Some(inner) = inner.as_ref() {
            inner.transaction_client()
        } else {
            panic!("Node not initialised");
        }
    }

    /// Stop this Node
    pub fn stop(&self) {
        info!(index =% self.config.authority_index, "stopping in-memory node");
        *self.inner.lock() = None;
        info!(index =% self.config.authority_index, "node stopped");
    }

    /// If this Node is currently running
    pub fn is_running(&self) -> bool {
        self.inner.lock().as_ref().map_or(false, |c| c.is_alive())
    }
}

pub(crate) struct AuthorityNodeInner {
    handle: Option<NodeHandle>,
    cancel_sender: Option<tokio::sync::watch::Sender<bool>>,
    consensus_authority: ConsensusAuthority,
    commit_receiver: ArcSwapOption<UnboundedReceiver<CommittedSubDag>>,
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,
}

#[derive(Debug)]
struct NodeHandle {
    node_id: sui_simulator::task::NodeId,
}

/// When dropped, stop and wait for the node running in this node to completely shutdown.
impl Drop for AuthorityNodeInner {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            tracing::info!("shutting down {}", handle.node_id);
            sui_simulator::runtime::Handle::try_current().map(|h| h.delete_node(handle.node_id));
        }
    }
}

impl AuthorityNodeInner {
    /// Spawn a new Node.
    pub async fn spawn(config: Config) -> Self {
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
            handle: Some(NodeHandle { node_id: node.id() }),
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

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        self.consensus_authority.transaction_client()
    }
}

pub(crate) async fn make_authority(
    config: Config,
) -> (
    ConsensusAuthority,
    UnboundedReceiver<CommittedSubDag>,
    Arc<CommitConsumerMonitor>,
) {
    let Config {
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

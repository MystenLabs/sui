// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::container::AuthorityNodeContainer;
use anyhow::Result;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use parking_lot::Mutex;
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};
use tempfile::TempDir;
use tracing::info;

use crate::transaction::NoopTransactionVerifier;
use crate::{
    CommitConsumer, CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority, TransactionClient,
};

#[derive(Clone)]
#[allow(unused)]
pub(crate) struct NodeConfig {
    pub authority_index: AuthorityIndex,
    pub db_dir: Arc<TempDir>,
    pub committee: Committee,
    pub keypairs: Vec<(NetworkKeyPair, ProtocolKeyPair)>,
    pub network_type: ConsensusNetwork,
    pub boot_counter: u64,
    pub protocol_config: ProtocolConfig,
}

/// A wrapper struct to allow us manage the underlying AuthorityNode.
#[allow(unused)]
pub(crate) struct Node {
    container: Mutex<Option<AuthorityNodeContainer>>,
    config: NodeConfig,
}

#[allow(unused)]
impl Node {
    /// Create a new Node from the provided `NodeConfig`.
    pub fn new(config: NodeConfig) -> Self {
        Self {
            container: Default::default(),
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
        *self.container.lock() = Some(AuthorityNodeContainer::spawn(config).await);
        Ok(())
    }

    pub fn spawn_committed_subdag_consumer(&self) -> Result<()> {
        let authority_index = self.config.authority_index;
        let container = self.container.lock();
        if let Some(container) = container.as_ref() {
            let mut commit_receiver = container.take_commit_receiver();
            let commit_consumer_monitor = container.commit_consumer_monitor();
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
        let container = self.container.lock();
        if let Some(container) = container.as_ref() {
            container.commit_consumer_monitor()
        } else {
            panic!("Container not initialised");
        }
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        let container = self.container.lock();
        if let Some(container) = container.as_ref() {
            container.transaction_client()
        } else {
            panic!("Container not initialised");
        }
    }

    /// Stop this Node
    pub fn stop(&self) {
        info!(index =% self.config.authority_index, "stopping in-memory node");
        *self.container.lock() = None;
        info!(index =% self.config.authority_index, "node stopped");
    }

    /// If this Node is currently running
    pub fn is_running(&self) -> bool {
        self.container
            .lock()
            .as_ref()
            .map_or(false, |c| c.is_alive())
    }
}

pub(crate) async fn make_authority(
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

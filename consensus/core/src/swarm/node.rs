// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::container::AuthorityNodeContainer;
use anyhow::Result;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, ProtocolKeyPair};
use parking_lot::Mutex;
use std::sync::Arc;
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};
use tempfile::TempDir;
use tracing::info;

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
    pub async fn spawn(&self) -> Result<()> {
        info!(index =% self.config.authority_index, "starting in-memory node");
        let config = self.config.clone();
        *self.container.lock() = Some(AuthorityNodeContainer::spawn(config).await);
        Ok(())
    }

    /// Start this Node, waiting until its completely started up.
    pub async fn start(&self) -> Result<()> {
        self.spawn().await
    }

    pub fn spawn_committed_subdag_consumer(&self) -> Result<()> {
        let authority_index = self.config.authority_index;
        let container = self.container.lock();
        if let Some(container) = container.as_ref() {
            let (mut commit_receiver, commit_consumer_monitor) = container.take_commit_receiver();
            let _handle = tokio::spawn(async move {
                while let Some(subdag) = commit_receiver.recv().await {
                    info!(index =% authority_index, "received committed subdag");
                    commit_consumer_monitor.set_highest_handled_commit(subdag.commit_ref.index);
                }
            });
        }
        Ok(())
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use anyhow::Result;
use parking_lot::Mutex;
use tracing::info;
use super::container::AuthorityNodeContainer;
use tempfile::TempDir;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, ProtocolKeyPair};
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};

#[derive(Clone)]
pub (crate) struct NodeConfig {
    pub authority_index: AuthorityIndex,
    pub db_dir: Arc<TempDir>,
    pub committee: Committee,
    pub keypairs: Vec<(NetworkKeyPair, ProtocolKeyPair)>,
    pub network_type: ConsensusNetwork,
    pub boot_counter: u64,
    pub protocol_config: ProtocolConfig,
}

pub (crate) struct Node {
    container: Mutex<Option<AuthorityNodeContainer>>,
    config: NodeConfig
}

impl Node {
    /// Create a new Node from the provided `NodeConfig`.
    ///
    /// The Node is returned without being started. See [`Node::spawn`] or [`Node::start`] for how to
    /// start the node.
    ///
    /// [`NodeConfig`]: sui_config::NodeConfig
    pub fn new(config: NodeConfig) -> Self {
        Self {
            container: Default::default(),
            config
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
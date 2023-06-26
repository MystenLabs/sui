// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Node;
use anyhow::Result;
use futures::future::try_join_all;
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::{
    mem, ops,
    path::{Path, PathBuf},
};
use sui_config::node::DBCheckpointConfig;
use sui_config::NodeConfig;
use sui_node::SuiNodeHandle;
use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
use sui_swarm_config::genesis_config::{AccountConfig, GenesisConfig, ValidatorGenesisConfig};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::{
    CommitteeConfig, ConfigBuilder, ProtocolVersionsConfig, SupportedProtocolVersionsCallback,
};
use sui_swarm_config::node_config_builder::FullnodeConfigBuilder;
use sui_types::base_types::AuthorityName;
use sui_types::object::Object;
use tempfile::TempDir;

pub struct SwarmBuilder<R = OsRng> {
    rng: R,
    // template: NodeConfig,
    dir: Option<PathBuf>,
    committee: CommitteeConfig,
    genesis_config: Option<GenesisConfig>,
    network_config: Option<NetworkConfig>,
    additional_objects: Vec<Object>,
    fullnode_count: usize,
    fullnode_rpc_port: Option<u16>,
    fullnode_rpc_addr: Option<SocketAddr>,
    supported_protocol_versions_config: ProtocolVersionsConfig,
    // Default to supported_protocol_versions_config, but can be overridden.
    fullnode_supported_protocol_versions_config: Option<ProtocolVersionsConfig>,
    db_checkpoint_config: DBCheckpointConfig,
    num_unpruned_validators: Option<usize>,
}

impl SwarmBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            rng: OsRng,
            dir: None,
            committee: CommitteeConfig::Size(NonZeroUsize::new(1).unwrap()),
            genesis_config: None,
            network_config: None,
            additional_objects: vec![],
            fullnode_count: 0,
            fullnode_rpc_port: None,
            fullnode_rpc_addr: None,
            supported_protocol_versions_config: ProtocolVersionsConfig::Default,
            fullnode_supported_protocol_versions_config: None,
            db_checkpoint_config: DBCheckpointConfig::default(),
            num_unpruned_validators: None,
        }
    }
}

impl<R> SwarmBuilder<R> {
    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> SwarmBuilder<N> {
        SwarmBuilder {
            rng,
            dir: self.dir,
            committee: self.committee,
            genesis_config: self.genesis_config,
            network_config: self.network_config,
            additional_objects: self.additional_objects,
            fullnode_count: self.fullnode_count,
            fullnode_rpc_port: self.fullnode_rpc_port,
            fullnode_rpc_addr: self.fullnode_rpc_addr,
            supported_protocol_versions_config: self.supported_protocol_versions_config,
            fullnode_supported_protocol_versions_config: self
                .fullnode_supported_protocol_versions_config,
            db_checkpoint_config: self.db_checkpoint_config,
            num_unpruned_validators: self.num_unpruned_validators,
        }
    }

    /// Set the directory that should be used by the Swarm for any on-disk data.
    ///
    /// If a directory is provided, it will not be cleaned up when the Swarm is dropped.
    ///
    /// Defaults to using a temporary directory that will be cleaned up when the Swarm is dropped.
    pub fn dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Set the committee size (the number of validators in the validator set).
    ///
    /// Defaults to 1.
    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee = CommitteeConfig::Size(committee_size);
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorGenesisConfig>) -> Self {
        self.committee = CommitteeConfig::Validators(validators);
        self
    }

    pub fn with_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        assert!(self.network_config.is_none() && self.genesis_config.is_none());
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn with_num_unpruned_validators(mut self, n: usize) -> Self {
        assert!(self.network_config.is_none());
        self.num_unpruned_validators = Some(n);
        self
    }

    pub fn with_network_config(mut self, network_config: NetworkConfig) -> Self {
        assert!(self.network_config.is_none() && self.genesis_config.is_none());
        self.network_config = Some(network_config);
        self
    }

    pub fn with_accounts(mut self, accounts: Vec<AccountConfig>) -> Self {
        self.get_or_init_genesis_config().accounts = accounts;
        self
    }

    pub fn with_objects<I: IntoIterator<Item = Object>>(mut self, objects: I) -> Self {
        self.additional_objects.extend(objects);
        self
    }

    pub fn with_fullnode_count(mut self, fullnode_count: usize) -> Self {
        self.fullnode_count = fullnode_count;
        self
    }

    pub fn with_fullnode_rpc_port(mut self, fullnode_rpc_port: u16) -> Self {
        assert!(self.fullnode_rpc_addr.is_none());
        self.fullnode_rpc_port = Some(fullnode_rpc_port);
        self
    }

    pub fn with_fullnode_rpc_addr(mut self, fullnode_rpc_addr: SocketAddr) -> Self {
        assert!(self.fullnode_rpc_port.is_none());
        self.fullnode_rpc_addr = Some(fullnode_rpc_addr);
        self
    }

    pub fn with_epoch_duration_ms(mut self, epoch_duration_ms: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .epoch_duration_ms = epoch_duration_ms;
        self
    }

    pub fn with_protocol_version(mut self, v: ProtocolVersion) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .protocol_version = v;
        self
    }

    pub fn with_supported_protocol_versions(mut self, c: SupportedProtocolVersions) -> Self {
        self.supported_protocol_versions_config = ProtocolVersionsConfig::Global(c);
        self
    }

    pub fn with_supported_protocol_version_callback(
        mut self,
        func: SupportedProtocolVersionsCallback,
    ) -> Self {
        self.supported_protocol_versions_config = ProtocolVersionsConfig::PerValidator(func);
        self
    }

    pub fn with_supported_protocol_versions_config(mut self, c: ProtocolVersionsConfig) -> Self {
        self.supported_protocol_versions_config = c;
        self
    }

    pub fn with_fullnode_supported_protocol_versions_config(
        mut self,
        c: ProtocolVersionsConfig,
    ) -> Self {
        self.fullnode_supported_protocol_versions_config = Some(c);
        self
    }

    pub fn with_db_checkpoint_config(mut self, db_checkpoint_config: DBCheckpointConfig) -> Self {
        self.db_checkpoint_config = db_checkpoint_config;
        self
    }

    fn get_or_init_genesis_config(&mut self) -> &mut GenesisConfig {
        if self.genesis_config.is_none() {
            assert!(self.network_config.is_none());
            self.genesis_config = Some(GenesisConfig::for_local_testing());
        }
        self.genesis_config.as_mut().unwrap()
    }
}

impl<R: rand::RngCore + rand::CryptoRng> SwarmBuilder<R> {
    /// Create the configured Swarm.
    pub fn build(self) -> Swarm {
        let dir = if let Some(dir) = self.dir {
            SwarmDirectory::Persistent(dir)
        } else {
            SwarmDirectory::Temporary(TempDir::new().unwrap())
        };

        let network_config = self.network_config.unwrap_or_else(|| {
            let mut config_builder = ConfigBuilder::new(dir.as_ref());

            if let Some(genesis_config) = self.genesis_config {
                config_builder = config_builder.with_genesis_config(genesis_config);
            }

            if let Some(num_unpruned_validators) = self.num_unpruned_validators {
                config_builder =
                    config_builder.with_num_unpruned_validators(num_unpruned_validators);
            }

            config_builder
                .committee(self.committee)
                .rng(self.rng)
                .with_objects(self.additional_objects)
                .with_supported_protocol_versions_config(
                    self.supported_protocol_versions_config.clone(),
                )
                .build()
        });

        let mut nodes: HashMap<_, _> = network_config
            .validator_configs()
            .iter()
            .map(|config| (config.protocol_public_key(), Node::new(config.to_owned())))
            .collect();

        let mut fullnode_config_builder = FullnodeConfigBuilder::new()
            .with_config_directory(dir.as_ref().into())
            .with_db_checkpoint_config(self.db_checkpoint_config.clone());
        if let Some(spvc) = &self.fullnode_supported_protocol_versions_config {
            let supported_versions = match spvc {
                ProtocolVersionsConfig::Default => SupportedProtocolVersions::SYSTEM_DEFAULT,
                ProtocolVersionsConfig::Global(v) => *v,
                ProtocolVersionsConfig::PerValidator(func) => func(0, None),
            };
            fullnode_config_builder =
                fullnode_config_builder.with_supported_protocol_versions(supported_versions);
        }

        if self.fullnode_count > 0 {
            (0..self.fullnode_count).for_each(|idx| {
                let mut builder = fullnode_config_builder.clone();
                if idx == 0 {
                    // Only the first fullnode is used as the rpc fullnode, we can only use the
                    // same address once.
                    if let Some(rpc_addr) = self.fullnode_rpc_addr {
                        builder = builder.with_rpc_addr(rpc_addr);
                    }
                    if let Some(rpc_port) = self.fullnode_rpc_port {
                        builder = builder.with_rpc_port(rpc_port);
                    }
                }
                let config = builder.build(&mut OsRng, &network_config);
                nodes.insert(config.protocol_public_key(), Node::new(config));
            });
        }
        Swarm {
            dir,
            network_config,
            nodes,
            fullnode_config_builder,
        }
    }
}

/// A handle to an in-memory Sui Network.
#[derive(Debug)]
pub struct Swarm {
    dir: SwarmDirectory,
    network_config: NetworkConfig,
    nodes: HashMap<AuthorityName, Node>,
    // Save a copy of the fullnode config builder to build future fullnodes.
    fullnode_config_builder: FullnodeConfigBuilder,
}

impl Drop for Swarm {
    fn drop(&mut self) {
        self.nodes_iter_mut().for_each(|node| node.stop());
    }
}

impl Swarm {
    fn nodes_iter_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.nodes.values_mut()
    }

    /// Return a new Builder
    pub fn builder() -> SwarmBuilder {
        SwarmBuilder::new()
    }

    /// Start all nodes associated with this Swarm
    pub async fn launch(&mut self) -> Result<()> {
        try_join_all(self.nodes_iter_mut().map(|node| node.start())).await?;
        tracing::info!("Successfully launched Swarm");
        Ok(())
    }

    /// Return the path to the directory where this Swarm's on-disk data is kept.
    pub fn dir(&self) -> &Path {
        self.dir.as_ref()
    }

    /// Ensure that the Swarm data directory will persist and not be cleaned up when this Swarm is
    /// dropped.
    pub fn persist_dir(&mut self) {
        self.dir.persist();
    }

    /// Return a reference to this Swarm's `NetworkConfig`.
    pub fn config(&self) -> &NetworkConfig {
        &self.network_config
    }

    /// Return a mutable reference to this Swarm's `NetworkConfig`.
    // TODO: It's not ideal to mutate network config. We should consider removing this.
    pub fn config_mut(&mut self) -> &mut NetworkConfig {
        &mut self.network_config
    }

    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn node(&self, name: &AuthorityName) -> Option<&Node> {
        self.nodes.get(name)
    }

    pub fn node_mut(&mut self, name: &AuthorityName) -> Option<&mut Node> {
        self.nodes.get_mut(name)
    }

    /// Return an iterator over shared references of all nodes that are set up as validators.
    /// This means that they have a consensus config. This however doesn't mean this validator is
    /// currently active (i.e. it's not necessarily in the validator set at the moment).
    pub fn validator_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .values()
            .filter(|node| node.config.consensus_config.is_some())
    }

    pub fn validator_node_handles(&self) -> Vec<SuiNodeHandle> {
        self.validator_nodes()
            .map(|node| node.get_node_handle().unwrap())
            .collect()
    }

    /// Returns an iterator over all currently active validators.
    pub fn active_validators(&self) -> impl Iterator<Item = &Node> {
        self.validator_nodes().filter(|node| {
            node.get_node_handle().map_or(false, |handle| {
                let state = handle.state();
                state.is_validator(&state.epoch_store_for_testing())
            })
        })
    }

    /// Return an iterator over shared references of all Fullnodes.
    pub fn fullnodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .values()
            .filter(|node| node.config.consensus_config.is_none())
    }

    pub async fn spawn_new_node(&mut self, config: NodeConfig) -> SuiNodeHandle {
        let name = config.protocol_public_key();
        let node = Node::new(config);
        node.start().await.unwrap();
        let handle = node.get_node_handle().unwrap();
        self.nodes.insert(name, node);
        handle
    }

    pub fn get_fullnode_config_builder(&self) -> FullnodeConfigBuilder {
        self.fullnode_config_builder.clone()
    }
}

#[derive(Debug)]
enum SwarmDirectory {
    Persistent(PathBuf),
    Temporary(TempDir),
}

impl SwarmDirectory {
    fn persist(&mut self) {
        match self {
            SwarmDirectory::Persistent(_) => {}
            SwarmDirectory::Temporary(_) => {
                let mut temp = SwarmDirectory::Persistent(PathBuf::new());
                mem::swap(self, &mut temp);
                let _ = mem::replace(self, temp.into_persistent());
            }
        }
    }

    fn into_persistent(self) -> Self {
        match self {
            SwarmDirectory::Temporary(tempdir) => SwarmDirectory::Persistent(tempdir.into_path()),
            SwarmDirectory::Persistent(dir) => SwarmDirectory::Persistent(dir),
        }
    }
}

impl ops::Deref for SwarmDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            SwarmDirectory::Persistent(dir) => dir.deref(),
            SwarmDirectory::Temporary(dir) => dir.path(),
        }
    }
}

impl AsRef<Path> for SwarmDirectory {
    fn as_ref(&self) -> &Path {
        match self {
            SwarmDirectory::Persistent(dir) => dir.as_ref(),
            SwarmDirectory::Temporary(dir) => dir.as_ref(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::Swarm;
    use std::num::NonZeroUsize;

    #[tokio::test]
    async fn launch() {
        telemetry_subscribers::init_for_testing();
        let mut swarm = Swarm::builder()
            .committee_size(NonZeroUsize::new(4).unwrap())
            .with_fullnode_count(1)
            .build();

        swarm.launch().await.unwrap();

        for validator in swarm.validator_nodes() {
            validator.health_check(true).await.unwrap();
        }

        for fullnode in swarm.fullnodes() {
            fullnode.health_check(false).await.unwrap();
        }
    }
}

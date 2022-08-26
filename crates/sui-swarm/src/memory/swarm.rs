// Copyright (c) 2022, Mysten Labs, Inc.
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
use sui_config::builder::ConfigBuilder;
use sui_config::genesis_config::GenesisConfig;
use sui_config::NetworkConfig;
use sui_types::base_types::SuiAddress;
use tempfile::TempDir;

pub struct SwarmBuilder<R = OsRng> {
    rng: R,
    // template: NodeConfig,
    dir: Option<PathBuf>,
    committee_size: NonZeroUsize,
    initial_accounts_config: Option<GenesisConfig>,
    fullnode_count: usize,
    fullnode_rpc_addr: Option<SocketAddr>,
    websocket_rpc_addr: Option<SocketAddr>,
}

impl SwarmBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            rng: OsRng,
            dir: None,
            committee_size: NonZeroUsize::new(1).unwrap(),
            initial_accounts_config: None,
            fullnode_count: 0,
            fullnode_rpc_addr: None,
            websocket_rpc_addr: None,
        }
    }
}

impl<R> SwarmBuilder<R> {
    pub fn rng<N: ::rand::RngCore + ::rand::CryptoRng>(self, rng: N) -> SwarmBuilder<N> {
        SwarmBuilder {
            rng,
            dir: self.dir,
            committee_size: self.committee_size,
            initial_accounts_config: self.initial_accounts_config,
            fullnode_count: self.fullnode_count,
            fullnode_rpc_addr: self.fullnode_rpc_addr,
            websocket_rpc_addr: self.websocket_rpc_addr,
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

    /// Set the committe size (the number of validators in the validator set).
    ///
    /// Defaults to 1.
    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee_size = committee_size;
        self
    }

    pub fn initial_accounts_config(mut self, initial_accounts_config: GenesisConfig) -> Self {
        self.initial_accounts_config = Some(initial_accounts_config);
        self
    }

    pub fn with_fullnode_count(mut self, fullnode_count: usize) -> Self {
        self.fullnode_count = fullnode_count;
        self
    }

    pub fn with_fullnode_rpc_addr(mut self, fullnode_rpc_addr: SocketAddr) -> Self {
        self.fullnode_rpc_addr = Some(fullnode_rpc_addr);
        self
    }

    pub fn with_websocket_rpc_addr(mut self, websocket_rpc_addr: SocketAddr) -> Self {
        self.websocket_rpc_addr = Some(websocket_rpc_addr);
        self
    }
}

impl<R: ::rand::RngCore + ::rand::CryptoRng> SwarmBuilder<R> {
    /// Create the configured Swarm.
    pub fn build(self) -> Swarm {
        let dir = if let Some(dir) = self.dir {
            SwarmDirectory::Persistent(dir)
        } else {
            SwarmDirectory::Temporary(TempDir::new().unwrap())
        };

        let mut config_builder = ConfigBuilder::new(dir.as_ref());

        if let Some(initial_accounts_config) = self.initial_accounts_config {
            config_builder = config_builder.initial_accounts_config(initial_accounts_config);
        }

        let network_config = config_builder
            .committee_size(self.committee_size)
            .rng(self.rng)
            .build();

        let validators = network_config
            .validator_configs()
            .iter()
            .map(|config| (config.sui_address(), Node::new(config.to_owned())))
            .collect();

        let mut fullnodes = HashMap::new();

        if self.fullnode_count > 0 {
            (0..self.fullnode_count).for_each(|_| {
                let mut config = network_config.generate_fullnode_config();
                if let Some(fullnode_rpc_addr) = self.fullnode_rpc_addr {
                    config.json_rpc_address = fullnode_rpc_addr;
                }
                config.websocket_address = self.websocket_rpc_addr;
                fullnodes.insert(config.sui_address(), Node::new(config));
            });
        }
        Swarm {
            dir,
            network_config,
            validators,
            fullnodes,
        }
    }

    pub fn from_network_config(self, dir: PathBuf, network_config: NetworkConfig) -> Swarm {
        let dir = SwarmDirectory::Persistent(dir);

        let validators = network_config
            .validator_configs()
            .iter()
            .map(|config| (config.sui_address(), Node::new(config.to_owned())))
            .collect();

        Swarm {
            dir,
            network_config,
            validators,
            fullnodes: HashMap::new(),
        }
    }
}

/// A handle to an in-memory Sui Network.
#[derive(Debug)]
pub struct Swarm {
    dir: SwarmDirectory,
    network_config: NetworkConfig,
    validators: HashMap<SuiAddress, Node>,
    fullnodes: HashMap<SuiAddress, Node>,
}

impl Swarm {
    /// Return a new Builder
    pub fn builder() -> SwarmBuilder {
        SwarmBuilder::new()
    }

    /// Start all of the Validators associated with this Swarm
    pub async fn launch(&mut self) -> Result<()> {
        let nodes_iter = self
            .validators
            .values_mut()
            .chain(self.fullnodes.values_mut());
        let start_handles = nodes_iter
            .map(|node| node.spawn())
            .collect::<Result<Vec<_>>>()?;

        try_join_all(start_handles).await?;

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
    pub fn config_mut(&mut self) -> &mut NetworkConfig {
        &mut self.network_config
    }

    /// Attempt to lookup and return a shared reference to the Validator with the provided `name`.
    pub fn validator(&self, name: SuiAddress) -> Option<&Node> {
        self.validators.get(&name)
    }

    /// Attempt to lookup and return a mutable reference to the Validator with the provided `name`.
    pub fn validator_mut(&mut self, name: SuiAddress) -> Option<&mut Node> {
        self.validators.get_mut(&name)
    }

    /// Return an iterator over shared references of all Validators.
    pub fn validators(&self) -> impl Iterator<Item = &Node> {
        self.validators.values()
    }

    /// Return an iterator over mutable references of all Validators.
    pub fn validators_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.validators.values_mut()
    }

    /// Attempt to lookup and return a shared reference to the Fullnode with the provided `name`.
    pub fn fullnode(&self, name: SuiAddress) -> Option<&Node> {
        self.fullnodes.get(&name)
    }

    /// Attempt to lookup and return a mutable reference to the Fullnode with the provided `name`.
    pub fn fullnode_mut(&mut self, name: SuiAddress) -> Option<&mut Node> {
        self.fullnodes.get_mut(&name)
    }

    /// Return an iterator over shared references of all Fullnodes.
    pub fn fullnodes(&self) -> impl Iterator<Item = &Node> {
        self.fullnodes.values()
    }

    /// Return an iterator over mutable references of all Fullnodes.
    pub fn fullnodes_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.fullnodes.values_mut()
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

        for validator in swarm.validators() {
            validator.health_check().await.unwrap();
        }

        for fullnode in swarm.fullnodes() {
            fullnode.health_check().await.unwrap();
        }
    }
}

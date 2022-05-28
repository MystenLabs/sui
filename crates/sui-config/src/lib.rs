// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use multiaddr::Multiaddr;
use rand::rngs::OsRng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use sui_types::base_types::ObjectID;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair_from_rng, KeyPair};
use tracing::trace;

pub mod builder;
pub mod genesis;
pub mod genesis_config;
pub mod node;
pub mod utils;

pub use node::{CommitteeConfig, ConsensusConfig, NodeConfig, ValidatorInfo};

const SUI_DIR: &str = ".sui";
const SUI_CONFIG_DIR: &str = "sui_config";
pub const SUI_NETWORK_CONFIG: &str = "network.yaml";
pub const SUI_FULLNODE_CONFIG: &str = "fullnode.yaml";
pub const SUI_WALLET_CONFIG: &str = "wallet.yaml";
pub const SUI_GATEWAY_CONFIG: &str = "gateway.yaml";
pub const SUI_DEV_NET_URL: &str = "https://gateway.devnet.sui.io:443";

pub const AUTHORITIES_DB_NAME: &str = "authorities_db";
pub const CONSENSUS_DB_NAME: &str = "consensus_db";
pub const FULL_NODE_DB_PATH: &str = "full_node_db";

const DEFAULT_STAKE: usize = 1;

/// This is a config that is used for testing or local use as it contains the config and keys for
/// all validators
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub validator_configs: Vec<NodeConfig>,
    loaded_move_packages: Vec<(PathBuf, ObjectID)>,
    genesis: genesis::Genesis,
    pub account_keys: Vec<KeyPair>,
}

impl Config for NetworkConfig {}

impl NetworkConfig {
    pub fn validator_configs(&self) -> &[NodeConfig] {
        &self.validator_configs
    }

    pub fn loaded_move_packages(&self) -> &[(PathBuf, ObjectID)] {
        &self.loaded_move_packages
    }

    pub fn add_move_package(&mut self, path: PathBuf, object_id: ObjectID) {
        self.loaded_move_packages.push((path, object_id))
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        self.validator_configs()[0]
            .committee_config()
            .validator_set()
    }

    pub fn committee(&self) -> Committee {
        self.validator_configs()[0].committee_config().committee()
    }

    pub fn into_validator_configs(self) -> Vec<NodeConfig> {
        self.validator_configs
    }

    pub fn generate_with_rng<R: rand::CryptoRng + rand::RngCore>(
        config_dir: &Path,
        quorum_size: usize,
        rng: R,
    ) -> Self {
        builder::ConfigBuilder::new(config_dir)
            .committee_size(NonZeroUsize::new(quorum_size).unwrap())
            .rng(rng)
            .build()
    }

    pub fn generate(config_dir: &Path, quorum_size: usize) -> Self {
        Self::generate_with_rng(config_dir, quorum_size, OsRng)
    }

    /// Generate a fullnode config based on this `NetworkConfig`. This is useful if you want to run
    /// a fullnode and have it connect to a network defined by this `NetworkConfig`.
    pub fn generate_fullnode_config(&self) -> NodeConfig {
        let key_pair = get_key_pair_from_rng(&mut OsRng).1;
        let validator_config = &self.validator_configs[0];

        let mut db_path = validator_config.db_path.clone();
        db_path.pop();

        NodeConfig {
            key_pair,
            db_path: db_path.join("fullnode"),
            network_address: new_network_address(),
            metrics_address: new_network_address(),
            json_rpc_address: format!("127.0.0.1:{}", utils::get_available_port())
                .parse()
                .unwrap(),

            consensus_config: None,
            committee_config: validator_config.committee_config.clone(),

            genesis: validator_config.genesis.clone(),
        }
    }
}

fn new_network_address() -> Multiaddr {
    format!("/dns/localhost/tcp/{}/http", utils::get_available_port())
        .parse()
        .unwrap()
}

pub fn sui_config_dir() -> Result<PathBuf, anyhow::Error> {
    match std::env::var_os("SUI_CONFIG_DIR") {
        Some(config_env) => Ok(config_env.into()),
        None => match dirs::home_dir() {
            Some(v) => Ok(v.join(SUI_DIR).join(SUI_CONFIG_DIR)),
            None => anyhow::bail!("Cannot obtain home directory path"),
        },
    }
    .and_then(|dir| {
        if !dir.exists() {
            std::fs::create_dir_all(dir.clone())?;
        }
        Ok(dir)
    })
}

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn persisted(self, path: &Path) -> PersistedConfig<Self> {
        PersistedConfig {
            inner: self,
            path: path.to_path_buf(),
        }
    }
}

pub struct PersistedConfig<C> {
    inner: C,
    path: PathBuf,
}

impl<C> PersistedConfig<C>
where
    C: Config,
{
    pub fn read(path: &Path) -> Result<C, anyhow::Error> {
        trace!("Reading config from '{:?}'", path);
        let reader = fs::File::open(path)?;
        Ok(serde_yaml::from_reader(reader)?)
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", &self.path);
        let config = serde_yaml::to_string(&self.inner)?;
        fs::write(&self.path, config)?;
        Ok(())
    }

    pub fn into_inner(self) -> C {
        self.inner
    }
}

impl<C> std::ops::Deref for PersistedConfig<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<C> std::ops::DerefMut for PersistedConfig<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

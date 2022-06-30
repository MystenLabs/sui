// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::Config;
use anyhow::Result;
use debug_ignore::DebugIgnore;
use multiaddr::Multiaddr;
use narwhal_config::Parameters as ConsensusParameters;
use narwhal_config::SharedCommittee as ConsensusCommittee;
use narwhal_crypto::ed25519::Ed25519PublicKey;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_types::base_types::SuiAddress;
use sui_types::committee::StakeUnit;
use sui_types::crypto::{KeyPair, PublicKeyBytes};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    #[serde(default = "default_key_pair")]
    pub key_pair: Arc<KeyPair>,
    pub db_path: PathBuf,
    #[serde(default = "default_grpc_address")]
    pub network_address: Multiaddr,
    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_json_rpc_address")]
    pub json_rpc_address: SocketAddr,
    #[serde(default = "default_websocket_address")]
    pub websocket_address: Option<SocketAddr>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    #[serde(default)]
    pub enable_event_processing: bool,

    #[serde(default)]
    pub enable_gossip: bool,

    #[serde(default)]
    pub enable_reconfig: bool,

    pub genesis: Genesis,
}

fn default_key_pair() -> Arc<KeyPair> {
    Arc::new(sui_types::crypto::KeyPair::get_key_pair().1)
}

fn default_grpc_address() -> Multiaddr {
    use multiaddr::multiaddr;
    multiaddr!(Ip4([0, 0, 0, 0]), Tcp(8080u16))
}

fn default_metrics_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9184)
}

pub fn default_json_rpc_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000)
}

pub fn default_websocket_address() -> Option<SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr};
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9001))
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn key_pair(&self) -> &KeyPair {
        &self.key_pair
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        *self.key_pair.public_key_bytes()
    }

    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn consensus_config(&self) -> Option<&ConsensusConfig> {
        self.consensus_config.as_ref()
    }

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        self.genesis.genesis()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsensusConfig {
    pub consensus_address: Multiaddr,
    pub consensus_db_path: PathBuf,

    pub narwhal_config: ConsensusParameters,

    pub narwhal_committee: DebugIgnore<ConsensusCommittee<Ed25519PublicKey>>,
}

impl ConsensusConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.consensus_address
    }

    pub fn db_path(&self) -> &Path {
        &self.consensus_db_path
    }

    pub fn narwhal_config(&self) -> &ConsensusParameters {
        &self.narwhal_config
    }

    pub fn narwhal_committee(&self) -> &ConsensusCommittee<Ed25519PublicKey> {
        &self.narwhal_committee
    }
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfo {
    pub public_key: PublicKeyBytes,
    pub stake: StakeUnit,
    pub network_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        self.public_key
    }

    pub fn stake(&self) -> StakeUnit {
        self.stake
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
pub struct Genesis {
    #[serde(flatten)]
    location: GenesisLocation,

    #[serde(skip)]
    genesis: once_cell::sync::OnceCell<genesis::Genesis>,
}

impl Genesis {
    pub fn new(genesis: genesis::Genesis) -> Self {
        Self {
            location: GenesisLocation::InPlace { genesis },
            genesis: Default::default(),
        }
    }

    pub fn new_from_file<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            location: GenesisLocation::File {
                genesis_file_location: path.into(),
            },
            genesis: Default::default(),
        }
    }

    fn genesis(&self) -> Result<&genesis::Genesis> {
        match &self.location {
            GenesisLocation::InPlace { genesis } => Ok(genesis),
            GenesisLocation::File {
                genesis_file_location,
            } => self
                .genesis
                .get_or_try_init(|| genesis::Genesis::load(&genesis_file_location)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq)]
#[serde(untagged)]
enum GenesisLocation {
    InPlace {
        genesis: genesis::Genesis,
    },
    File {
        #[serde(rename = "genesis-file-location")]
        genesis_file_location: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::Genesis;
    use crate::{genesis, NodeConfig};

    #[test]
    fn serialize_genesis_config_from_file() {
        let g = Genesis::new_from_file("path/to/file");

        let s = serde_yaml::to_string(&g).unwrap();
        assert_eq!("---\ngenesis-file-location: path/to/file\n", s);
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn serialize_genesis_config_in_place() {
        let g = Genesis::new(genesis::Genesis::get_default_genesis());

        let mut s = serde_yaml::to_string(&g).unwrap();
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);

        // If both in-place and file location are provided, prefer the in-place variant
        s.push_str("\ngenesis-file-location: path/to/file");
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn load_genesis_config_from_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let genesis_config = Genesis::new_from_file(file.path());

        let genesis = genesis::Genesis::get_default_genesis();
        genesis.save(file.path()).unwrap();

        let loaded_genesis = genesis_config.genesis().unwrap();
        assert_eq!(&genesis, loaded_genesis);
    }

    #[test]
    fn fullnode_template() {
        const TEMPLATE: &str = include_str!("../data/fullnode-template.yaml");

        let _template: NodeConfig = serde_yaml::from_str(TEMPLATE).unwrap();
    }
}

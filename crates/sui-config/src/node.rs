// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::Config;
use anyhow::Result;
use multiaddr::Multiaddr;
use narwhal_config::Parameters as ConsensusParameters;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_types::base_types::SuiAddress;
use sui_types::committee::StakeUnit;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::AuthoritySignature;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::PublicKey as AccountsPublicKey;
use sui_types::crypto::SuiKeyPair;
use sui_types::sui_serde::{AuthSignature, KeyPairBase64};

// Default max number of concurrent requests served
pub const DEFAULT_GRPC_CONCURRENCY_LIMIT: usize = 20000;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    /// The keypair that is used to deal with consensus transactions
    #[serde(default = "default_key_pair")]
    #[serde_as(as = "Arc<KeyPairBase64>")]
    pub key_pair: Arc<AuthorityKeyPair>,
    /// The keypair that the authority uses to receive payments
    #[serde(default = "default_sui_key_pair")]
    pub account_key_pair: Arc<SuiKeyPair>,
    #[serde(default = "default_sui_key_pair")]
    pub network_key_pair: Arc<SuiKeyPair>,
    // pub proof_of_possession: Arc<>,
    pub db_path: PathBuf,
    #[serde(default = "default_grpc_address")]
    pub network_address: Multiaddr,
    #[serde(default = "default_json_rpc_address")]
    pub json_rpc_address: SocketAddr,
    #[serde(default = "default_websocket_address")]
    pub websocket_address: Option<SocketAddr>,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_admin_interface_port")]
    pub admin_interface_port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    #[serde(default)]
    pub enable_event_processing: bool,

    #[serde(default)]
    pub enable_gossip: bool,

    #[serde(default)]
    pub enable_reconfig: bool,

    #[serde(default)]
    pub grpc_load_shed: Option<bool>,

    #[serde(default = "default_concurrency_limit")]
    pub grpc_concurrency_limit: Option<usize>,

    pub genesis: Genesis,
}

fn default_key_pair() -> Arc<AuthorityKeyPair> {
    Arc::new(sui_types::crypto::get_key_pair().1)
}

fn default_sui_key_pair() -> Arc<SuiKeyPair> {
    Arc::new((sui_types::crypto::get_key_pair::<AccountKeyPair>().1).into())
}

fn default_grpc_address() -> Multiaddr {
    use multiaddr::multiaddr;
    multiaddr!(Ip4([0, 0, 0, 0]), Tcp(8080u16))
}

fn default_metrics_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9184)
}

pub fn default_admin_interface_port() -> u16 {
    1337
}

pub fn default_json_rpc_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000)
}

pub fn default_websocket_address() -> Option<SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr};
    Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9001))
}

pub fn default_concurrency_limit() -> Option<usize> {
    Some(DEFAULT_GRPC_CONCURRENCY_LIMIT)
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn key_pair(&self) -> &AuthorityKeyPair {
        &self.key_pair
    }

    pub fn public_key(&self) -> AuthorityPublicKeyBytes {
        self.key_pair.public().into()
    }

    pub fn sui_address(&self) -> SuiAddress {
        (&self.public_key()).into()
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
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfo {
    pub name: String,
    pub public_key: AuthorityPublicKeyBytes,
    pub network_key: AccountsPublicKey,
    #[serde_as(as = "AuthSignature")]
    pub proof_of_possession: AuthoritySignature,
    pub stake: StakeUnit,
    pub delegation: StakeUnit,
    pub gas_price: u64,
    pub network_address: Multiaddr,
    pub narwhal_primary_to_primary: Multiaddr,

    //TODO remove all of these as they shouldn't be needed to be encoded in genesis
    pub narwhal_worker_to_primary: Multiaddr,
    pub narwhal_primary_to_worker: Multiaddr,
    pub narwhal_worker_to_worker: Multiaddr,
    pub narwhal_consensus_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sui_address(&self) -> SuiAddress {
        (&self.public_key()).into()
    }

    pub fn public_key(&self) -> AuthorityPublicKeyBytes {
        self.public_key
    }

    pub fn network_key(&self) -> &AccountsPublicKey {
        &self.network_key
    }

    pub fn proof_of_possession(&self) -> &AuthoritySignature {
        &self.proof_of_possession
    }

    pub fn stake(&self) -> StakeUnit {
        self.stake
    }

    pub fn delegation(&self) -> StakeUnit {
        self.delegation
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn voting_rights(validator_set: &[Self]) -> BTreeMap<AuthorityPublicKeyBytes, u64> {
        validator_set
            .iter()
            .map(|validator| {
                (
                    validator.public_key(),
                    validator.stake() + validator.delegation(),
                )
            })
            .collect()
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

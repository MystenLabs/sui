// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis;
use crate::p2p::P2pConfig;
use crate::Config;
use anyhow::anyhow;
use anyhow::Result;
use fastcrypto::traits::ToFromBytes;
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
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::NetworkKeyPair;
use sui_types::crypto::NetworkPublicKey;
use sui_types::crypto::PublicKey as AccountsPublicKey;
use sui_types::crypto::SuiKeyPair;
use sui_types::crypto::{AccountKeyPair, AuthorityPublicKey};
use sui_types::sui_serde::KeyPairBase64;

// Default max number of concurrent requests served
pub const DEFAULT_GRPC_CONCURRENCY_LIMIT: usize = 20000000000;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    /// The keypair that is used to deal with consensus transactions
    #[serde(default = "default_key_pair")]
    #[serde_as(as = "Arc<KeyPairBase64>")]
    pub protocol_key_pair: Arc<AuthorityKeyPair>,
    /// The keypair that is used by the narwhal worker.
    #[serde(default = "default_worker_key_pair")]
    #[serde_as(as = "Arc<KeyPairBase64>")]
    pub worker_key_pair: Arc<NetworkKeyPair>,
    /// The keypair that the authority uses to receive payments
    #[serde(default = "default_sui_key_pair")]
    pub account_key_pair: Arc<SuiKeyPair>,
    #[serde(default = "default_worker_key_pair")]
    #[serde_as(as = "Arc<KeyPairBase64>")]
    pub network_key_pair: Arc<NetworkKeyPair>,
    pub db_path: PathBuf,
    #[serde(default = "default_grpc_address")]
    pub network_address: Multiaddr,
    #[serde(default = "default_json_rpc_address")]
    pub json_rpc_address: SocketAddr,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
    #[serde(default = "default_admin_interface_port")]
    pub admin_interface_port: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_config: Option<ConsensusConfig>,

    #[serde(default)]
    pub enable_event_processing: bool,

    #[serde(default)]
    pub enable_checkpoint: bool,

    #[serde(default)]
    pub enable_reconfig: bool,

    #[serde(default)]
    pub grpc_load_shed: Option<bool>,

    #[serde(default = "default_concurrency_limit")]
    pub grpc_concurrency_limit: Option<usize>,

    #[serde(default)]
    pub p2p_config: P2pConfig,

    pub genesis: Genesis,
}

fn default_key_pair() -> Arc<AuthorityKeyPair> {
    Arc::new(sui_types::crypto::get_key_pair().1)
}

fn default_worker_key_pair() -> Arc<NetworkKeyPair> {
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

pub fn bool_true() -> bool {
    true
}

impl Config for NodeConfig {}

impl NodeConfig {
    pub fn protocol_key_pair(&self) -> &AuthorityKeyPair {
        &self.protocol_key_pair
    }

    pub fn worker_key_pair(&self) -> &NetworkKeyPair {
        &self.worker_key_pair
    }

    pub fn network_key_pair(&self) -> &NetworkKeyPair {
        &self.network_key_pair
    }

    pub fn protocol_public_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key_pair.public().into()
    }

    pub fn sui_address(&self) -> SuiAddress {
        (&self.account_key_pair.public()).into()
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

    #[allow(clippy::mutable_key_type)]
    pub fn narwhal_worker_cache(&self) -> Result<narwhal_config::SharedWorkerCache> {
        let genesis = self.genesis()?;
        let consensus_config = self
            .consensus_config()
            .ok_or_else(|| anyhow!("cannot generate worker cache without ConsensusConfig"))?;
        let workers = genesis
            .validator_set()
            .iter()
            .map(|validator| {
                let name = AuthorityPublicKey::from_bytes(validator.protocol_key().as_ref())
                    .expect("Can't get protocol key");
                let worker_address = if name != *self.protocol_key_pair().public() {
                    validator.narwhal_worker_address.clone()
                } else {
                    // Use internal worker addresses for our own node if configured.
                    consensus_config
                        .internal_worker_address
                        .as_ref()
                        .unwrap_or(&validator.narwhal_worker_address)
                        .clone()
                };
                let workers = [(
                    0, // worker_id
                    narwhal_config::WorkerInfo {
                        name: NetworkPublicKey::from_bytes(validator.worker_key().as_ref())
                            .expect("Can't get worker key"),
                        transactions: consensus_config.address.clone(),
                        worker_address,
                    },
                )]
                .into_iter()
                .collect();
                let worker_index = narwhal_config::WorkerIndex(workers);

                (name, worker_index)
            })
            .collect();
        Ok(narwhal_config::WorkerCache {
            workers,
            epoch: genesis.epoch() as narwhal_config::Epoch,
        }
        .into())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsensusConfig {
    pub address: Multiaddr,
    pub db_path: PathBuf,

    // Optional alternative address preferentially used by a primary to talk to its own worker.
    // For example, this could be used to connect to co-located workers over a private LAN address.
    pub internal_worker_address: Option<Multiaddr>,

    // Timeout to retry sending transaction to consensus internally.
    // Default to 60s.
    pub timeout_secs: Option<u64>,

    pub narwhal_config: ConsensusParameters,
}

impl ConsensusConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
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
    pub account_key: AccountsPublicKey,
    pub protocol_key: AuthorityPublicKeyBytes,
    pub worker_key: NetworkPublicKey,
    pub network_key: NetworkPublicKey,
    pub stake: StakeUnit,
    pub delegation: StakeUnit,
    pub gas_price: u64,
    pub commission_rate: u64,
    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sui_address(&self) -> SuiAddress {
        self.account_key().into()
    }

    pub fn protocol_key(&self) -> AuthorityPublicKeyBytes {
        self.protocol_key
    }

    pub fn worker_key(&self) -> &NetworkPublicKey {
        &self.worker_key
    }

    pub fn network_key(&self) -> &NetworkPublicKey {
        &self.network_key
    }

    pub fn account_key(&self) -> &AccountsPublicKey {
        &self.account_key
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

    pub fn commission_rate(&self) -> u64 {
        self.commission_rate
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn narwhal_primary_address(&self) -> &Multiaddr {
        &self.narwhal_primary_address
    }

    pub fn narwhal_worker_address(&self) -> &Multiaddr {
        &self.narwhal_worker_address
    }

    pub fn p2p_address(&self) -> &Multiaddr {
        &self.p2p_address
    }

    pub fn voting_rights(validator_set: &[Self]) -> BTreeMap<AuthorityPublicKeyBytes, u64> {
        validator_set
            .iter()
            .map(|validator| {
                (
                    validator.protocol_key(),
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

    pub fn genesis(&self) -> Result<&genesis::Genesis> {
        match &self.location {
            GenesisLocation::InPlace { genesis } => Ok(genesis),
            GenesisLocation::File {
                genesis_file_location,
            } => self
                .genesis
                .get_or_try_init(|| genesis::Genesis::load(genesis_file_location)),
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

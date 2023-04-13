// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::node::{default_enable_index_processing, AuthorityStorePruningConfig};
use crate::node::{
    default_end_of_epoch_broadcast_channel_capacity, AuthorityKeyPairWithPath, DBCheckpointConfig,
    KeyPairWithPath,
};
use crate::p2p::{P2pConfig, SeedPeer};
use crate::{
    builder::{self, ProtocolVersionsConfig, SupportedProtocolVersionsCallback},
    genesis, utils, Config, NodeConfig,
};
use fastcrypto::traits::KeyPair;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use sui_protocol_config::SupportedProtocolVersions;
use sui_types::committee::CommitteeWithNetworkMetadata;
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair,
};
use sui_types::multiaddr::Multiaddr;

/// This is a config that is used for testing or local use as it contains the config and keys for
/// all validators
#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub validator_configs: Vec<NodeConfig>,
    pub account_keys: Vec<AccountKeyPair>,
    pub genesis: genesis::Genesis,
}

impl Config for NetworkConfig {}

impl NetworkConfig {
    pub fn validator_configs(&self) -> &[NodeConfig] {
        &self.validator_configs
    }

    pub fn net_addresses(&self) -> Vec<Multiaddr> {
        self.genesis
            .committee_with_network()
            .network_metadata
            .into_values()
            .map(|n| n.network_address)
            .collect()
    }

    pub fn committee_with_network(&self) -> CommitteeWithNetworkMetadata {
        self.genesis.committee_with_network()
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

    pub fn fullnode_config_builder(&self) -> FullnodeConfigBuilder<'_> {
        FullnodeConfigBuilder::new(self)
    }
}

pub struct FullnodeConfigBuilder<'a> {
    network_config: &'a NetworkConfig,
    dir: Option<PathBuf>,
    enable_event_store: bool,
    listen_ip: Option<IpAddr>,
    // port for main network_address
    port: Option<u16>,
    // port for p2p data sync
    p2p_port: Option<u16>,
    // port for json rpc api
    rpc_port: Option<u16>,
    // port for admin interface
    admin_port: Option<u16>,
    supported_protocol_versions_config: ProtocolVersionsConfig,
    db_checkpoint_config: DBCheckpointConfig,
}

impl<'a> FullnodeConfigBuilder<'a> {
    fn new(network_config: &'a NetworkConfig) -> Self {
        Self {
            network_config,
            dir: None,
            enable_event_store: false,
            listen_ip: None,
            port: None,
            p2p_port: None,
            rpc_port: None,
            admin_port: None,
            supported_protocol_versions_config: ProtocolVersionsConfig::Default,
            db_checkpoint_config: DBCheckpointConfig::default(),
        }
    }

    // The EventStore uses a non-deterministic async pool which breaks determinism in
    // the simulator, so do not enable with_event_store in tests unless the test specifically
    // requires events.
    // TODO: In the simulator, we may be able to run event store in a separate thread and make
    // blocking calls to it to fix this.
    pub fn with_event_store(mut self) -> Self {
        self.enable_event_store = true;
        self
    }

    pub fn with_listen_ip(mut self, ip: IpAddr) -> Self {
        self.listen_ip = Some(ip);
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_p2p_port(mut self, port: u16) -> Self {
        self.p2p_port = Some(port);
        self
    }

    pub fn with_rpc_port(mut self, port: u16) -> Self {
        self.rpc_port = Some(port);
        self
    }

    pub fn set_rpc_port(mut self, port: Option<u16>) -> Self {
        self.rpc_port = port;
        self
    }

    pub fn with_admin_port(mut self, port: u16) -> Self {
        self.admin_port = Some(port);
        self
    }

    pub fn set_event_store(mut self, status: bool) -> Self {
        self.enable_event_store = status;
        self
    }

    pub fn with_dir(mut self, dir: PathBuf) -> Self {
        self.dir = Some(dir);
        self
    }

    pub fn with_random_dir(mut self) -> Self {
        self.dir = None;
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

    pub fn with_db_checkpoint_config(mut self, db_checkpoint_config: DBCheckpointConfig) -> Self {
        self.db_checkpoint_config = db_checkpoint_config;
        self
    }

    pub fn build(self) -> Result<NodeConfig, anyhow::Error> {
        let protocol_key_pair = get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut OsRng).1;
        let worker_key_pair = get_key_pair_from_rng::<NetworkKeyPair, _>(&mut OsRng).1;
        let account_key_pair = get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng).1;
        let network_key_pair = get_key_pair_from_rng::<NetworkKeyPair, _>(&mut OsRng).1;
        let validator_configs = &self.network_config.validator_configs;
        let validator_config = &validator_configs[0];

        let mut db_path = validator_config.db_path.clone();
        db_path.pop();

        let dir_name = self
            .dir
            .unwrap_or_else(|| OsRng.next_u32().to_string().into());

        let listen_ip = self.listen_ip.unwrap_or_else(utils::get_local_ip_for_tests);
        let listen_ip_str = format!("{}", listen_ip);

        let get_available_port = |public_port| {
            if listen_ip.is_loopback() || listen_ip == utils::get_local_ip_for_tests() {
                utils::get_available_port(&listen_ip_str)
            } else {
                public_port
            }
        };

        let network_address = format!(
            "/ip4/{}/tcp/{}/http",
            listen_ip,
            self.port.unwrap_or_else(|| get_available_port(8080))
        )
        .parse()
        .unwrap();

        let p2p_config = {
            let address = SocketAddr::new(
                listen_ip,
                self.p2p_port.unwrap_or_else(|| get_available_port(8084)),
            );
            let seed_peers = validator_configs
                .iter()
                .map(|config| SeedPeer {
                    peer_id: Some(anemo::PeerId(
                        config.network_key_pair().public().0.to_bytes(),
                    )),
                    address: config.p2p_config.external_address.clone().unwrap(),
                })
                .collect();

            P2pConfig {
                listen_address: address,
                external_address: Some(utils::socket_address_to_udp_multiaddr(address)),
                seed_peers,
                ..Default::default()
            }
        };

        let rpc_port = self.rpc_port.unwrap_or_else(|| get_available_port(9000));
        let jsonrpc_server_url = format!("{}:{}", listen_ip, rpc_port);
        let json_rpc_address: SocketAddr = jsonrpc_server_url.parse().unwrap();

        let supported_protocol_versions = match &self.supported_protocol_versions_config {
            ProtocolVersionsConfig::Default => SupportedProtocolVersions::SYSTEM_DEFAULT,
            ProtocolVersionsConfig::Global(v) => *v,
            ProtocolVersionsConfig::PerValidator(func) => func(0, None),
        };

        Ok(NodeConfig {
            protocol_key_pair: AuthorityKeyPairWithPath::new(protocol_key_pair),
            account_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(account_key_pair)),
            worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(worker_key_pair)),
            network_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(network_key_pair)),

            db_path: db_path.join(dir_name),
            network_address,
            metrics_address: utils::available_local_socket_address(),
            // TODO: admin server is hard coded to start on 127.0.0.1 - we should probably
            // provide the entire socket address here to avoid confusion.
            admin_interface_port: self.admin_port.unwrap_or_else(|| get_available_port(8888)),
            json_rpc_address,
            consensus_config: None,
            enable_event_processing: self.enable_event_store,
            enable_index_processing: default_enable_index_processing(),
            genesis: validator_config.genesis.clone(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            p2p_config,
            authority_store_pruning_config: AuthorityStorePruningConfig::fullnode_config(),
            end_of_epoch_broadcast_channel_capacity:
                default_end_of_epoch_broadcast_channel_capacity(),
            checkpoint_executor_config: Default::default(),
            metrics: None,
            supported_protocol_versions: Some(supported_protocol_versions),
            db_checkpoint_config: self.db_checkpoint_config,
            indirect_objects_threshold: usize::MAX,
            // Copy the expensive safety check config from the first validator config.
            expensive_safety_check_config: validator_config.expensive_safety_check_config.clone(),
            name_service_resolver_object_id: None,
        })
    }
}

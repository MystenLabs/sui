// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::node::default_checkpoints_per_epoch;
use crate::p2p::{P2pConfig, SeedPeer};
use crate::{builder, genesis, utils, Config, NodeConfig, ValidatorInfo};
use fastcrypto::traits::KeyPair;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_types::committee::Committee;
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair,
};
use sui_types::sui_serde::KeyPairBase64;

/// This is a config that is used for testing or local use as it contains the config and keys for
/// all validators
#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub validator_configs: Vec<NodeConfig>,
    #[serde_as(as = "Vec<KeyPairBase64>")]
    pub account_keys: Vec<AccountKeyPair>,
    pub genesis: genesis::Genesis,
}

impl Config for NetworkConfig {}

impl NetworkConfig {
    pub fn validator_configs(&self) -> &[NodeConfig] {
        &self.validator_configs
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        self.genesis.validator_set()
    }

    pub fn committee(&self) -> Committee {
        self.genesis.committee().unwrap()
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
    rpc_port: Option<u16>,
}

impl<'a> FullnodeConfigBuilder<'a> {
    fn new(network_config: &'a NetworkConfig) -> Self {
        Self {
            network_config,
            dir: None,
            enable_event_store: false,
            listen_ip: None,
            rpc_port: None,
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

    pub fn with_rpc_port(mut self, port: u16) -> Self {
        self.rpc_port = Some(port);
        self
    }

    pub fn set_rpc_port(mut self, port: Option<u16>) -> Self {
        self.rpc_port = port;
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

    pub fn build(self) -> Result<NodeConfig, anyhow::Error> {
        let protocol_key_pair: Arc<AuthorityKeyPair> =
            Arc::new(get_key_pair_from_rng(&mut OsRng).1);
        let worker_key_pair: Arc<NetworkKeyPair> = Arc::new(get_key_pair_from_rng(&mut OsRng).1);
        let account_key_pair: Arc<SuiKeyPair> = Arc::new(
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng)
                .1
                .into(),
        );
        let network_key_pair: Arc<NetworkKeyPair> = Arc::new(get_key_pair_from_rng(&mut OsRng).1);
        let validator_configs = &self.network_config.validator_configs;
        let validator_config = &validator_configs[0];

        let mut db_path = validator_config.db_path.clone();
        db_path.pop();

        let dir_name = self
            .dir
            .unwrap_or_else(|| OsRng.next_u32().to_string().into());

        let listen_ip = self.listen_ip.unwrap_or_else(utils::get_local_ip_for_tests);

        let network_address = format!(
            "/ip4/{}/tcp/{}/http",
            listen_ip,
            utils::get_available_port()
        )
        .parse()
        .unwrap();

        let p2p_config = {
            let address = utils::available_local_socket_address();
            let seed_peers = validator_configs
                .iter()
                .map(|config| SeedPeer {
                    peer_id: Some(anemo::PeerId(config.network_key_pair.public().0.to_bytes())),
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

        let rpc_port = self.rpc_port.unwrap_or_else(utils::get_available_port);
        let jsonrpc_server_url = format!("{}:{}", listen_ip, rpc_port);
        let json_rpc_address: SocketAddr = jsonrpc_server_url.parse().unwrap();

        Ok(NodeConfig {
            protocol_key_pair,
            worker_key_pair,
            account_key_pair,
            network_key_pair,
            db_path: db_path.join(dir_name),
            network_address,
            metrics_address: utils::available_local_socket_address(),
            admin_interface_port: utils::get_available_port(),
            json_rpc_address,
            consensus_config: None,
            enable_event_processing: self.enable_event_store,
            enable_checkpoint: false,
            checkpoints_per_epoch: default_checkpoints_per_epoch(),
            genesis: validator_config.genesis.clone(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            p2p_config,
        })
    }
}

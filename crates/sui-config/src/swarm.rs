// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{builder, genesis, utils, Config, NodeConfig, ValidatorInfo, FULL_NODE_DB_PATH};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, SuiKeyPair};
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

    pub fn generate_fullnode_config(&self) -> NodeConfig {
        self.generate_fullnode_config_with_custom_db_path(None, true)
    }

    /// Generate a fullnode config based on this `NetworkConfig`. This is useful if you want to run
    /// a fullnode and have it connect to a network defined by this `NetworkConfig`.
    pub fn generate_fullnode_config_with_custom_db_path(
        &self,
        fullnode_db_dir: Option<&str>,
        enable_websocket: bool,
    ) -> NodeConfig {
        let protocol_key_pair: Arc<AuthorityKeyPair> =
            Arc::new(get_key_pair_from_rng(&mut OsRng).1);
        let account_key_pair: Arc<SuiKeyPair> = Arc::new(
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng)
                .1
                .into(),
        );
        let network_key_pair: Arc<SuiKeyPair> = Arc::new(
            get_key_pair_from_rng::<AccountKeyPair, _>(&mut OsRng)
                .1
                .into(),
        );
        let validator_config = &self.validator_configs[0];

        let mut db_path = validator_config.db_path.clone();
        db_path.pop();

        NodeConfig {
            protocol_key_pair,
            account_key_pair,
            network_key_pair,
            db_path: db_path.join(fullnode_db_dir.unwrap_or(FULL_NODE_DB_PATH)),
            network_address: utils::new_network_address(),
            metrics_address: utils::available_local_socket_address(),
            admin_interface_port: utils::get_available_port(),
            json_rpc_address: utils::available_local_socket_address(),
            websocket_address: if enable_websocket {
                Some(utils::available_local_socket_address())
            } else {
                None
            },
            consensus_config: None,
            enable_event_processing: true,
            enable_gossip: true,
            enable_checkpoint: true,
            enable_reconfig: false,
            genesis: validator_config.genesis.clone(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
        }
    }
}

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{builder, genesis, utils, Config, NodeConfig, ValidatorInfo, FULL_NODE_DB_PATH};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use sui_types::committee::Committee;
use sui_types::crypto::{KeyPair};

/// This is a config that is used for testing or local use as it contains the config and keys for
/// all validators
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub validator_configs: Vec<NodeConfig>,
    pub account_keys: Vec<KeyPair>,
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

    /// Generate a fullnode config based on this `NetworkConfig`. This is useful if you want to run
    /// a fullnode and have it connect to a network defined by this `NetworkConfig`.
    pub fn generate_fullnode_config(&self) -> NodeConfig {
        let key_pair = Arc::new(KeyPair::get_key_pair_from_rng(&mut OsRng).1);
        let validator_config = &self.validator_configs[0];

        let mut db_path = validator_config.db_path.clone();
        db_path.pop();

        NodeConfig {
            key_pair,
            db_path: db_path.join(FULL_NODE_DB_PATH),
            network_address: utils::new_network_address(),
            metrics_address: utils::available_local_socket_address(),
            json_rpc_address: utils::available_local_socket_address(),
            websocket_address: Some(utils::available_local_socket_address()),
            consensus_config: None,
            enable_event_processing: true,
            enable_gossip: true,
            enable_reconfig: false,
            genesis: validator_config.genesis.clone(),
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis_config::ValidatorGenesisConfig;
use std::path::PathBuf;
use sui_config::node::{AuthorityKeyPairWithPath, KeyPairWithPath, DEFAULT_VALIDATOR_GAS_PRICE};
use sui_config::NodeConfig;
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, NetworkKeyPair, SuiKeyPair,
};

#[derive(Debug, Default)]
pub struct NodeConfigBuilder {
    validator_genesis_config: Option<ValidatorGenesisConfig>,
    db_path: Option<PathBuf>,
    validator_rgp: Option<u64>,
    #[cfg(msim)]
    node_index: Option<u8>,
}

impl NodeConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_validator_genesis_config(
        mut self,
        validator_genesis_config: ValidatorGenesisConfig,
    ) -> Self {
        assert!(self.account_keypair.is_none());
        assert!(self.validator_genesis_config.is_none());
        self.validator_genesis_config = Some(validator_genesis_config);
        self
    }

    pub fn with_db_path(mut self, db_path: PathBuf) -> Self {
        assert!(self.db_path.is_none());
        self.db_path = Some(db_path);
        self
    }

    #[cfg(msim)]
    pub fn with_node_index(mut self, node_index: usize) -> Self {
        assert!(self.node_index.is_none());
        self.node_index = Some(node_index);
        self
    }

    pub fn build_with_rng<R: rand::RngCore + rand::CryptoRng>(mut self, &mut rng: R) -> NodeConfig {
        let validator_genesis_config = self.validator_genesis_config.unwrap_or_else(|| {
            let validator_rgp = self.validator_rgp.unwrap_or(DEFAULT_VALIDATOR_GAS_PRICE);
            let protocol_key_pair = get_key_pair_from_rng::<AuthorityKeyPair, _>(rng).1;
            let worker_key_pair = get_key_pair_from_rng::<NetworkKeyPair, _>(rng).1;
            let account_key_pair = get_key_pair_from_rng::<AccountKeyPair, _>(rng).1.into();
            let network_key_pair = get_key_pair_from_rng::<NetworkKeyPair, _>(rng).1;
            #[cfg(msim)]
            {
                let node_index = self.node_index.expect("Node index must be specified in simtest mode in order to construct unique IP addresses");
                // we will probably never run this many validators in a sim
                let low_octet = node_index + 1;
                if low_octet > 255 {
                    todo!("smarter IP formatting required");
                }
                let ip = format!("10.10.0.{}", low_octet);

                ValidatorGenesisConfig::from_base_ip(
                    protocol_key_pair,
                    worker_key_pair,
                    account_key_pair,
                    network_key_pair,
                    None,
                    ip,
                    node_index,
                    validator_rgp,
                )
            }
            #[cfg(not(msim))]
            ValidatorGenesisConfig::from_localhost_for_testing(
                protocol_key_pair,
                worker_key_pair,
                account_key_pair,
                network_key_pair,
                validator_rgp,
            )
        });
        let db_path = self
            .db_path
            .unwrap_or_else(|| tempfile::TempDir::new().unwrap().into());
        NodeConfig {
            protocol_key_pair: AuthorityKeyPairWithPath::new(validator_genesis_config.key_pair),
            network_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                validator_genesis_config.network_key_pair,
            )),
            account_key_pair: KeyPairWithPath::new(validator_genesis_config.account_key_pair),
            worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                validator_genesis_config.worker_key_pair,
            )),
            db_path,
            network_address: validator_genesis_config.network_address,
            metrics_address: validator_genesis_config.metrics_address,
            // TODO: admin server is hard coded to start on 127.0.0.1 - we should probably
            // provide the entire socket address here to avoid confusion.
            admin_interface_port: match self.validator_ip_sel {
                ValidatorIpSelection::Simulator => 8888,
                _ => utils::get_available_port("127.0.0.1"),
            },
            json_rpc_address: utils::available_local_socket_address(),
            consensus_config: Some(consensus_config),
            enable_event_processing: false,
            enable_index_processing: default_enable_index_processing(),
            genesis: sui_config::node::Genesis::new(genesis.clone()),
            grpc_load_shed: None,
            grpc_concurrency_limit: Some(DEFAULT_GRPC_CONCURRENCY_LIMIT),
            p2p_config,
            authority_store_pruning_config: AuthorityStorePruningConfig::validator_config(),
            end_of_epoch_broadcast_channel_capacity:
                default_end_of_epoch_broadcast_channel_capacity(),
            checkpoint_executor_config: Default::default(),
            metrics: None,
            supported_protocol_versions: Some(supported_protocol_versions),
            db_checkpoint_config: self.db_checkpoint_config.clone(),
            indirect_objects_threshold: usize::MAX,
            expensive_safety_check_config: ExpensiveSafetyCheckConfig::new_enable_all(),
            name_service_resolver_object_id: None,
            transaction_deny_config: Default::default(),
            certificate_deny_config: Default::default(),
            state_debug_dump_config: self.state_debug_dump_config.clone(),
        }
    }
}

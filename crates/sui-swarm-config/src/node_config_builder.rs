// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::traits::KeyPair;
use sui_config::node::{
    default_enable_index_processing, default_end_of_epoch_broadcast_channel_capacity,
    default_local_execution_time_channel_capacity, AuthorityKeyPairWithPath,
    AuthorityOverloadConfig, AuthorityStorePruningConfig, CheckpointExecutorConfig,
    DBCheckpointConfig, ExecutionCacheConfig, ExpensiveSafetyCheckConfig, Genesis, KeyPairWithPath,
    StateArchiveConfig, StateSnapshotConfig, DEFAULT_GRPC_CONCURRENCY_LIMIT,
};
use sui_config::node::{default_zklogin_oauth_providers, RunWithRange};
use sui_config::p2p::{P2pConfig, SeedPeer, StateSyncConfig};
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_config::{
    local_ip_utils, ConsensusConfig, NodeConfig, AUTHORITIES_DB_NAME, CONSENSUS_DB_NAME,
    FULL_NODE_DB_PATH,
};
use sui_types::crypto::{AuthorityKeyPair, AuthorityPublicKeyBytes, NetworkKeyPair, SuiKeyPair};
use sui_types::multiaddr::Multiaddr;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::traffic_control::{PolicyConfig, RemoteFirewallConfig};

use crate::genesis_config::{ValidatorGenesisConfig, ValidatorGenesisConfigBuilder};
use crate::network_config::NetworkConfig;

/// This builder contains information that's not included in ValidatorGenesisConfig for building
/// a validator NodeConfig. It can be used to build either a genesis validator or a new validator.
#[derive(Clone, Default)]
pub struct ValidatorConfigBuilder {
    config_directory: Option<PathBuf>,
    supported_protocol_versions: Option<SupportedProtocolVersions>,
    force_unpruned_checkpoints: bool,
    jwk_fetch_interval: Option<Duration>,
    authority_overload_config: Option<AuthorityOverloadConfig>,
    execution_cache_config: Option<ExecutionCacheConfig>,
    data_ingestion_dir: Option<PathBuf>,
    policy_config: Option<PolicyConfig>,
    firewall_config: Option<RemoteFirewallConfig>,
    max_submit_position: Option<usize>,
    submit_delay_step_override_millis: Option<u64>,
    state_accumulator_v2: bool,
}

impl ValidatorConfigBuilder {
    pub fn new() -> Self {
        Self {
            state_accumulator_v2: true,
            ..Default::default()
        }
    }

    pub fn with_config_directory(mut self, config_directory: PathBuf) -> Self {
        assert!(self.config_directory.is_none());
        self.config_directory = Some(config_directory);
        self
    }

    pub fn with_supported_protocol_versions(
        mut self,
        supported_protocol_versions: SupportedProtocolVersions,
    ) -> Self {
        assert!(self.supported_protocol_versions.is_none());
        self.supported_protocol_versions = Some(supported_protocol_versions);
        self
    }

    pub fn with_unpruned_checkpoints(mut self) -> Self {
        self.force_unpruned_checkpoints = true;
        self
    }

    pub fn with_jwk_fetch_interval(mut self, i: Duration) -> Self {
        self.jwk_fetch_interval = Some(i);
        self
    }

    pub fn with_authority_overload_config(mut self, config: AuthorityOverloadConfig) -> Self {
        self.authority_overload_config = Some(config);
        self
    }

    pub fn with_execution_cache_config(mut self, config: ExecutionCacheConfig) -> Self {
        self.execution_cache_config = Some(config);
        self
    }

    pub fn with_data_ingestion_dir(mut self, path: PathBuf) -> Self {
        self.data_ingestion_dir = Some(path);
        self
    }

    pub fn with_policy_config(mut self, config: Option<PolicyConfig>) -> Self {
        self.policy_config = config;
        self
    }

    pub fn with_firewall_config(mut self, config: Option<RemoteFirewallConfig>) -> Self {
        self.firewall_config = config;
        self
    }

    pub fn with_max_submit_position(mut self, max_submit_position: usize) -> Self {
        self.max_submit_position = Some(max_submit_position);
        self
    }

    pub fn with_submit_delay_step_override_millis(
        mut self,
        submit_delay_step_override_millis: u64,
    ) -> Self {
        self.submit_delay_step_override_millis = Some(submit_delay_step_override_millis);
        self
    }

    pub fn with_state_accumulator_v2_enabled(mut self, enabled: bool) -> Self {
        self.state_accumulator_v2 = enabled;
        self
    }

    pub fn build(
        self,
        validator: ValidatorGenesisConfig,
        genesis: sui_config::genesis::Genesis,
    ) -> NodeConfig {
        let key_path = get_key_path(&validator.key_pair);
        let config_directory = self
            .config_directory
            .unwrap_or_else(|| tempfile::tempdir().unwrap().into_path());
        let db_path = config_directory
            .join(AUTHORITIES_DB_NAME)
            .join(key_path.clone());

        let network_address = validator.network_address;
        let consensus_db_path = config_directory.join(CONSENSUS_DB_NAME).join(key_path);
        let localhost = local_ip_utils::localhost_for_testing();
        let consensus_config = ConsensusConfig {
            db_path: consensus_db_path,
            db_retention_epochs: None,
            db_pruner_period_secs: None,
            max_pending_transactions: None,
            max_submit_position: self.max_submit_position,
            submit_delay_step_override_millis: self.submit_delay_step_override_millis,
            parameters: Default::default(),
        };

        let p2p_config = P2pConfig {
            listen_address: validator.p2p_listen_address.unwrap_or_else(|| {
                validator
                    .p2p_address
                    .udp_multiaddr_to_listen_address()
                    .unwrap()
            }),
            external_address: Some(validator.p2p_address),
            // Set a shorter timeout for checkpoint content download in tests, since
            // checkpoint pruning also happens much faster, and network is local.
            state_sync: Some(StateSyncConfig {
                checkpoint_content_timeout_ms: Some(10_000),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut pruning_config = AuthorityStorePruningConfig::default();
        if self.force_unpruned_checkpoints {
            pruning_config.set_num_epochs_to_retain_for_checkpoints(None);
        }
        let pruning_config = pruning_config;
        let checkpoint_executor_config = CheckpointExecutorConfig {
            data_ingestion_dir: self.data_ingestion_dir,
            ..Default::default()
        };

        NodeConfig {
            protocol_key_pair: AuthorityKeyPairWithPath::new(validator.key_pair),
            network_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(validator.network_key_pair)),
            account_key_pair: KeyPairWithPath::new(validator.account_key_pair),
            worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(validator.worker_key_pair)),
            db_path,
            network_address,
            metrics_address: validator.metrics_address,
            admin_interface_port: local_ip_utils::get_available_port(&localhost),
            json_rpc_address: local_ip_utils::new_tcp_address_for_testing(&localhost)
                .to_socket_addr()
                .unwrap(),
            consensus_config: Some(consensus_config),
            remove_deprecated_tables: false,
            enable_index_processing: default_enable_index_processing(),
            genesis: sui_config::node::Genesis::new(genesis),
            grpc_load_shed: None,
            grpc_concurrency_limit: Some(DEFAULT_GRPC_CONCURRENCY_LIMIT),
            p2p_config,
            authority_store_pruning_config: pruning_config,
            end_of_epoch_broadcast_channel_capacity:
                default_end_of_epoch_broadcast_channel_capacity(),
            checkpoint_executor_config,
            metrics: None,
            supported_protocol_versions: self.supported_protocol_versions,
            db_checkpoint_config: Default::default(),
            // By default, expensive checks will be enabled in debug build, but not in release build.
            expensive_safety_check_config: ExpensiveSafetyCheckConfig::default(),
            name_service_package_address: None,
            name_service_registry_id: None,
            name_service_reverse_registry_id: None,
            transaction_deny_config: Default::default(),
            certificate_deny_config: Default::default(),
            state_debug_dump_config: Default::default(),
            state_archive_write_config: StateArchiveConfig::default(),
            state_archive_read_config: vec![],
            state_snapshot_write_config: StateSnapshotConfig::default(),
            indexer_max_subscriptions: Default::default(),
            transaction_kv_store_read_config: Default::default(),
            transaction_kv_store_write_config: None,
            rpc: Some(sui_rpc_api::Config {
                ..Default::default()
            }),
            jwk_fetch_interval_seconds: self
                .jwk_fetch_interval
                .map(|i| i.as_secs())
                .unwrap_or(3600),
            zklogin_oauth_providers: default_zklogin_oauth_providers(),
            authority_overload_config: self.authority_overload_config.unwrap_or_default(),
            execution_cache: self.execution_cache_config.unwrap_or_default(),
            run_with_range: None,
            jsonrpc_server_type: None,
            policy_config: self.policy_config,
            firewall_config: self.firewall_config,
            state_accumulator_v2: self.state_accumulator_v2,
            enable_soft_bundle: true,
            enable_validator_tx_finalizer: true,
            verifier_signing_config: VerifierSigningConfig::default(),
            enable_db_write_stall: None,
            local_execution_time_channel_capacity: default_local_execution_time_channel_capacity(),
        }
    }

    pub fn build_new_validator<R: rand::RngCore + rand::CryptoRng>(
        self,
        rng: &mut R,
        network_config: &NetworkConfig,
    ) -> NodeConfig {
        let validator_config = ValidatorGenesisConfigBuilder::new().build(rng);
        self.build(validator_config, network_config.genesis.clone())
    }
}

#[derive(Clone, Debug, Default)]
pub struct FullnodeConfigBuilder {
    config_directory: Option<PathBuf>,
    // port for json rpc api
    rpc_port: Option<u16>,
    rpc_addr: Option<SocketAddr>,
    supported_protocol_versions: Option<SupportedProtocolVersions>,
    db_checkpoint_config: Option<DBCheckpointConfig>,
    expensive_safety_check_config: Option<ExpensiveSafetyCheckConfig>,
    db_path: Option<PathBuf>,
    network_address: Option<Multiaddr>,
    json_rpc_address: Option<SocketAddr>,
    metrics_address: Option<SocketAddr>,
    admin_interface_port: Option<u16>,
    genesis: Option<Genesis>,
    p2p_external_address: Option<Multiaddr>,
    p2p_listen_address: Option<SocketAddr>,
    network_key_pair: Option<KeyPairWithPath>,
    run_with_range: Option<RunWithRange>,
    policy_config: Option<PolicyConfig>,
    fw_config: Option<RemoteFirewallConfig>,
    data_ingestion_dir: Option<PathBuf>,
    disable_pruning: bool,
}

impl FullnodeConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config_directory(mut self, config_directory: PathBuf) -> Self {
        self.config_directory = Some(config_directory);
        self
    }

    pub fn with_rpc_port(mut self, port: u16) -> Self {
        assert!(self.rpc_addr.is_none() && self.rpc_port.is_none());
        self.rpc_port = Some(port);
        self
    }

    pub fn with_rpc_addr(mut self, addr: SocketAddr) -> Self {
        assert!(self.rpc_addr.is_none() && self.rpc_port.is_none());
        self.rpc_addr = Some(addr);
        self
    }

    pub fn with_supported_protocol_versions(mut self, versions: SupportedProtocolVersions) -> Self {
        self.supported_protocol_versions = Some(versions);
        self
    }

    pub fn with_db_checkpoint_config(mut self, db_checkpoint_config: DBCheckpointConfig) -> Self {
        self.db_checkpoint_config = Some(db_checkpoint_config);
        self
    }

    pub fn with_disable_pruning(mut self, disable_pruning: bool) -> Self {
        self.disable_pruning = disable_pruning;
        self
    }

    pub fn with_expensive_safety_check_config(
        mut self,
        expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    ) -> Self {
        self.expensive_safety_check_config = Some(expensive_safety_check_config);
        self
    }

    pub fn with_db_path(mut self, db_path: PathBuf) -> Self {
        self.db_path = Some(db_path);
        self
    }

    pub fn with_network_address(mut self, network_address: Multiaddr) -> Self {
        self.network_address = Some(network_address);
        self
    }

    pub fn with_json_rpc_address(mut self, json_rpc_address: SocketAddr) -> Self {
        self.json_rpc_address = Some(json_rpc_address);
        self
    }

    pub fn with_metrics_address(mut self, metrics_address: SocketAddr) -> Self {
        self.metrics_address = Some(metrics_address);
        self
    }

    pub fn with_admin_interface_port(mut self, admin_interface_port: u16) -> Self {
        self.admin_interface_port = Some(admin_interface_port);
        self
    }

    pub fn with_genesis(mut self, genesis: Genesis) -> Self {
        self.genesis = Some(genesis);
        self
    }

    pub fn with_p2p_external_address(mut self, p2p_external_address: Multiaddr) -> Self {
        self.p2p_external_address = Some(p2p_external_address);
        self
    }

    pub fn with_p2p_listen_address(mut self, p2p_listen_address: SocketAddr) -> Self {
        self.p2p_listen_address = Some(p2p_listen_address);
        self
    }

    pub fn with_network_key_pair(mut self, network_key_pair: Option<NetworkKeyPair>) -> Self {
        if let Some(network_key_pair) = network_key_pair {
            self.network_key_pair =
                Some(KeyPairWithPath::new(SuiKeyPair::Ed25519(network_key_pair)));
        }
        self
    }

    pub fn with_run_with_range(mut self, run_with_range: Option<RunWithRange>) -> Self {
        if let Some(run_with_range) = run_with_range {
            self.run_with_range = Some(run_with_range);
        }
        self
    }

    pub fn with_policy_config(mut self, config: Option<PolicyConfig>) -> Self {
        self.policy_config = config;
        self
    }

    pub fn with_fw_config(mut self, config: Option<RemoteFirewallConfig>) -> Self {
        self.fw_config = config;
        self
    }

    pub fn with_data_ingestion_dir(mut self, path: Option<PathBuf>) -> Self {
        self.data_ingestion_dir = path;
        self
    }

    pub fn build<R: rand::RngCore + rand::CryptoRng>(
        self,
        rng: &mut R,
        network_config: &NetworkConfig,
    ) -> NodeConfig {
        // Take advantage of ValidatorGenesisConfigBuilder to build the keypairs and addresses,
        // even though this is a fullnode.
        let validator_config = ValidatorGenesisConfigBuilder::new().build(rng);
        let ip = validator_config
            .network_address
            .to_socket_addr()
            .unwrap()
            .ip()
            .to_string();

        let key_path = get_key_path(&validator_config.key_pair);
        let config_directory = self
            .config_directory
            .unwrap_or_else(|| tempfile::tempdir().unwrap().into_path());

        let p2p_config = {
            let seed_peers = network_config
                .validator_configs
                .iter()
                .map(|config| SeedPeer {
                    peer_id: Some(anemo::PeerId(
                        config.network_key_pair().public().0.to_bytes(),
                    )),
                    address: config.p2p_config.external_address.clone().unwrap(),
                })
                .collect();

            P2pConfig {
                listen_address: self.p2p_listen_address.unwrap_or_else(|| {
                    validator_config.p2p_listen_address.unwrap_or_else(|| {
                        validator_config
                            .p2p_address
                            .udp_multiaddr_to_listen_address()
                            .unwrap()
                    })
                }),
                external_address: self
                    .p2p_external_address
                    .or(Some(validator_config.p2p_address.clone())),
                seed_peers,
                // Set a shorter timeout for checkpoint content download in tests, since
                // checkpoint pruning also happens much faster, and network is local.
                state_sync: Some(StateSyncConfig {
                    checkpoint_content_timeout_ms: Some(10_000),
                    ..Default::default()
                }),
                ..Default::default()
            }
        };

        let localhost = local_ip_utils::localhost_for_testing();
        let json_rpc_address = self.rpc_addr.unwrap_or_else(|| {
            let rpc_port = self
                .rpc_port
                .unwrap_or_else(|| local_ip_utils::get_available_port(&ip));
            format!("{}:{}", ip, rpc_port).parse().unwrap()
        });

        let checkpoint_executor_config = CheckpointExecutorConfig {
            data_ingestion_dir: self.data_ingestion_dir,
            ..Default::default()
        };

        let mut pruning_config = AuthorityStorePruningConfig::default();
        if self.disable_pruning {
            pruning_config.set_num_epochs_to_retain_for_checkpoints(None);
            pruning_config.set_num_epochs_to_retain(u64::MAX);
        };

        NodeConfig {
            protocol_key_pair: AuthorityKeyPairWithPath::new(validator_config.key_pair),
            account_key_pair: KeyPairWithPath::new(validator_config.account_key_pair),
            worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                validator_config.worker_key_pair,
            )),
            network_key_pair: self.network_key_pair.unwrap_or(KeyPairWithPath::new(
                SuiKeyPair::Ed25519(validator_config.network_key_pair),
            )),
            db_path: self
                .db_path
                .unwrap_or(config_directory.join(FULL_NODE_DB_PATH).join(key_path)),
            network_address: self
                .network_address
                .unwrap_or(validator_config.network_address),
            metrics_address: self
                .metrics_address
                .unwrap_or(local_ip_utils::new_local_tcp_socket_for_testing()),
            admin_interface_port: self
                .admin_interface_port
                .unwrap_or(local_ip_utils::get_available_port(&localhost)),
            json_rpc_address: self.json_rpc_address.unwrap_or(json_rpc_address),
            consensus_config: None,
            remove_deprecated_tables: false,
            enable_index_processing: default_enable_index_processing(),
            genesis: self.genesis.unwrap_or(sui_config::node::Genesis::new(
                network_config.genesis.clone(),
            )),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            p2p_config,
            authority_store_pruning_config: pruning_config,
            end_of_epoch_broadcast_channel_capacity:
                default_end_of_epoch_broadcast_channel_capacity(),
            checkpoint_executor_config,
            metrics: None,
            supported_protocol_versions: self.supported_protocol_versions,
            db_checkpoint_config: self.db_checkpoint_config.unwrap_or_default(),
            expensive_safety_check_config: self
                .expensive_safety_check_config
                .unwrap_or_else(ExpensiveSafetyCheckConfig::new_enable_all),
            name_service_package_address: None,
            name_service_registry_id: None,
            name_service_reverse_registry_id: None,
            transaction_deny_config: Default::default(),
            certificate_deny_config: Default::default(),
            state_debug_dump_config: Default::default(),
            state_archive_write_config: StateArchiveConfig::default(),
            state_archive_read_config: vec![],
            state_snapshot_write_config: StateSnapshotConfig::default(),
            indexer_max_subscriptions: Default::default(),
            transaction_kv_store_read_config: Default::default(),
            transaction_kv_store_write_config: Default::default(),
            rpc: Some(sui_rpc_api::Config {
                enable_indexing: Some(true),
                ..Default::default()
            }),
            // note: not used by fullnodes.
            jwk_fetch_interval_seconds: 3600,
            zklogin_oauth_providers: default_zklogin_oauth_providers(),
            authority_overload_config: Default::default(),
            run_with_range: self.run_with_range,
            jsonrpc_server_type: None,
            policy_config: self.policy_config,
            firewall_config: self.fw_config,
            execution_cache: ExecutionCacheConfig::default(),
            state_accumulator_v2: true,
            enable_soft_bundle: true,
            // This is a validator specific feature.
            enable_validator_tx_finalizer: false,
            verifier_signing_config: VerifierSigningConfig::default(),
            enable_db_write_stall: None,
            local_execution_time_channel_capacity: default_local_execution_time_channel_capacity(),
        }
    }
}

/// Given a validator keypair, return a path that can be used to identify the validator.
fn get_key_path(key_pair: &AuthorityKeyPair) -> String {
    let public_key: AuthorityPublicKeyBytes = key_pair.public().into();
    let mut key_path = Hex::encode(public_key);
    // 12 is rather arbitrary here but it's a nice balance between being short and being unique.
    key_path.truncate(12);
    key_path
}

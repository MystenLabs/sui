// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis_config::AccountConfig;
use crate::genesis_config::{GenesisConfig, ValidatorGenesisConfig};
use crate::network_config::NetworkConfig;
use fastcrypto::encoding::{Encoding, Hex};
use narwhal_config::{
    NetworkAdminServerParameters, Parameters as ConsensusParameters, PrometheusMetricsParameters,
};
use rand::rngs::OsRng;
use rand::RngCore;
use std::net::{IpAddr, SocketAddr};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};
use sui_config::genesis::{TokenAllocation, TokenDistributionScheduleBuilder};
use sui_config::node::{
    default_enable_index_processing, default_end_of_epoch_broadcast_channel_capacity,
    AuthorityKeyPairWithPath, AuthorityStorePruningConfig, DBCheckpointConfig,
    ExpensiveSafetyCheckConfig, KeyPairWithPath, StateDebugDumpConfig,
    DEFAULT_GRPC_CONCURRENCY_LIMIT, DEFAULT_VALIDATOR_GAS_PRICE,
};
use sui_config::utils;
use sui_config::{
    p2p::{P2pConfig, SeedPeer},
    ConsensusConfig, NodeConfig, AUTHORITIES_DB_NAME, CONSENSUS_DB_NAME,
};
use sui_protocol_config::SupportedProtocolVersions;
use sui_types::base_types::{AuthorityName, SuiAddress};
use sui_types::committee::{Committee, ProtocolVersion};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, PublicKey, SuiKeyPair,
};
use sui_types::object::Object;

pub enum CommitteeConfig {
    Size(NonZeroUsize),
    Validators(Vec<ValidatorGenesisConfig>),
    AccountKeys(Vec<AccountKeyPair>),
}

enum ValidatorIpSelection {
    Localhost,
    Simulator,
}

pub type SupportedProtocolVersionsCallback = Arc<
    dyn Fn(
            usize,                 /* validator idx */
            Option<AuthorityName>, /* None for fullnode */
        ) -> SupportedProtocolVersions
        + Send
        + Sync
        + 'static,
>;

#[derive(Clone)]
pub enum ProtocolVersionsConfig {
    // use SYSTEM_DEFAULT
    Default,
    // Use one range for all validators.
    Global(SupportedProtocolVersions),
    // A closure that returns the versions for each validator.
    PerValidator(SupportedProtocolVersionsCallback),
}

pub struct ConfigBuilder<R = OsRng> {
    rng: Option<R>,
    config_directory: PathBuf,
    randomize_ports: bool,
    committee: Option<CommitteeConfig>,
    genesis_config: Option<GenesisConfig>,
    reference_gas_price: Option<u64>,
    additional_objects: Vec<Object>,
    with_swarm: bool,
    validator_ip_sel: ValidatorIpSelection,
    // the versions that are supported by each validator
    supported_protocol_versions_config: ProtocolVersionsConfig,

    db_checkpoint_config: DBCheckpointConfig,
    state_debug_dump_config: StateDebugDumpConfig,
}

impl ConfigBuilder {
    pub fn new<P: AsRef<Path>>(config_directory: P) -> Self {
        Self {
            rng: Some(OsRng),
            config_directory: config_directory.as_ref().into(),
            randomize_ports: true,
            committee: Some(CommitteeConfig::Size(NonZeroUsize::new(1).unwrap())),
            genesis_config: None,
            reference_gas_price: None,
            additional_objects: vec![],
            with_swarm: false,
            // Set a sensible default here so that most tests can run with or without the
            // simulator.
            validator_ip_sel: if cfg!(msim) {
                ValidatorIpSelection::Simulator
            } else {
                ValidatorIpSelection::Localhost
            },
            supported_protocol_versions_config: ProtocolVersionsConfig::Default,
            db_checkpoint_config: DBCheckpointConfig::default(),
            state_debug_dump_config: StateDebugDumpConfig::default(),
        }
    }

    pub fn new_with_temp_dir() -> Self {
        Self::new(tempfile::tempdir().unwrap().into_path())
    }
}

impl<R> ConfigBuilder<R> {
    pub fn randomize_ports(mut self, randomize_ports: bool) -> Self {
        self.randomize_ports = randomize_ports;
        self
    }

    pub fn with_swarm(mut self) -> Self {
        self.with_swarm = true;
        self
    }

    pub fn committee(mut self, committee: CommitteeConfig) -> Self {
        self.committee = Some(committee);
        self
    }

    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee = Some(CommitteeConfig::Size(committee_size));
        self
    }

    pub fn with_validator_account_keys(mut self, keys: Vec<AccountKeyPair>) -> Self {
        self.committee = Some(CommitteeConfig::AccountKeys(keys));
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorGenesisConfig>) -> Self {
        self.committee = Some(CommitteeConfig::Validators(validators));
        self
    }

    pub fn with_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        assert!(self.genesis_config.is_none(), "Genesis config already set");
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn with_reference_gas_price(mut self, reference_gas_price: u64) -> Self {
        self.reference_gas_price = Some(reference_gas_price);
        self
    }

    pub fn with_accounts(mut self, accounts: Vec<AccountConfig>) -> Self {
        self.get_or_init_genesis_config().accounts = accounts;
        self
    }

    pub fn with_objects<I: IntoIterator<Item = Object>>(mut self, objects: I) -> Self {
        self.additional_objects.extend(objects);
        self
    }

    pub fn with_epoch_duration(mut self, epoch_duration_ms: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .epoch_duration_ms = epoch_duration_ms;
        self
    }

    pub fn with_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .protocol_version = protocol_version;
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

    pub fn with_debug_dump_config(mut self, state_debug_dump_config: StateDebugDumpConfig) -> Self {
        self.state_debug_dump_config = state_debug_dump_config;
        self
    }

    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> ConfigBuilder<N> {
        ConfigBuilder {
            rng: Some(rng),
            config_directory: self.config_directory,
            randomize_ports: self.randomize_ports,
            committee: self.committee,
            genesis_config: self.genesis_config,
            reference_gas_price: self.reference_gas_price,
            additional_objects: self.additional_objects,
            with_swarm: self.with_swarm,
            validator_ip_sel: self.validator_ip_sel,
            supported_protocol_versions_config: self.supported_protocol_versions_config,
            db_checkpoint_config: self.db_checkpoint_config,
            state_debug_dump_config: self.state_debug_dump_config,
        }
    }

    fn get_or_init_genesis_config(&mut self) -> &mut GenesisConfig {
        if self.genesis_config.is_none() {
            self.genesis_config = Some(GenesisConfig::for_local_testing());
        }
        self.genesis_config.as_mut().unwrap()
    }
}

impl<R: rand::RngCore + rand::CryptoRng> ConfigBuilder<R> {
    //TODO right now we always randomize ports, we may want to have a default port configuration
    pub fn build(mut self) -> NetworkConfig {
        let committee = self.committee.take().unwrap();

        let mut rng = self.rng.take().unwrap();

        let validator_with_account_key = |idx: usize,
                                          protocol_key_pair: AuthorityKeyPair,
                                          account_key_pair: AccountKeyPair,
                                          rng: &mut R|
         -> ValidatorGenesisConfig {
            let (worker_key_pair, network_key_pair): (NetworkKeyPair, NetworkKeyPair) =
                (get_key_pair_from_rng(rng).1, get_key_pair_from_rng(rng).1);

            ValidatorGenesisConfig::new(
                idx,
                protocol_key_pair,
                worker_key_pair,
                account_key_pair.into(),
                network_key_pair,
                self.reference_gas_price
                    .unwrap_or(DEFAULT_VALIDATOR_GAS_PRICE),
            )
        };

        let validators = match committee {
            CommitteeConfig::Size(size) => {
                // We always get fixed protocol keys from this function (which is isolated from
                // external test randomness because it uses a fixed seed). Necessary because some
                // tests call `make_tx_certs_and_signed_effects`, which locally forges a cert using
                // this same committee.
                let (_, keys) = Committee::new_simple_test_committee_of_size(size.into());

                keys.into_iter()
                    .enumerate()
                    .map(|(i, authority_key)| {
                        let account_key_pair =
                            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng).1;
                        validator_with_account_key(i, authority_key, account_key_pair, &mut rng)
                    })
                    .collect::<Vec<_>>()
            }

            CommitteeConfig::Validators(v) => v,

            CommitteeConfig::AccountKeys(keys) => {
                // See above re fixed protocol keys
                let (_, protocol_keys) = Committee::new_simple_test_committee_of_size(keys.len());
                keys.into_iter()
                    .zip(protocol_keys.into_iter())
                    .enumerate()
                    .map(|(i, (account_key, protocol_key))| {
                        validator_with_account_key(i, protocol_key, account_key, &mut rng)
                    })
                    .collect::<Vec<_>>()
            }
        };

        self.build_with_validators(rng, validators)
    }

    fn build_with_validators(
        mut self,
        mut rng: R,
        validators: Vec<ValidatorGenesisConfig>,
    ) -> NetworkConfig {
        self.get_or_init_genesis_config();
        let genesis_config = self.genesis_config.unwrap();

        let (account_keys, allocations) = genesis_config.generate_accounts(&mut rng).unwrap();

        let token_distribution_schedule = {
            let mut builder = TokenDistributionScheduleBuilder::new();
            for allocation in allocations {
                builder.add_allocation(allocation);
            }
            // Add allocations for each validator
            for validator in &validators {
                let account_key: PublicKey = validator.account_key_pair.public();
                let address = SuiAddress::from(&account_key);
                let stake = TokenAllocation {
                    recipient_address: address,
                    amount_mist: validator.stake,
                    staked_with_validator: Some(address),
                };
                builder.add_allocation(stake);
            }
            builder.build()
        };

        let genesis = {
            let mut builder = sui_genesis_builder::Builder::new()
                .with_parameters(genesis_config.parameters)
                .add_objects(self.additional_objects);

            for (i, validator) in validators.iter().enumerate() {
                let name = format!("validator-{i}");
                let validator_info = validator.to_validator_info(name);
                let pop = generate_proof_of_possession(
                    &validator.key_pair,
                    (&validator.account_key_pair.public()).into(),
                );
                builder = builder.add_validator(validator_info, pop);
            }

            builder = builder.with_token_distribution_schedule(token_distribution_schedule);

            for validator in &validators {
                builder = builder.add_validator_signature(&validator.key_pair);
            }

            builder.build()
        };

        let validator_configs = validators
            .into_iter()
            .enumerate()
            .map(|(idx, validator)| {
                let public_key: AuthorityPublicKeyBytes = validator.key_pair.public().into();
                let mut key_path = Hex::encode(public_key);
                key_path.truncate(12);
                let db_path = self
                    .config_directory
                    .join(AUTHORITIES_DB_NAME)
                    .join(key_path.clone());
                let network_address = validator.network_address;
                let consensus_address = validator.consensus_address;
                let consensus_db_path =
                    self.config_directory.join(CONSENSUS_DB_NAME).join(key_path);
                let internal_worker_address = validator.consensus_internal_worker_address;
                let consensus_config = ConsensusConfig {
                    address: consensus_address,
                    db_path: consensus_db_path,
                    internal_worker_address,
                    max_pending_transactions: None,
                    max_submit_position: None,
                    submit_delay_step_override_millis: None,
                    narwhal_config: ConsensusParameters {
                        network_admin_server: match self.validator_ip_sel {
                            ValidatorIpSelection::Simulator => NetworkAdminServerParameters {
                                primary_network_admin_server_port: 8889,
                                worker_network_admin_server_base_port: 8890,
                            },
                            _ => NetworkAdminServerParameters {
                                primary_network_admin_server_port: utils::get_available_port(
                                    "127.0.0.1",
                                ),
                                worker_network_admin_server_base_port: utils::get_available_port(
                                    "127.0.0.1",
                                ),
                            },
                        },
                        prometheus_metrics: PrometheusMetricsParameters {
                            socket_addr: validator.narwhal_metrics_address,
                        },
                        ..Default::default()
                    },
                };

                let p2p_config = P2pConfig {
                    listen_address: validator.p2p_listen_address.unwrap_or_else(|| {
                        validator
                            .p2p_address
                            .udp_multiaddr_to_listen_address()
                            .unwrap()
                    }),
                    external_address: Some(validator.p2p_address),
                    ..Default::default()
                };

                let supported_protocol_versions = match &self.supported_protocol_versions_config {
                    ProtocolVersionsConfig::Default => SupportedProtocolVersions::SYSTEM_DEFAULT,
                    ProtocolVersionsConfig::Global(v) => *v,
                    ProtocolVersionsConfig::PerValidator(func) => func(idx, Some(public_key)),
                };

                NodeConfig {
                    protocol_key_pair: AuthorityKeyPairWithPath::new(validator.key_pair),
                    network_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                        validator.network_key_pair,
                    )),
                    account_key_pair: KeyPairWithPath::new(validator.account_key_pair),
                    worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                        validator.worker_key_pair,
                    )),
                    db_path,
                    network_address,
                    metrics_address: validator.metrics_address,
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
            })
            .collect();
        NetworkConfig {
            validator_configs,
            genesis,
            account_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_config::node::Genesis;

    #[test]
    fn serialize_genesis_config_in_place() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let g = Genesis::new(genesis);

        let mut s = serde_yaml::to_string(&g).unwrap();
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);

        // If both in-place and file location are provided, prefer the in-place variant
        s.push_str("\ngenesis-file-location: path/to/file");
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn load_genesis_config_from_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let genesis_config = Genesis::new_from_file(file.path());

        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;
        genesis.save(file.path()).unwrap();

        let loaded_genesis = genesis_config.genesis().unwrap();
        loaded_genesis.checkpoint_contents().digest(); // cache digest before comparing.
        assert_eq!(&genesis, loaded_genesis);
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
    pub fn new(network_config: &'a NetworkConfig) -> Self {
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
            transaction_deny_config: Default::default(),
            certificate_deny_config: Default::default(),
            state_debug_dump_config: Default::default(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::sync::Arc;
    use sui_adapter::execution_mode;
    use sui_config::genesis::Genesis;
    use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
    use sui_types::epoch_data::EpochData;
    use sui_types::gas::SuiGasStatus;
    use sui_types::metrics::LimitsMetrics;
    use sui_types::sui_system_state::SuiSystemStateTrait;
    use sui_types::temporary_store::TemporaryStore;
    use sui_types::transaction::InputObjects;

    #[test]
    fn roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s: Genesis = serde_yaml::from_str(&s).unwrap();
        // cache the digest so that the comparison succeeds.
        from_s.checkpoint_contents().digest();
        assert_eq!(genesis, from_s);
    }

    #[test]
    fn genesis_transaction() {
        let builder = crate::network_config_builder::ConfigBuilder::new_with_temp_dir();
        let network_config = builder.build();
        let genesis = network_config.genesis;
        let protocol_version = ProtocolVersion::new(genesis.sui_system_object().protocol_version());
        let protocol_config = ProtocolConfig::get_for_version(protocol_version);

        let genesis_transaction = genesis.transaction().clone();

        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
            &protocol_config,
        );

        let enable_move_vm_paranoid_checks = false;
        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let move_vm = std::sync::Arc::new(
            sui_adapter::adapter::new_move_vm(
                native_functions,
                &protocol_config,
                enable_move_vm_paranoid_checks,
            )
            .expect("We defined natives to not fail here"),
        );

        // Use a throwaway metrics registry for genesis transaction execution.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        let transaction_data = &genesis_transaction.data().intent_message().value;
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (_inner_temp_store, effects, _execution_error) =
            sui_adapter::execution_engine::execute_transaction_to_effects::<
                execution_mode::Normal,
                _,
            >(
                vec![],
                temporary_store,
                kind,
                signer,
                &gas,
                *genesis_transaction.digest(),
                Default::default(),
                &move_vm,
                SuiGasStatus::new_unmetered(&protocol_config),
                &EpochData::new_test(),
                &protocol_config,
                metrics,
                false, // enable_expensive_checks
                &HashSet::new(),
            );

        assert_eq!(&effects, genesis.effects());
    }
}

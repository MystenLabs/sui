// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::genesis::{TokenAllocation, TokenDistributionScheduleBuilder};
use crate::genesis_config::AccountConfig;
use crate::node::{
    default_enable_index_processing, default_end_of_epoch_broadcast_channel_capacity,
    AuthorityKeyPairWithPath, DBCheckpointConfig, KeyPairWithPath, DEFAULT_VALIDATOR_GAS_PRICE,
};
use crate::{
    genesis,
    genesis_config::{GenesisConfig, ValidatorConfigInfo, ValidatorGenesisInfo},
    node::AuthorityStorePruningConfig,
    p2p::P2pConfig,
    utils, ConsensusConfig, NetworkConfig, NodeConfig, ValidatorInfo, AUTHORITIES_DB_NAME,
    CONSENSUS_DB_NAME,
};
use fastcrypto::encoding::{Encoding, Hex};
use narwhal_config::{
    NetworkAdminServerParameters, Parameters as ConsensusParameters, PrometheusMetricsParameters,
};
use rand::rngs::OsRng;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};
use sui_protocol_config::SupportedProtocolVersions;
use sui_types::base_types::{AuthorityName, SuiAddress};
use sui_types::committee::{Committee, ProtocolVersion};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, NetworkPublicKey, PublicKey,
    SuiKeyPair,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Object;

pub enum CommitteeConfig {
    Size(NonZeroUsize),
    Validators(Vec<ValidatorConfigInfo>),
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

    pub fn with_validators(mut self, validators: Vec<ValidatorConfigInfo>) -> Self {
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
         -> ValidatorConfigInfo {
            let (worker_key_pair, network_key_pair): (NetworkKeyPair, NetworkKeyPair) =
                (get_key_pair_from_rng(rng).1, get_key_pair_from_rng(rng).1);

            self.build_validator(
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

    fn build_validator(
        &self,
        index: usize,
        key_pair: AuthorityKeyPair,
        worker_key_pair: NetworkKeyPair,
        account_key_pair: SuiKeyPair,
        network_key_pair: NetworkKeyPair,
        gas_price: u64,
    ) -> ValidatorConfigInfo {
        match self.validator_ip_sel {
            ValidatorIpSelection::Localhost => ValidatorConfigInfo {
                genesis_info: ValidatorGenesisInfo::from_localhost_for_testing(
                    key_pair,
                    worker_key_pair,
                    account_key_pair,
                    network_key_pair,
                    gas_price,
                ),
                consensus_address: utils::new_tcp_network_address(),
                consensus_internal_worker_address: None,
            },

            ValidatorIpSelection::Simulator => {
                // we will probably never run this many validators in a sim
                let low_octet = index + 1;
                if low_octet > 255 {
                    todo!("smarter IP formatting required");
                }

                let ip = format!("10.10.0.{}", low_octet);
                let make_tcp_addr = |port: u16| -> Multiaddr {
                    format!("/ip4/{}/tcp/{}/http", ip, port).parse().unwrap()
                };

                ValidatorConfigInfo {
                    genesis_info: ValidatorGenesisInfo::from_base_ip(
                        key_pair,
                        worker_key_pair,
                        account_key_pair,
                        network_key_pair,
                        None,
                        ip.clone(),
                        index,
                        gas_price,
                    ),
                    consensus_address: make_tcp_addr(4000 + index as u16),
                    consensus_internal_worker_address: None,
                }
            }
        }
    }

    fn build_with_validators(
        mut self,
        mut rng: R,
        validators: Vec<ValidatorConfigInfo>,
    ) -> NetworkConfig {
        let validator_set = validators
            .iter()
            .enumerate()
            .map(|(i, validator)| {
                let name = format!("validator-{i}");
                let protocol_key: AuthorityPublicKeyBytes =
                    validator.genesis_info.key_pair.public().into();
                let account_key: PublicKey = validator.genesis_info.account_key_pair.public();
                let network_key: NetworkPublicKey =
                    validator.genesis_info.network_key_pair.public().clone();
                let worker_key: NetworkPublicKey =
                    validator.genesis_info.worker_key_pair.public().clone();
                let network_address = validator.genesis_info.network_address.clone();
                let pop = generate_proof_of_possession(
                    &validator.genesis_info.key_pair,
                    (&validator.genesis_info.account_key_pair.public()).into(),
                );

                (
                    ValidatorInfo {
                        name,
                        protocol_key,
                        worker_key,
                        network_key,
                        account_address: SuiAddress::from(&account_key),
                        gas_price: validator.genesis_info.gas_price,
                        commission_rate: validator.genesis_info.commission_rate,
                        network_address,
                        p2p_address: validator.genesis_info.p2p_address.clone(),
                        narwhal_primary_address: validator
                            .genesis_info
                            .narwhal_primary_address
                            .clone(),
                        narwhal_worker_address: validator
                            .genesis_info
                            .narwhal_worker_address
                            .clone(),
                        description: String::new(),
                        image_url: String::new(),
                        project_url: String::new(),
                    },
                    pop,
                )
            })
            .collect::<Vec<_>>();

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
                let account_key: PublicKey = validator.genesis_info.account_key_pair.public();
                let address = SuiAddress::from(&account_key);
                let stake = TokenAllocation {
                    recipient_address: address,
                    amount_mist: validator.genesis_info.stake,
                    staked_with_validator: Some(address),
                };
                builder.add_allocation(stake);
            }
            builder.build()
        };

        let genesis = {
            let mut builder = genesis::Builder::new()
                .with_parameters(genesis_config.parameters)
                .add_objects(self.additional_objects);

            for (validator, proof_of_possession) in validator_set {
                builder = builder.add_validator(validator, proof_of_possession);
            }

            builder = builder.with_token_distribution_schedule(token_distribution_schedule);

            for validator in &validators {
                builder = builder.add_validator_signature(&validator.genesis_info.key_pair);
            }

            builder.build()
        };

        let validator_configs = validators
            .into_iter()
            .enumerate()
            .map(|(idx, validator)| {
                let public_key: AuthorityPublicKeyBytes =
                    validator.genesis_info.key_pair.public().into();
                let mut key_path = Hex::encode(public_key);
                key_path.truncate(12);
                let db_path = self
                    .config_directory
                    .join(AUTHORITIES_DB_NAME)
                    .join(key_path.clone());
                let network_address = validator.genesis_info.network_address;
                let consensus_address = validator.consensus_address;
                let consensus_db_path =
                    self.config_directory.join(CONSENSUS_DB_NAME).join(key_path);
                let internal_worker_address = validator.consensus_internal_worker_address;
                let consensus_config = ConsensusConfig {
                    address: consensus_address,
                    db_path: consensus_db_path,
                    internal_worker_address,
                    max_pending_transactions: None,
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
                            socket_addr: validator.genesis_info.narwhal_metrics_address,
                        },
                        ..Default::default()
                    },
                };

                let p2p_config = P2pConfig {
                    listen_address: validator.genesis_info.p2p_listen_address.unwrap_or_else(
                        || {
                            validator
                                .genesis_info
                                .p2p_address
                                .udp_multiaddr_to_listen_address()
                                .unwrap()
                        },
                    ),
                    external_address: Some(validator.genesis_info.p2p_address),
                    ..Default::default()
                };

                let supported_protocol_versions = match &self.supported_protocol_versions_config {
                    ProtocolVersionsConfig::Default => SupportedProtocolVersions::SYSTEM_DEFAULT,
                    ProtocolVersionsConfig::Global(v) => *v,
                    ProtocolVersionsConfig::PerValidator(func) => func(idx, Some(public_key)),
                };

                NodeConfig {
                    protocol_key_pair: AuthorityKeyPairWithPath::new(
                        validator.genesis_info.key_pair,
                    ),
                    network_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                        validator.genesis_info.network_key_pair,
                    )),
                    account_key_pair: KeyPairWithPath::new(validator.genesis_info.account_key_pair),
                    worker_key_pair: KeyPairWithPath::new(SuiKeyPair::Ed25519(
                        validator.genesis_info.worker_key_pair,
                    )),
                    db_path,
                    network_address,
                    metrics_address: validator.genesis_info.metrics_address,
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
                    genesis: crate::node::Genesis::new(genesis.clone()),
                    grpc_load_shed: genesis_config.grpc_load_shed,
                    grpc_concurrency_limit: genesis_config.grpc_concurrency_limit,
                    p2p_config,
                    authority_store_pruning_config: AuthorityStorePruningConfig::validator_config(),
                    end_of_epoch_broadcast_channel_capacity:
                        default_end_of_epoch_broadcast_channel_capacity(),
                    checkpoint_executor_config: Default::default(),
                    metrics: None,
                    supported_protocol_versions: Some(supported_protocol_versions),
                    db_checkpoint_config: self.db_checkpoint_config.clone(),
                    indirect_objects_threshold: usize::MAX,
                    expensive_safety_check_config: Default::default(),
                    name_service_resolver_object_id: None,
                    transaction_deny_config: Default::default(),
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

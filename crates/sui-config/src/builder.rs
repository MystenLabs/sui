// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    genesis,
    genesis_config::{GenesisConfig, ValidatorGenesisInfo},
    p2p::P2pConfig,
    utils, ConsensusConfig, NetworkConfig, NodeConfig, ValidatorInfo, AUTHORITIES_DB_NAME,
    CONSENSUS_DB_NAME,
};
use rand::rngs::OsRng;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};
use sui_types::{
    base_types::encode_bytes_hex,
    crypto::{
        generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
        AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, NetworkPublicKey, PublicKey,
        SuiKeyPair,
    },
};

pub enum CommitteeConfig {
    Size(NonZeroUsize),
    Validators(Vec<ValidatorGenesisInfo>),
}

enum ValidatorIpSelection {
    Localhost,
    Simulator,
}

pub struct ConfigBuilder<R = OsRng> {
    rng: Option<R>,
    config_directory: PathBuf,
    randomize_ports: bool,
    committee: Option<CommitteeConfig>,
    initial_accounts_config: Option<GenesisConfig>,
    with_swarm: bool,
    validator_ip_sel: ValidatorIpSelection,
}

impl ConfigBuilder {
    pub fn new<P: AsRef<Path>>(config_directory: P) -> Self {
        Self {
            rng: Some(OsRng),
            config_directory: config_directory.as_ref().into(),
            randomize_ports: true,
            committee: Some(CommitteeConfig::Size(NonZeroUsize::new(1).unwrap())),
            initial_accounts_config: None,
            with_swarm: false,
            // Set a sensible default here so that most tests can run with or without the
            // simulator.
            validator_ip_sel: if cfg!(msim) {
                ValidatorIpSelection::Simulator
            } else {
                ValidatorIpSelection::Localhost
            },
        }
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

    pub fn with_validators(mut self, validators: Vec<ValidatorGenesisInfo>) -> Self {
        self.committee = Some(CommitteeConfig::Validators(validators));
        self
    }

    pub fn initial_accounts_config(mut self, initial_accounts_config: GenesisConfig) -> Self {
        self.initial_accounts_config = Some(initial_accounts_config);
        self
    }

    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> ConfigBuilder<N> {
        ConfigBuilder {
            rng: Some(rng),
            config_directory: self.config_directory,
            randomize_ports: self.randomize_ports,
            committee: self.committee,
            initial_accounts_config: self.initial_accounts_config,
            with_swarm: self.with_swarm,
            validator_ip_sel: self.validator_ip_sel,
        }
    }
}

impl<R: rand::RngCore + rand::CryptoRng> ConfigBuilder<R> {
    //TODO right now we always randomize ports, we may want to have a default port configuration
    pub fn build(mut self) -> NetworkConfig {
        let committee = self.committee.take().unwrap();

        let mut rng = self.rng.take().unwrap();

        let validators = match committee {
            CommitteeConfig::Size(size) => (0..size.get())
                .map(|i| {
                    (
                        i,
                        (
                            get_key_pair_from_rng(&mut rng).1,
                            get_key_pair_from_rng(&mut rng).1,
                            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng)
                                .1
                                .into(),
                            get_key_pair_from_rng(&mut rng).1,
                        ),
                    )
                })
                .map(
                    |(i, (key_pair, worker_key_pair, account_key_pair, network_key_pair)): (
                        _,
                        (AuthorityKeyPair, NetworkKeyPair, SuiKeyPair, NetworkKeyPair),
                    )| {
                        self.build_validator(
                            i,
                            key_pair,
                            worker_key_pair,
                            account_key_pair,
                            network_key_pair,
                        )
                    },
                )
                .collect::<Vec<_>>(),
            CommitteeConfig::Validators(v) => v,
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
    ) -> ValidatorGenesisInfo {
        match self.validator_ip_sel {
            ValidatorIpSelection::Localhost => ValidatorGenesisInfo::from_localhost_for_testing(
                key_pair,
                worker_key_pair,
                account_key_pair,
                network_key_pair,
            ),
            ValidatorIpSelection::Simulator => {
                // we will probably never run this many validators in a sim
                let low_octet = index + 1;
                if low_octet > 255 {
                    todo!("smarter IP formatting required");
                }

                let ip = format!("10.10.0.{}", low_octet);

                ValidatorGenesisInfo::from_base_ip(
                    key_pair,
                    worker_key_pair,
                    account_key_pair,
                    network_key_pair,
                    ip,
                    index,
                )
            }
        }
    }

    fn build_with_validators(
        self,
        mut rng: R,
        validators: Vec<ValidatorGenesisInfo>,
    ) -> NetworkConfig {
        let validator_set = validators
            .iter()
            .enumerate()
            .map(|(i, validator)| {
                let name = format!("validator-{i}");
                let protocol_key: AuthorityPublicKeyBytes = validator.key_pair.public().into();
                let account_key: PublicKey = validator.account_key_pair.public();
                let network_key: NetworkPublicKey = validator.network_key_pair.public().clone();
                let worker_key: NetworkPublicKey = validator.worker_key_pair.public().clone();
                let stake = validator.stake;
                let network_address = validator.network_address.clone();
                let pop = generate_proof_of_possession(
                    &validator.key_pair,
                    (&validator.account_key_pair.public()).into(),
                );

                (
                    ValidatorInfo {
                        name,
                        protocol_key,
                        worker_key,
                        network_key,
                        account_key,
                        stake,
                        delegation: 0, // no delegation yet at genesis
                        gas_price: validator.gas_price,
                        network_address,
                        narwhal_primary_address: validator.narwhal_primary_address.clone(),
                        narwhal_worker_address: validator.narwhal_worker_address.clone(),
                        narwhal_consensus_address: validator.narwhal_consensus_address.clone(),
                    },
                    pop,
                )
            })
            .collect::<Vec<_>>();

        let initial_accounts_config = self
            .initial_accounts_config
            .unwrap_or_else(GenesisConfig::for_local_testing);
        let (account_keys, objects) = initial_accounts_config.generate_accounts(&mut rng).unwrap();

        let genesis = {
            let mut builder = genesis::Builder::new().add_objects(objects);

            for (validator, proof_of_possession) in validator_set {
                builder = builder.add_validator(validator, proof_of_possession);
            }

            builder.build()
        };

        let validator_configs = validators
            .into_iter()
            .map(|validator| {
                let public_key: AuthorityPublicKeyBytes = validator.key_pair.public().into();
                let db_path = self
                    .config_directory
                    .join(AUTHORITIES_DB_NAME)
                    .join(encode_bytes_hex(public_key));
                let network_address = validator.network_address;
                let consensus_address = validator.narwhal_consensus_address;
                let consensus_db_path = self
                    .config_directory
                    .join(CONSENSUS_DB_NAME)
                    .join(encode_bytes_hex(public_key));
                let consensus_config = ConsensusConfig {
                    consensus_address,
                    consensus_db_path,
                    timeout_secs: Some(60),
                    narwhal_config: Default::default(),
                };

                let p2p_config = P2pConfig {
                    listen_address: utils::available_local_socket_address(),
                    ..Default::default()
                };

                NodeConfig {
                    protocol_key_pair: Arc::new(validator.key_pair),
                    worker_key_pair: Arc::new(validator.worker_key_pair),
                    account_key_pair: Arc::new(validator.account_key_pair),
                    network_key_pair: Arc::new(validator.network_key_pair),
                    db_path,
                    network_address,
                    metrics_address: utils::available_local_socket_address(),
                    admin_interface_port: utils::get_available_port(),
                    json_rpc_address: utils::available_local_socket_address(),
                    websocket_address: None,
                    consensus_config: Some(consensus_config),
                    enable_event_processing: false,
                    enable_gossip: true,
                    enable_checkpoint: true,
                    enable_reconfig: false,
                    genesis: crate::node::Genesis::new(genesis.clone()),
                    grpc_load_shed: initial_accounts_config.grpc_load_shed,
                    grpc_concurrency_limit: initial_accounts_config.grpc_concurrency_limit,
                    p2p_config,
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

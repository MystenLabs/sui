// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use debug_ignore::DebugIgnore;
use narwhal_config::{
    Authority, Committee as ConsensusCommittee, PrimaryAddresses, Stake, WorkerAddresses,
};
use rand::rngs::OsRng;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_types::{base_types::encode_bytes_hex, crypto::get_key_pair_from_rng};

use crate::{
    genesis, new_network_address, CommitteeConfig, ConsensuseConfig, NetworkConfig,
    ValidatorConfig, ValidatorInfo, AUTHORITIES_DB_NAME, CONSENSUS_DB_NAME, DEFAULT_STAKE,
};

pub struct ConfigBuilder<R = OsRng> {
    rng: R,
    config_directory: PathBuf,
    randomize_ports: bool,
    committee_size: NonZeroUsize,
}

impl ConfigBuilder {
    pub fn new<P: AsRef<Path>>(config_directory: P) -> Self {
        Self {
            rng: OsRng,
            config_directory: config_directory.as_ref().into(),
            randomize_ports: true,
            committee_size: NonZeroUsize::new(1).unwrap(),
        }
    }
}

impl<R> ConfigBuilder<R> {
    pub fn randomize_ports(mut self, randomize_ports: bool) -> Self {
        self.randomize_ports = randomize_ports;
        self
    }

    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee_size = committee_size;
        self
    }

    pub fn rng<N: ::rand::RngCore + ::rand::CryptoRng>(self, rng: N) -> ConfigBuilder<N> {
        ConfigBuilder {
            rng,
            config_directory: self.config_directory,
            randomize_ports: self.randomize_ports,
            committee_size: self.committee_size,
        }
    }
}

impl<R: ::rand::RngCore + ::rand::CryptoRng> ConfigBuilder<R> {
    //TODO right now we always randomize ports, we may want to have a default port configuration
    pub fn build(mut self) -> NetworkConfig {
        let epoch = 0;

        let keys = (0..self.committee_size.get())
            .map(|_| get_key_pair_from_rng(&mut self.rng).1)
            .collect::<Vec<_>>();

        let validator_set = keys
            .iter()
            .map(|key| {
                let public_key = *key.public_key_bytes();
                let stake = DEFAULT_STAKE;
                let network_address = new_network_address();

                ValidatorInfo {
                    public_key,
                    stake,
                    network_address,
                }
            })
            .collect::<Vec<_>>();

        let genesis = {
            let mut builder = genesis::Builder::new()
                .sui_framework(PathBuf::from(DEFAULT_FRAMEWORK_PATH))
                .move_framework(
                    PathBuf::from(DEFAULT_FRAMEWORK_PATH)
                        .join("deps")
                        .join("move-stdlib"),
                );

            for validator in &validator_set {
                builder = builder.add_validator(validator.public_key(), validator.stake());
            }

            builder.build()
        };

        let narwhal_committee = validator_set
            .iter()
            .map(|validator| {
                let name = validator
                    .public_key
                    .make_narwhal_public_key()
                    .expect("Can't get narwhal public key");
                let primary = PrimaryAddresses {
                    primary_to_primary: new_network_address(),
                    worker_to_primary: new_network_address(),
                };
                let workers = [(
                    0, // worker_id
                    WorkerAddresses {
                        primary_to_worker: new_network_address(),
                        transactions: new_network_address(),
                        worker_to_worker: new_network_address(),
                    },
                )]
                .into_iter()
                .collect();
                let authority = Authority {
                    stake: validator.stake() as Stake, //TODO this should at least be the same size integer
                    primary,
                    workers,
                };

                (name, authority)
            })
            .collect();
        let consensus_committee = ConsensusCommittee {
            authorities: narwhal_committee,
        };

        let committe_config = CommitteeConfig {
            epoch,
            validator_set,
            consensus_committee: DebugIgnore(consensus_committee),
        };

        let validator_configs = keys
            .into_iter()
            .map(|key| {
                let db_path = self
                    .config_directory
                    .join(AUTHORITIES_DB_NAME)
                    .join(encode_bytes_hex(key.public_key_bytes()));
                let network_address = committe_config
                    .validator_set()
                    .iter()
                    .find(|validator| validator.public_key() == *key.public_key_bytes())
                    .map(|validator| validator.network_address().clone())
                    .unwrap();
                let consensus_address = committe_config
                    .narwhal_committee()
                    .authorities
                    .get(&key.public_key_bytes().make_narwhal_public_key().unwrap())
                    .unwrap()
                    .workers
                    .get(&0)
                    .unwrap()
                    .transactions
                    .clone();
                let consensus_db_path = self
                    .config_directory
                    .join(CONSENSUS_DB_NAME)
                    .join(encode_bytes_hex(key.public_key_bytes()));
                let consensus_config = ConsensuseConfig {
                    consensus_address,
                    consensus_db_path,
                    narwhal_config: Default::default(),
                };

                let metrics_address = new_network_address();

                ValidatorConfig {
                    key_pair: key,
                    db_path,
                    network_address,
                    metrics_address,
                    consensus_config,
                    committee_config: committe_config.clone(),
                    genesis: genesis.clone(),
                }
            })
            .collect();

        NetworkConfig {
            validator_configs,
            loaded_move_packages: vec![],
            genesis,
        }
    }
}

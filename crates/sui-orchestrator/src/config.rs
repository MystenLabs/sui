// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use move_core_types::account_address::AccountAddress;
use multiaddr::Multiaddr;
use rand::{rngs::StdRng, SeedableRng};
use sui_config::{
    genesis::GenesisChainParameters,
    genesis_config::{
        AccountConfig, GenesisConfig, ObjectConfigRange, ValidatorConfigInfo, ValidatorGenesisInfo,
    },
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{AuthorityKeyPair, KeypairTraits, NetworkKeyPair, SuiKeyPair},
};

use crate::client::Instance;

/// All configurations information to run a sui client or validator.
// TODO: This module is very ad-hoc and hard to read, needs refactoring.
pub struct Config {
    genesis_config: GenesisConfig,
    keystore: FileBasedKeystore,
    pub listen_addresses: Vec<Multiaddr>,
}

impl Config {
    pub const GENESIS_CONFIG_FILE: &'static str = "benchmark-genesis.yml";
    pub const GAS_KEYSTORE_FILE: &'static str = "gas.keystore";

    pub fn new(instances: &[Instance]) -> Self {
        let mut rng = StdRng::seed_from_u64(0);

        let gas_key = SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng));
        let gas_address = SuiAddress::from(&gas_key.public());
        let genesis_config = Self::make_genesis_config(instances, gas_address);
        let listen_addresses = Self::parse_listen_address(&genesis_config);

        let mut keystore = FileBasedKeystore::default();
        keystore.add_key(gas_key).unwrap();

        Self {
            genesis_config,
            keystore,
            listen_addresses,
        }
    }

    pub fn gas_object_id_offsets(quantity: usize) -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(0);
        (0..quantity)
            .map(|_| format!("{:#x}", ObjectID::random_from_rng(&mut rng)))
            .collect()
    }

    pub fn print_files(&mut self) {
        let yaml = serde_yaml::to_string(&self.genesis_config).unwrap();
        let path = PathBuf::from(Self::GENESIS_CONFIG_FILE);
        fs::write(path, yaml).unwrap();

        let path = PathBuf::from(Self::GAS_KEYSTORE_FILE);
        self.keystore.set_path(&path);
        self.keystore.save().unwrap();
    }

    pub fn files(&self) -> Vec<PathBuf> {
        vec![
            Self::GENESIS_CONFIG_FILE.into(),
            Self::GAS_KEYSTORE_FILE.into(),
        ]
    }

    pub fn genesis_command(&self) -> String {
        let genesis = format!("~/{}", Self::GENESIS_CONFIG_FILE);
        format!("cargo run --release --bin sui -- genesis -f --from-config {genesis}")
    }

    // Generate a genesis configuration file suitable for benchmarks.
    fn make_genesis_config(instances: &[Instance], gas_address: SuiAddress) -> GenesisConfig {
        let mut rng = StdRng::seed_from_u64(0);

        // Set the validator's configs.
        let validator_config_info: Vec<_> = instances
            .iter()
            .map(|instance| {
                ValidatorConfigInfo {
                    consensus_address: "/ip4/127.0.0.1/tcp/8083/http".parse().unwrap(),
                    consensus_internal_worker_address: None,
                    genesis_info: ValidatorGenesisInfo::from_base_ip(
                        AuthorityKeyPair::generate(&mut rng), // key_pair
                        NetworkKeyPair::generate(&mut rng),   // worker_key_pair
                        SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng)), // account_key_pair
                        NetworkKeyPair::generate(&mut rng),   // network_key_pair
                        Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))), // p2p_listen_address
                        instance.main_ip.to_string(),
                        500, // port_offset
                    ),
                }
            })
            .collect();

        // Generate the genesis gas objects.
        let genesis_gas_objects = Self::gas_object_id_offsets(instances.len())
            .iter()
            .map(|id| ObjectConfigRange {
                offset: AccountAddress::from_hex_literal(id).unwrap().into(),
                count: 5000,
                gas_value: 18446744073709551615,
            })
            .collect();

        // Set the initial gas objects.
        let account_config = AccountConfig {
            address: Some(gas_address),
            gas_objects: vec![],
            gas_object_ranges: Some(genesis_gas_objects),
        };

        // Make the genesis configuration file.
        GenesisConfig {
            validator_config_info: Some(validator_config_info),
            parameters: GenesisChainParameters::new(),
            committee_size: instances.len(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            accounts: vec![account_config],
        }
    }

    fn parse_listen_address(genesis_config: &GenesisConfig) -> Vec<Multiaddr> {
        let mut addresses = Vec::new();
        if let Some(validator_configs) = genesis_config.validator_config_info.as_ref() {
            for validator_info in validator_configs {
                let address = &validator_info.genesis_info.network_address;
                addresses.push(Self::zero_ip_multi_address(address));
            }
        }
        addresses
    }

    /// Set the ip address to `0.0.0.0`. For instance, it converts the following address
    /// `/ip4/155.138.174.208/tcp/1500/http` into `/ip4/0.0.0.0/tcp/1500/http`.
    fn zero_ip_multi_address(address: &Multiaddr) -> Multiaddr {
        let mut new_address = Multiaddr::empty();
        for component in address {
            match component {
                multiaddr::Protocol::Ip4(_) => new_address.push(multiaddr::Protocol::Ip4(
                    std::net::Ipv4Addr::new(0, 0, 0, 0),
                )),
                c => new_address.push(c),
            }
        }
        new_address
    }
}

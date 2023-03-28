// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use move_core_types::account_address::AccountAddress;
use rand::{rngs::StdRng, SeedableRng};
use sui_config::{
    genesis::GenesisCeremonyParameters,
    genesis_config::{
        AccountConfig, GenesisConfig, ObjectConfigRange, ValidatorConfigInfo, ValidatorGenesisInfo,
    },
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{AuthorityKeyPair, KeypairTraits, NetworkKeyPair, SuiKeyPair},
    multiaddr::Multiaddr,
};

use crate::{benchmark::BenchmarkParameters, client::Instance, settings::Settings};

use super::{ProtocolCommands, ProtocolMetrics};

/// All configurations information to run a sui client or validator.
pub struct SuiProtocol {
    /// The working directory on the remote hosts (containing the databases and configuration files).
    working_dir: PathBuf,
}

impl ProtocolCommands for SuiProtocol {
    const CLIENT_METRICS_PORT: u16 = 8081;

    fn protocol_dependencies() -> Vec<&'static str> {
        vec![
            // Install typical sui dependencies.
            "sudo apt-get -y install curl git-all clang cmake gcc libssl-dev pkg-config libclang-dev",
            // This dependency is missing from the Sui docs.
            "sudo apt-get -y install libpq-dev",
        ]
    }

    fn db_directories(&self) -> Vec<PathBuf> {
        let authorities_db = [&self.working_dir, &Self::AUTHORITIES_DB.into()]
            .iter()
            .collect();
        let consensus_db = [&self.working_dir, &Self::CONSENSUS_DB.into()]
            .iter()
            .collect();
        vec![authorities_db, consensus_db]
    }

    fn genesis_command<'a, I>(&self, instances: I) -> String
    where
        I: Iterator<Item = &'a Instance>,
    {
        let working_dir = self.working_dir.display();
        let ips = instances
            .map(|x| x.main_ip.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let genesis = [
            "cargo run --release --bin sui --",
            "genesis",
            &format!("-f --working-dir {working_dir} --benchmark-ips {ips}"),
        ]
        .join(" ");

        [
            &format!("mkdir -p {working_dir}"),
            "source $HOME/.cargo/env",
            &genesis,
        ]
        .join(" && ")
    }

    fn node_command<'a, I>(&self, instances: I) -> Box<dyn Fn(usize) -> String>
    where
        I: Iterator<Item = &'a Instance>,
    {
        let instances: Vec<_> = instances.cloned().collect();
        let listen_addresses = Self::make_listen_addresses(&instances);

        let working_dir = self.working_dir.clone();
        Box::new(move |i| {
            let validator_config = SuiProtocol::validator_configs(i);
            let config_path: PathBuf = [&working_dir, &validator_config.into()].iter().collect();
            let path = config_path.display();
            let address = listen_addresses[i].clone();

            let run = [
                "cargo run --release --bin sui-node --",
                &format!("--config-path {path} --listen-address {address}"),
            ]
            .join(" ");
            ["source $HOME/.cargo/env", &run].join(" && ")
        })
    }

    fn client_command<'a, I>(
        &self,
        _instances: I,
        parameters: &BenchmarkParameters,
    ) -> Box<dyn Fn(usize) -> String>
    where
        I: Iterator<Item = &'a Instance>,
    {
        let genesis_path: PathBuf = [&self.working_dir, &Self::GENESIS_BLOB.into()]
            .iter()
            .collect();
        let keystore_path: PathBuf = [&self.working_dir, &Self::GAS_KEYSTORE_FILE.into()]
            .iter()
            .collect();
        let committee_size = parameters.nodes;
        let load_share = parameters.load / committee_size;
        let shared_counter = parameters.shared_objects_ratio;
        let transfer_objects = 100 - shared_counter;
        let metrics_port = Self::CLIENT_METRICS_PORT;

        Box::new(move |i| {
            let genesis = genesis_path.display();
            let keystore = keystore_path.display();
            // let gas_id = SuiProtocol::gas_object_id_offsets(committee_size)[i].clone();
            let gas_id = GenesisConfig::benchmark_gas_object_id_offsets(committee_size)[i].clone();
            let run = [
                "cargo run --release --bin stress --",
                "--num-client-threads 24 --num-server-threads 1",
                "--local false --num-transfer-accounts 2",
                &format!("--genesis-blob-path {genesis} --keystore-path {keystore}",),
                &format!("--primary-gas-id {gas_id}"),
                "bench",
                &format!("--in-flight-ratio 30 --num-workers 24 --target-qps {load_share}"),
                &format!("--shared-counter {shared_counter} --transfer-object {transfer_objects}"),
                &format!("--client-metric-host 0.0.0.0 --client-metric-port {metrics_port}"),
            ]
            .join(" ");
            ["source $HOME/.cargo/env", &run].join(" && ")
        })
    }
}

impl ProtocolMetrics for SuiProtocol {
    const BENCHMARK_DURATION: &'static str = "benchmark_duration";
    const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
    const LATENCY_BUCKETS: &'static str = "latency_s";
    const LATENCY_SUM: &'static str = "latency_s_sum";
    const LATENCY_SQUARED_SUM: &'static str = "latency_squared_s";
}

impl SuiProtocol {
    /// Make a new instance of the Sui protocol commands generator.
    pub fn new(settings: &Settings) -> Self {
        Self {
            working_dir: [&settings.working_dir, &Self::CONFIG_DIR.into()]
                .iter()
                .collect(),
        }
    }

    /// Convert the ip of the validators' network addresses to 0.0.0.0.
    pub fn make_listen_addresses(instances: &[Instance]) -> Vec<Multiaddr> {
        let ips: Vec<_> = instances.iter().map(|x| x.main_ip.to_string()).collect();
        let genesis_config = GenesisConfig::new_for_benchmarks(&ips);
        let mut addresses = Vec::new();
        if let Some(validator_configs) = genesis_config.validator_config_info.as_ref() {
            for validator_info in validator_configs {
                let address = &validator_info.genesis_info.network_address;
                addresses.push(address.zero_ip_multi_address());
            }
        }
        addresses
    }
}

/// TODO: All these functions and variables are already defined in other parts of the codebase
/// or should not be needed after #9695 lands.
impl SuiProtocol {
    const AUTHORITIES_DB: &str = "authorities_db";
    const CONSENSUS_DB: &str = "consensus_db";
    const CONFIG_DIR: &str = "sui_config";
    const GENESIS_BLOB: &str = "genesis.blob";
    const GENESIS_CONFIG_FILE: &str = "benchmark-genesis.yml";
    pub const GAS_KEYSTORE_FILE: &str = "benchmark.keystore";

    pub fn validator_configs(i: usize) -> String {
        format!("validator-config-{i}.yaml")
    }

    fn gas_key() -> SuiKeyPair {
        let mut rng = StdRng::seed_from_u64(0);
        SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng))
    }

    pub fn gas_object_id_offsets(quantity: usize) -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(0);
        (0..quantity)
            .map(|_| format!("{:#x}", ObjectID::random_from_rng(&mut rng)))
            .collect()
    }

    pub fn print_files(instances: &[Instance]) {
        let ips: Vec<_> = instances.iter().map(|x| x.main_ip.to_string()).collect();
        let genesis_config = GenesisConfig::new_for_benchmarks(&ips);
        // let genesis_config = Self::make_genesis_config(instances);
        let yaml = serde_yaml::to_string(&genesis_config).unwrap();
        let path = PathBuf::from(Self::GENESIS_CONFIG_FILE);
        fs::write(path, yaml).unwrap();

        let mut keystore = FileBasedKeystore::default();
        // let gas_key = Self::gas_key();
        let gas_key = GenesisConfig::benchmark_gas_key();
        keystore.add_key(gas_key).unwrap();
        keystore.set_path(&PathBuf::from(Self::GAS_KEYSTORE_FILE));
        keystore.save().unwrap();
    }

    pub fn configuration_files() -> Vec<PathBuf> {
        vec![
            Self::GENESIS_CONFIG_FILE.into(),
            // Self::GAS_KEYSTORE_FILE.into(),
        ]
    }

    pub fn genesis_config_command(&self, instances: &[Instance]) -> String {
        let ips = instances
            .iter()
            .map(|x| x.main_ip.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let genesis_file = format!("~/{}", Self::GENESIS_CONFIG_FILE);
        let genesis = [
            "cargo run --release --bin sui --",
            // &format!(
            //     "genesis -f --from-config {genesis_file} --working-dir {} --benchmark-ips {ips}",
            //     self.working_dir.display()
            // ),
            &format!(
                "genesis -f  --working-dir {} --benchmark-ips {ips}",
                self.working_dir.display()
            ),
        ]
        .join(" ");
        [format!("mkdir -p {}", self.working_dir.display()), genesis].join(" && ")
    }

    /// Generate a genesis configuration file suitable for benchmarks.
    pub fn make_genesis_config(instances: &[Instance]) -> GenesisConfig {
        let gas_key = Self::gas_key();
        let gas_address = SuiAddress::from(&gas_key.public());

        // Set the validator's configs.
        let mut rng = StdRng::seed_from_u64(0);
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
            parameters: GenesisCeremonyParameters::new(),
            committee_size: instances.len(),
            grpc_load_shed: None,
            grpc_concurrency_limit: None,
            accounts: vec![account_config],
        }
    }
}

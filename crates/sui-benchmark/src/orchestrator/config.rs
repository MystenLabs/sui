use std::{fs, path::PathBuf};

use move_core_types::account_address::AccountAddress;
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

use super::state::Instance;

pub struct Config {
    genesis_config: GenesisConfig,
    keystore: FileBasedKeystore,
}

impl Config {
    // pub const GAS_OBJECT_ID_OFFSET: &'static str = "0x59931dcac57ba20d75321acaf55e8eb5a2c47e9f";
    pub const GENESIS_CONFIG_FILE: &'static str = "benchmark-genesis.yml";
    pub const GAS_KEYSTORE_FILE: &'static str = "gas.keystore";

    pub fn new(instances: &[Instance]) -> Self {
        let mut rng = StdRng::seed_from_u64(0);

        let gas_key = SuiKeyPair::Ed25519(NetworkKeyPair::generate(&mut rng));
        let gas_address = SuiAddress::from(&gas_key.public());
        let genesis_config = Self::make_genesis_config(instances, gas_address);

        let mut keystore = FileBasedKeystore::default();
        keystore.add_key(gas_key).unwrap();

        Self {
            genesis_config,
            keystore,
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
}

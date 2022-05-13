// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use debug_ignore::DebugIgnore;
use multiaddr::Multiaddr;
use narwhal_config::Parameters as ConsensusParameters;
use narwhal_config::{
    Authority, Committee as ConsensusCommittee, PrimaryAddresses, Stake, WorkerAddresses,
};
use narwhal_crypto::ed25519::Ed25519PublicKey;
use rand::rngs::OsRng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_types::base_types::{encode_bytes_hex, ObjectID, SuiAddress};
use sui_types::committee::{Committee, EpochId};
use sui_types::crypto::{get_key_pair_from_rng, KeyPair, PublicKeyBytes};
use tracing::trace;

pub mod utils;

const DEFAULT_STAKE: usize = 1;

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorConfig {
    key_pair: KeyPair,
    db_path: PathBuf,
    network_address: Multiaddr,
    metrics_address: Multiaddr,

    consensus_config: ConsensuseConfig,
    committee_config: CommitteeConfig,
}

impl Config for ValidatorConfig {}

impl ValidatorConfig {
    pub fn key_pair(&self) -> &KeyPair {
        &self.key_pair
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        *self.key_pair.public_key_bytes()
    }

    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }

    pub fn consensus_config(&self) -> &ConsensuseConfig {
        &self.consensus_config
    }

    pub fn committee_config(&self) -> &CommitteeConfig {
        &self.committee_config
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsensuseConfig {
    consensus_address: Multiaddr,
    consensus_db_path: PathBuf,

    //TODO make narwhal config serializable
    #[serde(skip_serializing)]
    #[serde(default)]
    narwhal_config: DebugIgnore<ConsensusParameters>,
}

impl ConsensuseConfig {
    pub fn address(&self) -> &Multiaddr {
        &self.consensus_address
    }

    pub fn db_path(&self) -> &Path {
        &self.consensus_db_path
    }

    pub fn narwhal_config(&self) -> &ConsensusParameters {
        &self.narwhal_config
    }
}

//TODO get this information from on-chain + some way to do network discovery
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitteeConfig {
    epoch: EpochId,
    validator_set: Vec<ValidatorInfo>,
    consensus_committee: DebugIgnore<ConsensusCommittee<Ed25519PublicKey>>,
}

impl CommitteeConfig {
    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        &self.validator_set
    }

    pub fn narwhal_committee(&self) -> &ConsensusCommittee<Ed25519PublicKey> {
        &self.consensus_committee
    }

    pub fn committee(&self) -> Committee {
        let voting_rights = self
            .validator_set()
            .iter()
            .map(|validator| (validator.public_key(), validator.stake()))
            .collect();
        Committee::new(self.epoch(), voting_rights)
    }
}

/// Publicly known information about a validator
/// TODO read most of this from on-chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ValidatorInfo {
    public_key: PublicKeyBytes,
    stake: usize,
    network_address: Multiaddr,
}

impl ValidatorInfo {
    pub fn sui_address(&self) -> SuiAddress {
        SuiAddress::from(self.public_key())
    }

    pub fn public_key(&self) -> PublicKeyBytes {
        self.public_key
    }

    pub fn stake(&self) -> usize {
        self.stake
    }

    pub fn network_address(&self) -> &Multiaddr {
        &self.network_address
    }
}

/// This is a config that is used for testing or local use as it contains the config and keys for
/// all validators
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    validator_configs: Vec<ValidatorConfig>,
    loaded_move_packages: Vec<(PathBuf, ObjectID)>,
}

impl Config for NetworkConfig {}

impl NetworkConfig {
    pub fn validator_configs(&self) -> &[ValidatorConfig] {
        &self.validator_configs
    }

    pub fn loaded_move_packages(&self) -> &[(PathBuf, ObjectID)] {
        &self.loaded_move_packages
    }

    pub fn add_move_package(&mut self, path: PathBuf, object_id: ObjectID) {
        self.loaded_move_packages.push((path, object_id))
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        self.validator_configs()[0]
            .committee_config()
            .validator_set()
    }

    pub fn committee(&self) -> Committee {
        self.validator_configs()[0].committee_config().committee()
    }

    pub fn into_validator_configs(self) -> Vec<ValidatorConfig> {
        self.validator_configs
    }

    pub fn generate_with_rng<R: rand::CryptoRng + rand::RngCore>(
        config_dir: &Path,
        quorum_size: usize,
        mut rng: R,
    ) -> Self {
        let epoch = 0;

        let keys = (0..quorum_size)
            .map(|_| get_key_pair_from_rng(&mut rng).1)
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
                let db_path = config_dir
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
                let consensus_db_path = config_dir
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
                }
            })
            .collect();

        Self {
            validator_configs,
            loaded_move_packages: vec![],
        }
    }

    pub fn generate(config_dir: &Path, quorum_size: usize) -> Self {
        Self::generate_with_rng(config_dir, quorum_size, OsRng)
    }
}

// pub struct ConfigBuilder<R> {
//     rng: R,

// }

fn new_network_address() -> Multiaddr {
    format!("/dns/localhost/tcp/{}/http", utils::get_available_port())
        .parse()
        .unwrap()
}

const SUI_DIR: &str = ".sui";
const SUI_CONFIG_DIR: &str = "sui_config";
pub const SUI_NETWORK_CONFIG: &str = "network.conf";
pub const SUI_WALLET_CONFIG: &str = "wallet.conf";
pub const SUI_GATEWAY_CONFIG: &str = "gateway.conf";
pub const SUI_DEV_NET_URL: &str = "https://gateway.devnet.sui.io:9000";

pub const AUTHORITIES_DB_NAME: &str = "authorities_db";
pub const DEFAULT_STARTING_PORT: u16 = 10000;
pub const CONSENSUS_DB_NAME: &str = "consensus_db";

pub fn sui_config_dir() -> Result<PathBuf, anyhow::Error> {
    match std::env::var_os("SUI_CONFIG_DIR") {
        Some(config_env) => Ok(config_env.into()),
        None => match dirs::home_dir() {
            Some(v) => Ok(v.join(SUI_DIR).join(SUI_CONFIG_DIR)),
            None => anyhow::bail!("Cannot obtain home directory path"),
        },
    }
    .and_then(|dir| {
        if !dir.exists() {
            std::fs::create_dir_all(dir.clone())?;
        }
        Ok(dir)
    })
}

#[derive(Serialize, Deserialize)]
pub struct GenesisConfig {
    pub committee_size: usize,
    pub accounts: Vec<AccountConfig>,
    pub move_packages: Vec<PathBuf>,
    pub sui_framework_lib_path: PathBuf,
    pub move_framework_lib_path: PathBuf,
}

impl Config for GenesisConfig {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccountConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "SuiAddress::optional_address_as_hex",
        deserialize_with = "SuiAddress::optional_address_from_hex"
    )]
    pub address: Option<SuiAddress>,
    pub gas_objects: Vec<ObjectConfig>,
    pub gas_object_ranges: Option<Vec<ObjectConfigRange>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfigRange {
    /// Starting object id
    pub offset: ObjectID,
    /// Number of object ids
    pub count: u64,
    /// Gas value per object id
    pub gas_value: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfig {
    #[serde(default = "ObjectID::random")]
    pub object_id: ObjectID,
    #[serde(default = "default_gas_value")]
    pub gas_value: u64,
}

fn default_gas_value() -> u64 {
    DEFAULT_GAS_AMOUNT
}

const DEFAULT_GAS_AMOUNT: u64 = 100000;
const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    pub fn for_local_testing() -> Result<Self, anyhow::Error> {
        Self::custom_genesis(
            DEFAULT_NUMBER_OF_AUTHORITIES,
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn custom_genesis(
        num_authorities: usize,
        num_accounts: usize,
        num_objects_per_account: usize,
    ) -> Result<Self, anyhow::Error> {
        assert!(
            num_authorities > 0,
            "num_authorities should be larger than 0"
        );

        let mut accounts = Vec::new();
        for _ in 0..num_accounts {
            let mut objects = Vec::new();
            for _ in 0..num_objects_per_account {
                objects.push(ObjectConfig {
                    object_id: ObjectID::random(),
                    gas_value: DEFAULT_GAS_AMOUNT,
                })
            }
            accounts.push(AccountConfig {
                address: None,
                gas_objects: objects,
                gas_object_ranges: Some(Vec::new()),
            })
        }

        Ok(Self {
            accounts,
            ..Default::default()
        })
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            committee_size: DEFAULT_NUMBER_OF_AUTHORITIES,
            accounts: vec![],
            move_packages: vec![],
            sui_framework_lib_path: PathBuf::from(DEFAULT_FRAMEWORK_PATH),
            move_framework_lib_path: PathBuf::from(DEFAULT_FRAMEWORK_PATH)
                .join("deps")
                .join("move-stdlib"),
        }
    }
}

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn persisted(self, path: &Path) -> PersistedConfig<Self> {
        PersistedConfig {
            inner: self,
            path: path.to_path_buf(),
        }
    }
}

pub struct PersistedConfig<C> {
    inner: C,
    path: PathBuf,
}

impl<C> PersistedConfig<C>
where
    C: Config,
{
    pub fn read(path: &Path) -> Result<C, anyhow::Error> {
        trace!("Reading config from '{:?}'", path);
        let reader = fs::File::open(path)?;
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", &self.path);
        let config = serde_json::to_string_pretty(&self.inner)?;
        fs::write(&self.path, config)?;
        Ok(())
    }
}

impl<C> std::ops::Deref for PersistedConfig<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<C> std::ops::DerefMut for PersistedConfig<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

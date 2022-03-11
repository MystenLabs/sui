// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gateway::{EmbeddedGatewayConfig, GatewayType};
use crate::keystore::KeystoreType;
use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::hex::Hex;
use serde_with::serde_as;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_network::network::PortAllocator;
use sui_network::transport;
use sui_types::base_types::*;
use sui_types::crypto::{get_key_pair, KeyPair};
use tracing::log::trace;

const DEFAULT_WEIGHT: usize = 1;
const DEFAULT_GAS_AMOUNT: u64 = 100000;
pub const AUTHORITIES_DB_NAME: &str = "authorities_db";
pub const DEFAULT_STARTING_PORT: u16 = 10000;

static PORT_ALLOCATOR: Lazy<Mutex<PortAllocator>> =
    Lazy::new(|| Mutex::new(PortAllocator::new(DEFAULT_STARTING_PORT)));

#[derive(Serialize, Deserialize)]
pub struct AuthorityInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub name: AuthorityName,
    pub host: String,
    pub base_port: u16,
}

#[derive(Serialize)]
pub struct AuthorityPrivateInfo {
    pub key_pair: KeyPair,
    pub host: String,
    pub port: u16,
    pub db_path: PathBuf,
    pub stake: usize,
}

// Custom deserializer with optional default fields
impl<'de> Deserialize<'de> for AuthorityPrivateInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let (_, new_key_pair) = get_key_pair();

        let json = Value::deserialize(deserializer)?;
        let key_pair = if let Some(val) = json.get("key_pair") {
            KeyPair::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            new_key_pair
        };
        let host = if let Some(val) = json.get("host") {
            String::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            "127.0.0.1".to_string()
        };
        let port = if let Some(val) = json.get("port") {
            u16::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            PORT_ALLOCATOR
                .lock()
                .map_err(serde::de::Error::custom)?
                .next_port()
                .ok_or_else(|| serde::de::Error::custom("No available port."))?
        };
        let db_path = if let Some(val) = json.get("db_path") {
            PathBuf::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            PathBuf::from(".")
                .join(AUTHORITIES_DB_NAME)
                .join(encode_bytes_hex(key_pair.public_key_bytes()))
        };
        let stake = if let Some(val) = json.get("stake") {
            usize::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            DEFAULT_WEIGHT
        };

        Ok(AuthorityPrivateInfo {
            key_pair,
            host,
            port,
            db_path,
            stake,
        })
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct WalletConfig {
    #[serde_as(as = "Vec<Hex>")]
    pub accounts: Vec<SuiAddress>,
    pub keystore: KeystoreType,
    pub gateway: GatewayType,

    #[serde(skip)]
    config_path: PathBuf,
}

impl Config for WalletConfig {
    fn create(path: &Path) -> Result<Self, anyhow::Error> {
        let working_dir = path
            .parent()
            .ok_or_else(|| anyhow!("Cannot determine parent directory."))?;
        Ok(WalletConfig {
            accounts: Vec::new(),
            keystore: KeystoreType::File(working_dir.join("wallet.ks")),
            gateway: GatewayType::Embedded(EmbeddedGatewayConfig {
                db_folder_path: working_dir.join("client_db"),
                ..Default::default()
            }),
            config_path: path.to_path_buf(),
        })
    }

    fn set_config_path(&mut self, path: &Path) {
        self.config_path = path.to_path_buf();
    }

    fn config_path(&self) -> &Path {
        &self.config_path
    }
}

impl Display for WalletConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();

        writeln!(writer, "Config path : {:?}", self.config_path)?;
        writeln!(writer, "Managed addresses : {}", self.accounts.len())?;
        write!(writer, "{}", self.gateway)?;

        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize)]
pub struct NetworkConfig {
    pub authorities: Vec<AuthorityPrivateInfo>,
    pub buffer_size: usize,
    pub loaded_move_packages: Vec<(PathBuf, ObjectID)>,
    #[serde(skip)]
    config_path: PathBuf,
}

impl Config for NetworkConfig {
    fn create(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(Self {
            authorities: vec![],
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
            loaded_move_packages: vec![],
            config_path: path.to_path_buf(),
        })
    }

    fn set_config_path(&mut self, path: &Path) {
        self.config_path = path.to_path_buf();
    }

    fn config_path(&self) -> &Path {
        &self.config_path
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GenesisConfig {
    pub authorities: Vec<AuthorityPrivateInfo>,
    pub accounts: Vec<AccountConfig>,
    pub move_packages: Vec<PathBuf>,
    #[serde(default = "default_sui_framework_lib")]
    pub sui_framework_lib_path: PathBuf,
    #[serde(default = "default_move_framework_lib")]
    pub move_framework_lib_path: PathBuf,
    #[serde(skip)]
    config_path: PathBuf,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AccountConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "SuiAddress::optional_address_as_hex",
        deserialize_with = "SuiAddress::optional_address_from_hex"
    )]
    pub address: Option<SuiAddress>,
    pub gas_objects: Vec<ObjectConfig>,
}

#[derive(Serialize, Deserialize)]
pub struct ObjectConfig {
    #[serde(default = "ObjectID::random")]
    pub object_id: ObjectID,
    #[serde(default = "default_gas_value")]
    pub gas_value: u64,
}

fn default_gas_value() -> u64 {
    DEFAULT_GAS_AMOUNT
}

fn default_sui_framework_lib() -> PathBuf {
    PathBuf::from(DEFAULT_FRAMEWORK_PATH)
}

fn default_move_framework_lib() -> PathBuf {
    PathBuf::from(DEFAULT_FRAMEWORK_PATH)
        .join("deps")
        .join("move-stdlib")
}

const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    pub fn default_genesis(path: &Path) -> Result<Self, anyhow::Error> {
        GenesisConfig::custom_genesis(
            path,
            DEFAULT_NUMBER_OF_AUTHORITIES,
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn custom_genesis(
        path: &Path,
        num_authorities: usize,
        num_accounts: usize,
        num_objects_per_account: usize,
    ) -> Result<Self, anyhow::Error> {
        let working_dir = path
            .parent()
            .ok_or_else(|| anyhow!("Cannot resolve file path."))?;
        let mut authorities = Vec::new();
        for _ in 0..num_authorities {
            // Get default authority config from deserialization logic.
            let mut authority = AuthorityPrivateInfo::deserialize(Value::String(String::new()))?;
            authority.db_path = working_dir
                .join(AUTHORITIES_DB_NAME)
                .join(encode_bytes_hex(&authority.key_pair.public_key_bytes()));
            authorities.push(authority)
        }
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
            })
        }
        Ok(Self {
            authorities,
            accounts,
            move_packages: vec![],
            sui_framework_lib_path: default_sui_framework_lib(),
            move_framework_lib_path: default_move_framework_lib(),
            config_path: path.to_path_buf(),
        })
    }
}

impl Config for GenesisConfig {
    fn create(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(Self {
            authorities: vec![],
            accounts: vec![],
            config_path: path.to_path_buf(),
            move_packages: vec![],
            sui_framework_lib_path: default_sui_framework_lib(),
            move_framework_lib_path: default_move_framework_lib(),
        })
    }

    fn set_config_path(&mut self, path: &Path) {
        self.config_path = path.to_path_buf()
    }

    fn config_path(&self) -> &Path {
        &self.config_path
    }
}

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn read_or_create(path: &Path) -> Result<Self, anyhow::Error> {
        let path_buf = PathBuf::from(path);
        Ok(if path_buf.exists() {
            Self::read(path)?
        } else {
            trace!("Config file not found, creating new config '{:?}'", path);
            let new_config = Self::create(path)?;
            new_config.write(path)?;
            new_config
        })
    }

    fn read(path: &Path) -> Result<Self, anyhow::Error> {
        trace!("Reading config from '{:?}'", path);
        let reader = BufReader::new(File::open(path)?);
        let mut config: Self = serde_json::from_reader(reader)?;
        config.set_config_path(path);
        Ok(config)
    }

    fn write(&self, path: &Path) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", path);
        let config = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, config).expect("Unable to write to config file");
        Ok(())
    }

    fn save(&self) -> Result<(), anyhow::Error> {
        self.write(self.config_path())
    }

    fn create(path: &Path) -> Result<Self, anyhow::Error>;

    fn set_config_path(&mut self, path: &Path);
    fn config_path(&self) -> &Path;
}

// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority_client::AuthorityClient;
use sui_network::network::NetworkClient;
use sui_types::base_types::*;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, KeyPair};
use sui_types::error::SuiError;

use crate::utils::optional_address_as_hex;
use crate::utils::optional_address_from_hex;
use crate::utils::{Config, PortAllocator, DEFAULT_STARTING_PORT};
use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_network::transport;

const DEFAULT_WEIGHT: usize = 1;
const DEFAULT_GAS_AMOUNT: u64 = 100000;
pub const AUTHORITIES_DB_NAME: &str = "authorities_db";

static PORT_ALLOCATOR: Lazy<Mutex<PortAllocator>> =
    Lazy::new(|| Mutex::new(PortAllocator::new(DEFAULT_STARTING_PORT)));

#[derive(Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub address: SuiAddress,
    pub key_pair: KeyPair,
}

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

#[derive(Serialize, Deserialize)]
pub struct WalletConfig {
    pub accounts: Vec<AccountInfo>,
    pub authorities: Vec<AuthorityInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: PathBuf,

    #[serde(skip)]
    config_path: PathBuf,
}

impl Config for WalletConfig {
    fn create(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(WalletConfig {
            accounts: Vec::new(),
            authorities: Vec::new(),
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
            db_folder_path: PathBuf::from("./client_db"),
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

impl WalletConfig {
    pub fn make_committee(&self) -> Committee {
        let voting_rights = self
            .authorities
            .iter()
            .map(|authority| (authority.name, 1))
            .collect();
        Committee::new(voting_rights)
    }

    pub fn make_authority_clients(&self) -> BTreeMap<AuthorityName, AuthorityClient> {
        let mut authority_clients = BTreeMap::new();
        for authority in &self.authorities {
            let client = AuthorityClient::new(NetworkClient::new(
                authority.host.clone(),
                authority.base_port,
                self.buffer_size,
                self.send_timeout,
                self.recv_timeout,
            ));
            authority_clients.insert(authority.name, client);
        }
        authority_clients
    }
    pub fn get_account_cfg_info(&self, address: &SuiAddress) -> Result<&AccountInfo, SuiError> {
        self.accounts
            .iter()
            .find(|info| &info.address == address)
            .ok_or(SuiError::AccountNotFound)
    }
}

impl Display for WalletConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Config path : {:?}\nClient state DB folder path : {:?}\nManaged addresses : {}",
            self.config_path,
            self.db_folder_path,
            self.accounts.len()
        )
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
        serialize_with = "optional_address_as_hex",
        deserialize_with = "optional_address_from_hex"
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
        let working_dir = path.parent().ok_or(anyhow!("Cannot resolve file path."))?;
        let mut authorities = Vec::new();
        for _ in 0..DEFAULT_NUMBER_OF_AUTHORITIES {
            // Get default authority config from deserialization logic.
            let mut authority = AuthorityPrivateInfo::deserialize(Value::String(String::new()))?;
            authority.db_path = working_dir
                .join(AUTHORITIES_DB_NAME)
                .join(encode_bytes_hex(&authority.key_pair.public_key_bytes()));
            authorities.push(authority)
        }
        let mut accounts = Vec::new();
        for _ in 0..DEFAULT_NUMBER_OF_ACCOUNT {
            let mut objects = Vec::new();
            for _ in 0..DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT {
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

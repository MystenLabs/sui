// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::*;

use crate::utils::Config;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sui_network::transport;

#[derive(Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub address: SuiAddress,
    pub key_pair: KeyPair,
}

#[derive(Serialize, Deserialize)]
pub struct AuthorityInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub address: SuiAddress,
    pub host: String,
    pub base_port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct AuthorityPrivateInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub address: SuiAddress,
    pub key_pair: KeyPair,
    pub host: String,
    pub port: u16,
    pub db_path: PathBuf,
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
    #[serde(skip)]
    config_path: PathBuf,
}

impl Config for NetworkConfig {
    fn create(path: &Path) -> Result<Self, anyhow::Error> {
        Ok(Self {
            authorities: Vec::new(),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
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

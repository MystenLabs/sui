// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_types::{
    base_types::*,
    messages::{Order, OrderKind},
};

use crate::utils::Config;
use fastx_network::transport;

use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
};
use std::{
    fs::{self, File},
    io::BufReader,
};

pub struct KeyPairConfig {}
impl KeyPairConfig {
    pub fn sign_order(path: &str, order_kind: OrderKind) -> Result<Order, anyhow::Error> {
        tracing::log::trace!("Reading keyfile config from '{}'", path);
        let reader = BufReader::new(File::open(PathBuf::from(path))?);
        let cfg: KeyPair = serde_json::from_reader(reader)?;

        Ok(Order::new(order_kind, &cfg))
    }
    pub fn create_and_get_public_key() -> (String, FastPayAddress) {
        let key_pair = get_key_pair();
        let pk = key_pair.0;
        let mut hasher = sha3::Sha3_256::default();
        sha3::Digest::update(&mut hasher, pk);
        let path = format!("{:02X}.kf", sha3::Digest::finalize(hasher));

        tracing::log::trace!("Writing keypair config to '{}'", path);
        fs::write(path.clone(), serde_json::to_string(&key_pair.1).unwrap())
            .expect("Unable to write to config file");
        drop(key_pair);
        (path, pk)
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccountInfoConfig {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub key_file_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct AuthorityInfo {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub host: String,
    pub base_port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct AuthorityPrivateInfo {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub key_pair: KeyPair,
    pub host: String,
    pub port: u16,
    pub db_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct WalletConfig {
    pub accounts: Vec<AccountInfoConfig>,
    pub authorities: Vec<AuthorityInfo>,
    pub send_timeout: Duration,
    pub recv_timeout: Duration,
    pub buffer_size: usize,
    pub db_folder_path: String,

    #[serde(skip)]
    config_path: String,
}

impl Config for WalletConfig {
    fn create(path: &str) -> Result<Self, anyhow::Error> {
        Ok(WalletConfig {
            accounts: Vec::new(),
            authorities: Vec::new(),
            send_timeout: Duration::from_micros(4000000),
            recv_timeout: Duration::from_micros(4000000),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
            db_folder_path: "./client_db".to_string(),
            config_path: path.to_string(),
        })
    }

    fn set_config_path(&mut self, path: &str) {
        self.config_path = path.to_string();
    }

    fn config_path(&self) -> &str {
        &*self.config_path
    }
}

impl Display for WalletConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Config path : {}\nClient state DB folder path : {}\nManaged addresses : {}",
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
    config_path: String,
}

impl Config for NetworkConfig {
    fn create(path: &str) -> Result<Self, anyhow::Error> {
        Ok(Self {
            authorities: Vec::new(),
            buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
            config_path: path.to_string(),
        })
    }

    fn set_config_path(&mut self, path: &str) {
        self.config_path = path.to_string()
    }

    fn config_path(&self) -> &str {
        &*self.config_path
    }
}

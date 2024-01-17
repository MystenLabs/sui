// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use sui_config::Config;
use sui_types::crypto::{get_account_key_pair, SuiKeyPair};

pub const DEFAULT_RPC_PORT: u16 = 9527;
pub const LOCALHOST: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GasStationConfig {
    pub keypair: SuiKeyPair,
    pub local_db_path: PathBuf,
    pub rpc_host_ip: Ipv4Addr,
    pub rpc_port: u16,
    pub gas_pool_config: GasPoolStorageConfig,
    pub fullnode_url: String,
}

impl Default for GasStationConfig {
    fn default() -> Self {
        let (_, keypair) = get_account_key_pair();
        GasStationConfig {
            keypair: keypair.into(),
            local_db_path: tempfile::tempdir().unwrap().into_path(),
            rpc_host_ip: LOCALHOST,
            rpc_port: DEFAULT_RPC_PORT,
            gas_pool_config: GasPoolStorageConfig::default(),
            fullnode_url: "http://localhost:9000".to_string(),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GasPoolStorageConfig {
    LocalRocksDbForTesting { db_path: PathBuf },
    RemoteRocksDb { db_rpc_url: String },
}

impl Default for GasPoolStorageConfig {
    fn default() -> Self {
        GasPoolStorageConfig::LocalRocksDbForTesting {
            db_path: tempfile::tempdir().unwrap().into_path(),
        }
    }
}

impl Config for GasStationConfig {}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::Deserialize;
use std::{env, fs, path::Path};

/// config as loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub remote_store_url: String,
    pub eth_rpc_url: String,
    pub db_url: Option<String>,
    pub checkpoints_path: String,
    pub concurrency: u64,
    pub bridge_genesis_checkpoint: u64,
    pub eth_sui_bridge_contract_address: String,
    pub start_block: u64,
    pub metric_url: String,
    pub metric_port: u16,
}

/// Load the config to run.
pub fn load_config(path: &Path) -> Result<Config> {
    let reader = fs::File::open(path)?;
    let mut config: Config = serde_yaml::from_reader(reader)?;
    if let Ok(db_url) = env::var("DB_URL") {
        config.db_url = Some(db_url);
    } else {
        match config.db_url.as_ref() {
            Some(_) => (),
            None => panic!("db_url must be set in config or via the $DB_URL env var"),
        }
    }
    Ok(config.clone())
}

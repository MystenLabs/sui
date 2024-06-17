// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::Deserialize;
use std::{fs, path::Path};

/// config as loaded from `config.yaml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub remote_store_url: String,
    pub eth_rpc_url: String,
    pub db_url: String,
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
    let config: Config = serde_yaml::from_reader(reader)?;
    Ok(config)
}

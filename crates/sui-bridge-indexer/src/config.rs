// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::env;

/// config as loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexerConfig {
    pub remote_store_url: String,
    /// Only provide this if you use a colocated FN
    pub checkpoints_path: Option<String>,

    pub sui_rpc_url: String,
    pub eth_rpc_url: String,
    pub eth_ws_url: String,

    #[serde(default = "default_db_url")]
    pub db_url: String,
    pub concurrency: u64,
    /// Used as the starting checkpoint for backfill tasks when indexer starts with an empty DB.
    pub sui_bridge_genesis_checkpoint: u64,
    /// Used as the starting checkpoint for backfill tasks when indexer starts with an empty DB.
    pub eth_bridge_genesis_block: u64,
    pub eth_sui_bridge_contract_address: String,

    pub metric_port: u16,
}

impl sui_config::Config for IndexerConfig {}

pub fn default_db_url() -> String {
    env::var("DB_URL").expect("db_url must be set in config or via the $DB_URL env var")
}

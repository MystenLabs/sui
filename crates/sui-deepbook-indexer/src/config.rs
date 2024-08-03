// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::env;

/// config as loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexerConfig {
    pub remote_store_url: String,
    #[serde(default = "default_db_url")]
    pub db_url: String,
    pub checkpoints_path: String,
    pub concurrency: u64,
    pub metric_url: String,
    pub metric_port: u16,
    pub sui_rpc_url: Option<String>,
    pub start_checkpoint: Option<u64>,
    pub end_checkpoint: Option<u64>,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        IndexerConfig {
            remote_store_url: "".to_string(),
            db_url: default_db_url(),
            checkpoints_path: "".to_string(),
            concurrency: 1,
            metric_url: "".to_string(),
            metric_port: 0,
            sui_rpc_url: None,
            start_checkpoint: None,
            end_checkpoint: None,
        }
    }
}
impl sui_config::Config for IndexerConfig {}

pub fn default_db_url() -> String {
    env::var("DB_URL").expect("db_url must be set in config or via the $DB_URL env var")
}

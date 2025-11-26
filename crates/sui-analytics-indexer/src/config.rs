// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Configuration types for the analytics indexer.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;

use crate::pipeline::Pipeline;

fn default_client_metric_host() -> String {
    "127.0.0.1".to_string()
}

fn default_client_metric_port() -> u16 {
    8081
}

fn default_remote_store_url() -> String {
    "https://checkpoints.mainnet.sui.io".to_string()
}

fn default_max_row_count() -> usize {
    100000
}

fn default_file_format() -> FileFormat {
    FileFormat::Parquet
}

/// Output file format for analytics data.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Csv,
    Parquet,
}

/// Main configuration for an analytics indexer job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobConfig {
    /// The url of the checkpoint client to connect to.
    pub rest_url: String,
    /// The url of the metrics client to connect to.
    #[serde(default = "default_client_metric_host")]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[serde(default = "default_client_metric_port")]
    pub client_metric_port: u16,
    /// Remote object store where data gets written to
    pub remote_store_config: ObjectStoreConfig,
    /// Remote store URL.
    #[serde(default = "default_remote_store_url")]
    pub remote_store_url: String,
    /// Optional streaming URL for real-time indexing
    pub streaming_url: Option<String>,
    /// Optional RPC API URL for request/reply from full node
    pub rpc_api_url: Option<String>,
    /// Optional RPC username
    pub rpc_username: Option<String>,
    /// Optional RPC password
    pub rpc_password: Option<String>,
    /// Optional working directory for temporary files (defaults to system temp dir)
    pub work_dir: Option<PathBuf>,
    pub sf_account_identifier: Option<String>,
    pub sf_warehouse: Option<String>,
    pub sf_database: Option<String>,
    pub sf_schema: Option<String>,
    pub sf_username: Option<String>,
    pub sf_role: Option<String>,
    pub sf_password_file: Option<String>,

    // This is private to enforce using the PipelineConfig struct
    #[serde(rename = "pipelines")]
    pipeline_configs: Vec<PipelineConfig>,

    #[serde(default)]
    pub ingestion: IngestionConfig,

    #[serde(default)]
    pub concurrent: ConcurrentConfig,

    pub first_checkpoint: Option<u64>,
    pub last_checkpoint: Option<u64>,
}

impl JobConfig {
    /// Returns the pipeline configurations.
    pub fn pipeline_configs(&self) -> &[PipelineConfig] {
        &self.pipeline_configs
    }
}

/// Configuration for a single analytics pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Type of data to write i.e. checkpoint, object, transaction, etc
    pub pipeline: Pipeline,
    /// File format to use (csv or parquet)
    #[serde(default = "default_file_format")]
    pub file_format: FileFormat,
    /// Maximum number of rows before uploading to the datastore.
    #[serde(default = "default_max_row_count")]
    pub max_row_count: usize,
    pub package_id_filter: Option<String>,
    /// Snowflake table to monitor
    pub sf_table_id: Option<String>,
    /// Snowflake column containing checkpoint numbers
    pub sf_checkpoint_col_id: Option<String>,
    /// Whether to report max checkpoint from Snowflake table
    #[serde(default)]
    pub report_sf_max_table_checkpoint: bool,
}

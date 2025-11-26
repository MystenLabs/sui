// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Configuration types for the analytics indexer.

use std::path::PathBuf;

use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::schema::RowSchema;
use crate::writers::{CsvWriter, ParquetWriter};
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

fn default_reader_interval_ms() -> u64 {
    1000
}

fn default_file_format() -> FileFormat {
    FileFormat::Parquet
}

fn default_request_timeout_secs() -> u64 {
    30
}

/// Output file format for analytics data.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Csv,
    Parquet,
}

impl FileFormat {
    /// Serializes rows to the appropriate format and returns the bytes.
    pub fn serialize_rows<S: Serialize + RowSchema + Send + Sync + 'static>(
        &self,
        rows: Vec<S>,
    ) -> Result<Option<Bytes>> {
        match self {
            FileFormat::Csv => {
                let mut w = CsvWriter::new()?;
                w.write(Box::new(rows.into_iter()))?;
                Ok(w.flush::<S>()?.map(Bytes::from))
            }
            FileFormat::Parquet => {
                let mut w = ParquetWriter::new()?;
                w.write(Box::new(rows.into_iter()))?;
                Ok(w.flush::<S>()?.map(Bytes::from))
            }
        }
    }
}

/// Object store configuration for analytics output.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum OutputStoreConfig {
    Gcs {
        bucket: String,
        /// Path to service account JSON file
        service_account_path: PathBuf,
    },
    S3 {
        bucket: String,
        region: String,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        endpoint: Option<String>,
    },
    Azure {
        container: String,
        account: String,
        access_key: String,
    },
    File {
        path: PathBuf,
    },
}

/// Main configuration for an analytics indexer job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// The url of the checkpoint client to connect to.
    pub rest_url: String,
    /// The url of the metrics client to connect to.
    #[serde(default = "default_client_metric_host")]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[serde(default = "default_client_metric_port")]
    pub client_metric_port: u16,
    /// Output object store configuration
    pub output_store: OutputStoreConfig,
    /// Request timeout for object store operations
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
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

    pub task_name: Option<String>,

    /// Reader interval in milliseconds for tasked indexer.
    #[serde(default = "default_reader_interval_ms")]
    pub reader_interval_ms: u64,

    #[serde(rename = "pipelines")]
    pipeline_configs: Vec<PipelineConfig>,

    #[serde(default)]
    pub ingestion: IngestionConfig,

    #[serde(default)]
    pub concurrent: ConcurrentConfig,

    pub first_checkpoint: Option<u64>,
    pub last_checkpoint: Option<u64>,
}

impl IndexerConfig {
    pub fn pipeline_configs(&self) -> &[PipelineConfig] {
        &self.pipeline_configs
    }
}

/// Configuration for a single analytics task/pipeline.
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

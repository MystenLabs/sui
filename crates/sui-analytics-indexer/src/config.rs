// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Configuration types for the analytics indexer.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;

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

fn default_file_format() -> FileFormat {
    FileFormat::Parquet
}

fn default_request_timeout_secs() -> u64 {
    30
}

fn default_max_pending_uploads() -> usize {
    10
}

fn default_max_concurrent_serialization() -> usize {
    3
}

fn default_watermark_update_interval_secs() -> u64 {
    60
}

fn default_force_batch_cut_after_secs() -> u64 {
    600 // 10 minutes
}

/// Output file format for analytics data.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Csv,
    Parquet,
}

/// Object store configuration for analytics output.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum OutputStoreConfig {
    Gcs {
        bucket: String,
        /// Path to service account JSON file
        service_account_path: PathBuf,
        /// Custom HTTP headers to include in all requests (e.g., for requester-pays buckets)
        #[serde(default)]
        custom_headers: Option<HashMap<String, String>>,
        #[serde(default = "default_request_timeout_secs")]
        request_timeout_secs: u64,
    },
    S3 {
        bucket: String,
        region: String,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        endpoint: Option<String>,
        #[serde(default = "default_request_timeout_secs")]
        request_timeout_secs: u64,
    },
    Azure {
        container: String,
        account: String,
        access_key: String,
        #[serde(default = "default_request_timeout_secs")]
        request_timeout_secs: u64,
    },
    File {
        path: PathBuf,
    },
    /// Custom object store for testing. Allows sharing a store instance across runs.
    #[serde(skip)]
    Custom(std::sync::Arc<dyn object_store::ObjectStore>),
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
    /// Optional local ingestion path for reading checkpoints from disk instead of remote
    pub local_ingestion_path: Option<PathBuf>,
    pub sf_account_identifier: Option<String>,
    pub sf_warehouse: Option<String>,
    pub sf_database: Option<String>,
    pub sf_schema: Option<String>,
    pub sf_username: Option<String>,
    pub sf_role: Option<String>,
    pub sf_password_file: Option<String>,

    /// Migration mode identifier. When set, the indexer operates in migration mode:
    /// - Overwrites existing files matching target checkpoint ranges
    /// - Uses conditional PUT with etag to prevent concurrent modification
    /// - Uses per-file metadata to track migration progress separately from main pipeline
    #[serde(default)]
    pub migration_id: Option<String>,

    /// File format for output files (csv or parquet).
    #[serde(default = "default_file_format")]
    pub file_format: FileFormat,

    #[serde(rename = "pipelines")]
    pub pipeline_configs: Vec<PipelineConfig>,

    #[serde(default)]
    pub ingestion: IngestionConfig,

    #[serde(default)]
    pub sequential: SequentialConfig,

    pub first_checkpoint: Option<u64>,
    pub last_checkpoint: Option<u64>,

    /// Maximum serialized files waiting in upload queue per pipeline.
    /// When the queue is full, serialization blocks until uploads complete.
    #[serde(default = "default_max_pending_uploads")]
    pub max_pending_uploads: usize,

    /// Maximum concurrent serialization tasks per pipeline.
    /// Limits CPU usage from parallel parquet/csv encoding.
    #[serde(default = "default_max_concurrent_serialization")]
    pub max_concurrent_serialization: usize,

    /// Minimum interval between watermark writes to object store (seconds).
    /// Watermarks are updated after file uploads; this rate-limits those writes.
    /// Default: 60 seconds.
    #[serde(default = "default_watermark_update_interval_secs")]
    pub watermark_update_interval_secs: u64,
}

impl IndexerConfig {
    /// Validate the indexer configuration.
    ///
    /// Checks for:
    /// - Duplicate pipeline types (each pipeline can only be configured once)
    /// - Individual pipeline config validity (e.g., batch_size required in live mode)
    pub fn validate(&self) -> anyhow::Result<()> {
        // Check for duplicate pipeline types
        let mut seen = std::collections::HashSet::new();
        for config in &self.pipeline_configs {
            let name = config.pipeline.name();
            if !seen.insert(name) {
                anyhow::bail!(
                    "Duplicate pipeline type '{}' in config. Each pipeline type can only be configured once.",
                    name
                );
            }
        }

        // Validate individual pipeline configs
        let is_migration_mode = self.migration_id.is_some();
        for config in &self.pipeline_configs {
            config.validate(is_migration_mode)?;
        }

        Ok(())
    }

    pub fn pipeline_configs(&self) -> &[PipelineConfig] {
        &self.pipeline_configs
    }

    pub fn get_pipeline_config(&self, name: &str) -> Option<&PipelineConfig> {
        self.pipeline_configs
            .iter()
            .find(|p| p.pipeline.name() == name)
    }
}

/// Batch size configuration for when to write files.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchSizeConfig {
    /// Write a file after accumulating this many checkpoints.
    Checkpoints(usize),
    /// Write a file after accumulating this many rows.
    Rows(usize),
}

impl BatchSizeConfig {
    /// Validate the batch size configuration.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            BatchSizeConfig::Checkpoints(0) => {
                anyhow::bail!("batch_size.checkpoints must be > 0")
            }
            BatchSizeConfig::Rows(0) => {
                anyhow::bail!("batch_size.rows must be > 0")
            }
            _ => Ok(()),
        }
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
    pub package_id_filter: Option<String>,
    /// Snowflake table to monitor
    pub sf_table_id: Option<String>,
    /// Snowflake column containing checkpoint numbers
    pub sf_checkpoint_col_id: Option<String>,
    /// Whether to report max checkpoint from Snowflake table
    #[serde(default)]
    pub report_sf_max_table_checkpoint: bool,
    /// Batch size configuration - determines when to write files.
    /// Required for live mode (when top-level migration_id is None).
    /// Ignored in migration mode (file boundaries come from existing files).
    #[serde(default)]
    pub batch_size: Option<BatchSizeConfig>,
    /// Override the output path prefix. Defaults to the pipeline name.
    #[serde(default)]
    pub output_prefix: Option<String>,
    /// Force a batch cut after this many seconds, even if size thresholds aren't met.
    /// Default: 600 (10 minutes).
    #[serde(default = "default_force_batch_cut_after_secs")]
    pub force_batch_cut_after_secs: u64,
}

impl PipelineConfig {
    /// Validate the configuration.
    ///
    /// Returns an error if batch_size is required but not set, or if batch_size is invalid.
    /// In migration mode, batch_size is not required since file boundaries
    /// come from existing files.
    pub fn validate(&self, is_migration_mode: bool) -> anyhow::Result<()> {
        if !is_migration_mode {
            match &self.batch_size {
                None => anyhow::bail!(
                    "batch_size is required for pipeline '{}' (not in migration mode)",
                    self.pipeline
                ),
                Some(batch_size) => batch_size.validate()?,
            }
        }
        Ok(())
    }

    /// Get the output path prefix for this pipeline.
    ///
    /// Returns the configured `output_prefix` if set, otherwise the pipeline's default path.
    pub fn output_prefix(&self) -> &str {
        self.output_prefix
            .as_deref()
            .unwrap_or_else(|| self.pipeline.default_path())
    }
}

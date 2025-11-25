// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use package_store::PackageCache;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tracing::info;

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_types::base_types::EpochId;
use sui_types::dynamic_field::DynamicFieldType;

use crate::handlers::checkpoint_handler::{CheckpointHandler, CheckpointProcessor};
use crate::handlers::df_handler::{DynamicFieldHandler, DynamicFieldProcessor};
use crate::handlers::event_handler::{EventHandler, EventProcessor};
use crate::handlers::move_call_handler::{MoveCallHandler, MoveCallProcessor};
use crate::handlers::object_handler::{ObjectHandler, ObjectProcessor};
use crate::handlers::package_bcs_handler::{PackageBCSHandler, PackageBCSProcessor};
use crate::handlers::package_handler::{PackageHandler, PackageProcessor};
use crate::handlers::transaction_bcs_handler::{TransactionBCSHandler, TransactionBCSProcessor};
use crate::handlers::transaction_handler::{TransactionHandler, TransactionProcessor};
use crate::handlers::transaction_objects_handler::{
    TransactionObjectsHandler, TransactionObjectsProcessor,
};
use crate::handlers::wrapped_object_handler::{WrappedObjectHandler, WrappedObjectProcessor};
use crate::tables::{InputObjectKind, ObjectStatus, OwnerType};
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;

pub mod analytics_metrics;
pub mod csv;
pub mod errors;
mod handlers;
pub mod package_store;
pub mod parquet;
pub mod tables;

// Re-export handler traits and generic batch struct for public API
pub use handlers::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata};

fn default_client_metric_host() -> String {
    "127.0.0.1".to_string()
}

fn default_client_metric_port() -> u16 {
    8081
}

fn default_checkpoint_root() -> PathBuf {
    PathBuf::from("/tmp")
}

fn default_batch_size() -> usize {
    10
}

fn default_data_limit() -> usize {
    100
}

fn default_remote_store_url() -> String {
    "https://checkpoints.mainnet.sui.io".to_string()
}

fn default_remote_store_timeout_secs() -> u64 {
    5
}

fn default_package_cache_path() -> PathBuf {
    PathBuf::from("/opt/sui/db/package_cache")
}

fn default_checkpoint_interval() -> u64 {
    10000
}

fn default_max_file_size_mb() -> u64 {
    100
}

fn default_max_row_count() -> usize {
    100000
}

fn default_time_interval_s() -> u64 {
    600
}

fn default_file_format() -> FileFormat {
    FileFormat::Parquet
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Csv,
    Parquet,
}

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
    /// Object store download batch size.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Maximum number of checkpoints to queue in memory.
    #[serde(default = "default_data_limit")]
    pub data_limit: usize,
    /// Remote store URL.
    #[serde(default = "default_remote_store_url")]
    pub remote_store_url: String,
    /// These are key-value config pairs that are defined in the object_store crate
    /// <https://docs.rs/object_store/latest/object_store/gcp/enum.GoogleConfigKey.html>
    #[serde(default)]
    pub remote_store_options: Vec<(String, String)>,
    /// Remote store timeout
    #[serde(default = "default_remote_store_timeout_secs")]
    pub remote_store_timeout_secs: u64,
    /// Directory to contain the package cache for pipelines
    #[serde(default = "default_package_cache_path")]
    pub package_cache_path: PathBuf,
    /// Root directory to contain the temporary directory for checkpoint entries.
    #[serde(default = "default_checkpoint_root")]
    pub checkpoint_root: PathBuf,
    pub bq_service_account_key_file: Option<String>,
    pub bq_project_id: Option<String>,
    pub bq_dataset_id: Option<String>,
    pub sf_account_identifier: Option<String>,
    pub sf_warehouse: Option<String>,
    pub sf_database: Option<String>,
    pub sf_schema: Option<String>,
    pub sf_username: Option<String>,
    pub sf_role: Option<String>,
    pub sf_password_file: Option<String>,

    // This is private to enforce using the PipelineConfig struct
    #[serde(rename = "tasks")]
    pipeline_configs: Vec<PipelineConfig>,
}

impl JobConfig {
    // Convenience method to get pipeline configs
    pub fn pipeline_configs(&self) -> &[PipelineConfig] {
        &self.pipeline_configs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Name of the pipeline. Must be unique per process. Used to identify pipelines in the Progress Store.
    pub pipeline_name: String,
    /// Type of data to write i.e. checkpoint, object, transaction, etc
    pub pipeline: Pipeline,
    /// File format to use (csv or parquet)
    #[serde(default = "default_file_format")]
    pub file_format: FileFormat,
    /// Number of checkpoints to process before uploading to the datastore.
    #[serde(default = "default_checkpoint_interval")]
    pub checkpoint_interval: u64,
    /// Maximum file size in mb before uploading to the datastore.
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,
    /// Maximum number of rows before uploading to the datastore.
    #[serde(default = "default_max_row_count")]
    pub max_row_count: usize,
    /// Checkpoint sequence number to start the download from
    pub starting_checkpoint_seq_num: Option<u64>,
    /// Time to process in seconds before uploding to the datastore.
    #[serde(default = "default_time_interval_s")]
    pub time_interval_s: u64,
    /// Remote object store path prefix to use while writing
    #[serde(default)]
    remote_store_path_prefix: Option<String>,
    pub bq_table_id: Option<String>,
    pub bq_checkpoint_col_id: Option<String>,
    #[serde(default)]
    pub report_bq_max_table_checkpoint: bool,
    pub sf_table_id: Option<String>,
    pub sf_checkpoint_col_id: Option<String>,
    #[serde(default)]
    pub report_sf_max_table_checkpoint: bool,
    pub package_id_filter: Option<String>,
    /// Enable backfill mode to match existing checkpoint ranges
    #[serde(default)]
    pub backfill_mode: bool,
    /// Epoch to backfill (required when backfill_mode is true)
    pub starting_epoch: Option<u64>,
}

impl PipelineConfig {
    pub fn remote_store_path_prefix(&self) -> Result<Option<Path>> {
        self.remote_store_path_prefix
            .as_ref()
            .map(|pb| Ok(Path::from(pb.as_str())))
            .transpose()
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    PartialEq,
    strum_macros::Display,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    EnumIter,
)]
#[repr(u8)]
pub enum Pipeline {
    Checkpoint = 0,
    Object,
    Transaction,
    TransactionBCS,
    TransactionObjects,
    Event,
    MoveCall,
    MovePackage,
    MovePackageBCS,
    DynamicField,
    WrappedObject,
}

/// Construct a relative file path from directory prefix and metadata
pub(crate) fn construct_file_path(
    dir_prefix: &str,
    epoch_num: EpochId,
    checkpoint_range: Range<u64>,
    file_format: FileFormat,
) -> PathBuf {
    let extension = match file_format {
        FileFormat::Csv => "csv",
        FileFormat::Parquet => "parquet",
    };
    PathBuf::from(dir_prefix)
        .join(format!("epoch_{}", epoch_num))
        .join(format!(
            "{}_{}.{}",
            checkpoint_range.start, checkpoint_range.end, extension
        ))
}

/// Parse checkpoint range from a file path
/// Example: "events/epoch_42/1000_2437.parquet" -> Some(1000..2437)
pub fn parse_checkpoint_range_from_path(path: &str) -> Option<Range<u64>> {
    let path = std::path::Path::new(path);
    let filename = path.file_stem()?.to_str()?;

    // Parse "1000_2437" -> 1000..2437
    let parts: Vec<&str> = filename.split('_').collect();
    if parts.len() >= 2 {
        let start: u64 = parts[parts.len() - 2].parse().ok()?;
        let end: u64 = parts[parts.len() - 1].parse().ok()?;
        Some(start..end)
    } else {
        None
    }
}

/// Discover existing checkpoint ranges from object store
/// Returns sorted list of (checkpoint_range, file_path) tuples
/// Fails if ranges have gaps (not contiguous)
pub async fn discover_checkpoint_ranges(
    object_store: &dyn object_store::ObjectStore,
    pipeline: Pipeline,
    epoch: u64,
) -> Result<Vec<(Range<u64>, String)>> {
    let prefix = format!("{}/epoch_{}/", pipeline.dir_prefix().as_ref(), epoch);
    let prefix_path = object_store::path::Path::from(prefix.as_str());

    let mut ranges = Vec::new();

    use futures::TryStreamExt;
    let list_stream = object_store.list(Some(&prefix_path));
    let objects: Vec<_> = list_stream.try_collect().await?;

    for meta in objects {
        let path = meta.location.to_string();

        if let Some(range) = parse_checkpoint_range_from_path(&path) {
            ranges.push((range, path));
        }
    }

    if ranges.is_empty() {
        anyhow::bail!(
            "No existing files found for {}/epoch_{}",
            pipeline.dir_prefix().as_ref(),
            epoch
        );
    }

    // Sort by checkpoint start
    ranges.sort_by_key(|(range, _)| range.start);

    // Validate ranges are contiguous (fail fast if gaps detected)
    for i in 0..ranges.len().saturating_sub(1) {
        let current_end = ranges[i].0.end;
        let next_start = ranges[i + 1].0.start;

        if current_end != next_start {
            anyhow::bail!(
                "Gap detected in checkpoint ranges: range {} ends at {}, but range {} starts at {}",
                i,
                current_end,
                i + 1,
                next_start
            );
        }
    }

    Ok(ranges)
}

impl Pipeline {
    pub(crate) fn dir_prefix(&self) -> Path {
        match self {
            Pipeline::Checkpoint => Path::from("checkpoints"),
            Pipeline::Transaction => Path::from("transactions"),
            Pipeline::TransactionBCS => Path::from("transaction_bcs"),
            Pipeline::TransactionObjects => Path::from("transaction_objects"),
            Pipeline::Object => Path::from("objects"),
            Pipeline::Event => Path::from("events"),
            Pipeline::MoveCall => Path::from("move_call"),
            Pipeline::MovePackage => Path::from("move_package"),
            Pipeline::MovePackageBCS => Path::from("move_package_bcs"),
            Pipeline::DynamicField => Path::from("dynamic_field"),
            Pipeline::WrappedObject => Path::from("wrapped_object"),
        }
    }

    pub async fn register_handler(
        &self,
        indexer: &mut Indexer<ObjectStore>,
        pipeline_config: &PipelineConfig,
        package_cache: Option<Arc<PackageCache>>,
        config: ConcurrentConfig,
        object_store: Arc<dyn object_store::ObjectStore>,
    ) -> Result<()> {
        match self {
            Pipeline::Checkpoint => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for Checkpoint pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    CheckpointHandler::new_backfill(
                        CheckpointProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    CheckpointHandler::new(CheckpointProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::Transaction => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for Transaction pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    TransactionHandler::new_backfill(
                        TransactionProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    TransactionHandler::new(TransactionProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::TransactionBCS => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for TransactionBCS pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    TransactionBCSHandler::new_backfill(
                        TransactionBCSProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    TransactionBCSHandler::new(TransactionBCSProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::Event => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for Event handler"))?;
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for Event pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    EventHandler::new_backfill(
                        EventProcessor::new(cache),
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    EventHandler::new(EventProcessor::new(cache), pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::MoveCall => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for MoveCall pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    MoveCallHandler::new_backfill(
                        MoveCallProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    MoveCallHandler::new(MoveCallProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::Object => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for Object handler"))?;
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for Object pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    ObjectHandler::new_backfill(
                        ObjectProcessor::new(cache, &pipeline_config.package_id_filter),
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    ObjectHandler::new(
                        ObjectProcessor::new(cache, &pipeline_config.package_id_filter),
                        pipeline_config.clone(),
                    )
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::DynamicField => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for DynamicField handler"))?;
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for DynamicField pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    DynamicFieldHandler::new_backfill(
                        DynamicFieldProcessor::new(cache),
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    DynamicFieldHandler::new(
                        DynamicFieldProcessor::new(cache),
                        pipeline_config.clone(),
                    )
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::TransactionObjects => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for TransactionObjects pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    TransactionObjectsHandler::new_backfill(
                        TransactionObjectsProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    TransactionObjectsHandler::new(
                        TransactionObjectsProcessor,
                        pipeline_config.clone(),
                    )
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::MovePackage => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for MovePackage pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    PackageHandler::new_backfill(
                        PackageProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    PackageHandler::new(PackageProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::MovePackageBCS => {
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for MovePackageBCS pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    PackageBCSHandler::new_backfill(
                        PackageBCSProcessor,
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    PackageBCSHandler::new(PackageBCSProcessor, pipeline_config.clone())
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
            Pipeline::WrappedObject => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for WrappedObject handler"))?;
                let handler = if pipeline_config.backfill_mode {
                    let epoch = pipeline_config.starting_epoch.ok_or_else(|| {
                        anyhow!("starting_epoch required when backfill_mode is true")
                    })?;
                    let target_ranges =
                        discover_checkpoint_ranges(object_store.as_ref(), *self, epoch).await?;
                    info!(
                        "Backfill mode enabled for WrappedObject pipeline, discovered {} ranges for epoch {}",
                        target_ranges.len(),
                        epoch
                    );
                    WrappedObjectHandler::new_backfill(
                        WrappedObjectProcessor::new(cache),
                        pipeline_config.clone(),
                        target_ranges,
                    )
                } else {
                    WrappedObjectHandler::new(
                        WrappedObjectProcessor::new(cache),
                        pipeline_config.clone(),
                    )
                };
                indexer.concurrent_pipeline(handler, config).await?;
            }
        }
        Ok(())
    }
}

pub enum ParquetValue {
    U64(u64),
    Str(String),
    Bool(bool),
    I64(i64),
    OptionU64(Option<u64>),
    OptionStr(Option<String>),
}

impl From<u64> for ParquetValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<i64> for ParquetValue {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl From<String> for ParquetValue {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<Option<u64>> for ParquetValue {
    fn from(value: Option<u64>) -> Self {
        Self::OptionU64(value)
    }
}

impl From<Option<String>> for ParquetValue {
    fn from(value: Option<String>) -> Self {
        Self::OptionStr(value)
    }
}

impl From<bool> for ParquetValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<OwnerType> for ParquetValue {
    fn from(value: OwnerType) -> Self {
        Self::Str(value.to_string())
    }
}

impl From<Option<OwnerType>> for ParquetValue {
    fn from(value: Option<OwnerType>) -> Self {
        value.map(|v| v.to_string()).into()
    }
}

impl From<ObjectStatus> for ParquetValue {
    fn from(value: ObjectStatus) -> Self {
        Self::Str(value.to_string())
    }
}

impl From<Option<ObjectStatus>> for ParquetValue {
    fn from(value: Option<ObjectStatus>) -> Self {
        Self::OptionStr(value.map(|v| v.to_string()))
    }
}

impl From<Option<InputObjectKind>> for ParquetValue {
    fn from(value: Option<InputObjectKind>) -> Self {
        Self::OptionStr(value.map(|v| v.to_string()))
    }
}

impl From<DynamicFieldType> for ParquetValue {
    fn from(value: DynamicFieldType) -> Self {
        Self::Str(value.to_string())
    }
}

impl From<Option<DynamicFieldType>> for ParquetValue {
    fn from(value: Option<DynamicFieldType>) -> Self {
        Self::OptionStr(value.map(|v| v.to_string()))
    }
}

pub trait ParquetSchema {
    fn schema() -> Vec<String>;

    fn get_column(&self, idx: usize) -> ParquetValue;
}

// New framework-based indexer implementation
pub mod indexer_alt {
    use super::*;
    use anyhow::Context;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_indexer_alt_framework::pipeline::CommitterConfig;
    use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
    use sui_indexer_alt_framework::{Indexer, ingestion::IngestionConfig};
    use sui_indexer_alt_object_store::ObjectStore;
    use tokio_util::sync::CancellationToken;

    pub struct AnalyticsIndexerConfig {
        pub job_config: JobConfig,
        pub write_concurrency: usize,
        pub watermark_interval: Duration,
        pub first_checkpoint: Option<u64>,
        pub last_checkpoint: Option<u64>,
    }

    impl Default for AnalyticsIndexerConfig {
        fn default() -> Self {
            Self {
                job_config: JobConfig {
                    rest_url: "https://checkpoints.mainnet.sui.io".to_string(),
                    client_metric_host: default_client_metric_host(),
                    client_metric_port: default_client_metric_port(),
                    remote_store_config: ObjectStoreConfig::default(),
                    batch_size: default_batch_size(),
                    data_limit: default_data_limit(),
                    remote_store_url: default_remote_store_url(),
                    remote_store_options: vec![],
                    remote_store_timeout_secs: default_remote_store_timeout_secs(),
                    package_cache_path: default_package_cache_path(),
                    checkpoint_root: default_checkpoint_root(),
                    bq_service_account_key_file: None,
                    bq_project_id: None,
                    bq_dataset_id: None,
                    sf_account_identifier: None,
                    sf_warehouse: None,
                    sf_database: None,
                    sf_schema: None,
                    sf_username: None,
                    sf_role: None,
                    sf_password_file: None,
                    pipeline_configs: vec![],
                },
                write_concurrency: 10,
                watermark_interval: Duration::from_secs(60),
                first_checkpoint: None,
                last_checkpoint: None,
            }
        }
    }

    pub async fn start_analytics_indexer(
        config: AnalyticsIndexerConfig,
        registry: prometheus::Registry,
        cancel: CancellationToken,
    ) -> Result<tokio::task::JoinHandle<()>> {
        info!("Starting analytics indexer with framework");
        info!("Job config: {:#?}", config.job_config);

        // Setup object store from remote_store_config
        let object_store = create_object_store_from_config(
            &config.job_config.remote_store_config,
            config.job_config.remote_store_timeout_secs,
        )
        .await?;

        let store = ObjectStore::new(object_store.clone());

        // Create package cache for handlers that need it
        let package_cache = Arc::new(PackageCache::new(
            &config.job_config.package_cache_path,
            &config.job_config.rest_url,
        ));

        // Create the indexer args from config
        let indexer_args = sui_indexer_alt_framework::IndexerArgs {
            first_checkpoint: config.first_checkpoint,
            last_checkpoint: config.last_checkpoint,
            pipeline: vec![],
            task: Default::default(),
        };

        let client_args = sui_indexer_alt_framework::ingestion::ClientArgs {
            ingestion:
                sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs {
                    remote_store_url: Some(url::Url::parse(&config.job_config.remote_store_url)?),
                    local_ingestion_path: None,
                    rpc_api_url: None,
                    rpc_username: None,
                    rpc_password: None,
                },
            streaming:
                sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs {
                    streaming_url: None,
                },
        };

        let ingestion_config = IngestionConfig {
            checkpoint_buffer_size: config.job_config.data_limit,
            ingest_concurrency: config.job_config.batch_size,
            retry_interval_ms: 5000,
            streaming_backoff_initial_batch_size: 10,
            streaming_backoff_max_batch_size: 10000,
        };

        let concurrent_config = ConcurrentConfig {
            committer: CommitterConfig {
                write_concurrency: config.write_concurrency,
                watermark_interval_ms: config.watermark_interval.as_millis() as u64,
                ..Default::default()
            },
            pruner: None,
        };

        let mut indexer = Indexer::new(
            store.clone(),
            indexer_args,
            client_args,
            ingestion_config,
            None,
            &registry,
            cancel.clone(),
        )
        .await?;

        // Register pipelines for each enabled file type
        for pipeline_config in &config.job_config.pipeline_configs {
            info!(
                "Registering pipeline: {} with file type: {:?}",
                pipeline_config.pipeline_name, pipeline_config.pipeline
            );

            register_pipeline(
                &mut indexer,
                pipeline_config,
                Some(package_cache.clone()),
                concurrent_config.clone(),
                object_store.clone(),
            )
            .await?;
        }

        // Start the indexer
        let handle = indexer.run().await?;

        Ok(handle)
    }

    async fn register_pipeline(
        indexer: &mut Indexer<ObjectStore>,
        pipeline_config: &PipelineConfig,
        package_cache: Option<Arc<PackageCache>>,
        config: ConcurrentConfig,
        object_store: Arc<dyn object_store::ObjectStore>,
    ) -> Result<()> {
        pipeline_config
            .pipeline
            .register_handler(
                indexer,
                pipeline_config,
                package_cache,
                config,
                object_store,
            )
            .await
    }

    async fn create_object_store_from_config(
        config: &ObjectStoreConfig,
        _timeout_secs: u64,
    ) -> Result<Arc<dyn object_store::ObjectStore>> {
        let store = config
            .make()
            .context("Failed to create object store from configuration")?;
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_checkpoint_range_from_path_parquet() {
        let path = "events/epoch_42/1000_2437.parquet";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, Some(1000..2437));
    }

    #[test]
    fn test_parse_checkpoint_range_from_path_csv() {
        let path = "checkpoints/epoch_100/5000_10000.csv";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, Some(5000..10000));
    }

    #[test]
    fn test_parse_checkpoint_range_from_path_no_extension() {
        let path = "events/epoch_42/1000_2437";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, Some(1000..2437));
    }

    #[test]
    fn test_parse_checkpoint_range_from_path_invalid() {
        let path = "events/epoch_42/invalid.parquet";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, None);
    }

    #[test]
    fn test_parse_checkpoint_range_from_path_single_number() {
        let path = "events/epoch_42/1000.parquet";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, None);
    }

    #[test]
    fn test_parse_checkpoint_range_from_path_with_underscores() {
        let path = "move_package/epoch_5/100_200.parquet";
        let range = parse_checkpoint_range_from_path(path);
        assert_eq!(range, Some(100..200));
    }
}

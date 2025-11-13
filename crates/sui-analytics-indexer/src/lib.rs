// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use once_cell::sync::Lazy;
use package_store::PackageCache;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tracing::info;

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_types::base_types::EpochId;
use sui_types::dynamic_field::DynamicFieldType;

use crate::tables::{InputObjectKind, ObjectStatus, OwnerType};

pub mod analytics_metrics;
pub mod errors;
mod handlers;
pub mod package_store;
pub mod parquet;
pub mod tables;

// Re-export handler traits and generic batch struct for public API
pub use handlers::{AnalyticsBatch, AnalyticsMetadata};

use async_trait::async_trait;
use std::marker::PhantomData;
use std::sync::Arc;

/// Generic wrapper that implements Handler for any Processor with analytics batching
pub struct AnalyticsHandler<P, B> {
    processor: P,
    config: PipelineConfig,
    _batch: PhantomData<B>,
}

impl<P, B> AnalyticsHandler<P, B> {
    pub fn new(processor: P, config: PipelineConfig) -> Self {
        Self {
            processor,
            config,
            _batch: PhantomData,
        }
    }
}

// Implement Processor by delegating to inner processor
#[async_trait]
impl<P, B> sui_indexer_alt_framework::pipeline::Processor for AnalyticsHandler<P, B>
where
    P: sui_indexer_alt_framework::pipeline::Processor + Send + Sync,
    P::Value: Send + Sync,
    B: Send + Sync + 'static,
{
    const NAME: &'static str = P::NAME;
    const FANOUT: usize = P::FANOUT;
    type Value = P::Value;

    async fn process(
        &self,
        checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
    ) -> Result<Vec<Self::Value>> {
        self.processor.process(checkpoint).await
    }
}

// Implement Handler with shared batching logic
#[async_trait]
impl<P> sui_indexer_alt_framework::pipeline::concurrent::Handler
    for AnalyticsHandler<P, AnalyticsBatch<P::Value>>
where
    P: sui_indexer_alt_framework::pipeline::Processor + Send + Sync,
    P::Value: AnalyticsMetadata + Serialize + ParquetSchema + Send + Sync,
{
    type Store = sui_indexer_alt_object_store::ObjectStore;
    type Batch = AnalyticsBatch<P::Value>;

    fn min_eager_rows(&self) -> usize {
        self.config.max_row_count
    }

    fn max_pending_rows(&self) -> usize {
        self.config.max_row_count * 5
    }

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> sui_indexer_alt_framework::pipeline::concurrent::BatchStatus {
        let Some(first) = values.next() else {
            return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
        };

        let epoch = first.get_epoch();
        let checkpoint = first.get_checkpoint_sequence_number();

        batch.inner.set_epoch(epoch);
        batch.inner.update_last_checkpoint(checkpoint);

        if let Err(e) = batch
            .inner
            .write_rows(std::iter::once(first).chain(values.by_ref()))
        {
            tracing::error!("Failed to write rows to ParquetBatch: {}", e);
            return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
        }

        sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        let Some(file_path) = batch.inner.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.inner.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.inner.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}

const EPOCH_DIR_PREFIX: &str = "epoch_";
const CHECKPOINT_DIR_PREFIX: &str = "checkpoints";
const OBJECT_DIR_PREFIX: &str = "objects";
const TRANSACTION_DIR_PREFIX: &str = "transactions";
const TRANSACTION_BCS_DIR_PREFIX: &str = "transaction_bcs";
const EVENT_DIR_PREFIX: &str = "events";
const TRANSACTION_OBJECT_DIR_PREFIX: &str = "transaction_objects";
const MOVE_CALL_PREFIX: &str = "move_call";
const MOVE_PACKAGE_PREFIX: &str = "move_package";
const PACKAGE_BCS_DIR_PREFIX: &str = "move_package_bcs";
const DYNAMIC_FIELD_PREFIX: &str = "dynamic_field";

const WRAPPED_OBJECT_PREFIX: &str = "wrapped_object";

const TRANSACTION_CONCURRENCY_LIMIT_VAR_NAME: &str = "TRANSACTION_CONCURRENCY_LIMIT";
const DEFAULT_TRANSACTION_CONCURRENCY_LIMIT: usize = 64;
pub static TRANSACTION_CONCURRENCY_LIMIT: Lazy<usize> = Lazy::new(|| {
    let async_transactions_opt = std::env::var(TRANSACTION_CONCURRENCY_LIMIT_VAR_NAME)
        .ok()
        .and_then(|s| s.parse().ok());
    if let Some(async_transactions) = async_transactions_opt {
        info!(
            "Using custom value for '{}' max checkpoints in progress: {}",
            TRANSACTION_CONCURRENCY_LIMIT_VAR_NAME, async_transactions
        );
        async_transactions
    } else {
        info!(
            "Using default value for '{}' -- max checkpoints in progress: {}",
            TRANSACTION_CONCURRENCY_LIMIT_VAR_NAME, DEFAULT_TRANSACTION_CONCURRENCY_LIMIT
        );
        DEFAULT_TRANSACTION_CONCURRENCY_LIMIT
    }
});

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

fn default_file_format() -> FileFormat {
    FileFormat::CSV
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
    pub file_type: FileType,
    /// File format to store data in i.e. csv, parquet, etc
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
pub enum FileFormat {
    CSV = 0,
    PARQUET = 1,
}

impl FileFormat {
    pub fn file_suffix(&self) -> &str {
        match self {
            FileFormat::CSV => "csv",
            FileFormat::PARQUET => "parquet",
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    EnumIter,
)]
#[repr(u8)]
pub enum FileType {
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

impl FileType {
    pub fn dir_prefix(&self) -> Path {
        match self {
            FileType::Checkpoint => Path::from(CHECKPOINT_DIR_PREFIX),
            FileType::Transaction => Path::from(TRANSACTION_DIR_PREFIX),
            FileType::TransactionBCS => Path::from(TRANSACTION_BCS_DIR_PREFIX),
            FileType::TransactionObjects => Path::from(TRANSACTION_OBJECT_DIR_PREFIX),
            FileType::Object => Path::from(OBJECT_DIR_PREFIX),
            FileType::Event => Path::from(EVENT_DIR_PREFIX),
            FileType::MoveCall => Path::from(MOVE_CALL_PREFIX),
            FileType::MovePackage => Path::from(MOVE_PACKAGE_PREFIX),
            FileType::MovePackageBCS => Path::from(PACKAGE_BCS_DIR_PREFIX),
            FileType::DynamicField => Path::from(DYNAMIC_FIELD_PREFIX),
            FileType::WrappedObject => Path::from(WRAPPED_OBJECT_PREFIX),
        }
    }

    pub fn file_path(
        &self,
        file_format: FileFormat,
        epoch_num: EpochId,
        checkpoint_range: Range<u64>,
    ) -> Path {
        self.dir_prefix()
            .child(format!("{}{}", EPOCH_DIR_PREFIX, epoch_num))
            .child(format!(
                "{}_{}.{}",
                checkpoint_range.start,
                checkpoint_range.end,
                file_format.file_suffix()
            ))
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FileMetadata {
    pub file_type: FileType,
    pub file_format: FileFormat,
    pub epoch_num: u64,
    pub checkpoint_seq_range: Range<u64>,
}

impl FileMetadata {
    pub fn file_path(&self) -> Path {
        self.file_type.file_path(
            self.file_format,
            self.epoch_num,
            self.checkpoint_seq_range.clone(),
        )
    }
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

    // Import all the handlers and processors
    use crate::handlers::checkpoint_handler::{CheckpointHandler, CheckpointProcessor};
    use crate::handlers::df_handler::{DynamicFieldHandler, DynamicFieldProcessor};
    use crate::handlers::event_handler::{EventHandler, EventProcessor};
    use crate::handlers::move_call_handler::{MoveCallHandler, MoveCallProcessor};
    use crate::handlers::object_handler::{ObjectHandler, ObjectProcessor};
    use crate::handlers::package_bcs_handler::{PackageBCSHandler, PackageBCSProcessor};
    use crate::handlers::package_handler::{PackageHandler, PackageProcessor};
    use crate::handlers::transaction_bcs_handler::{
        TransactionBCSHandler, TransactionBCSProcessor,
    };
    use crate::handlers::transaction_handler::{TransactionHandler, TransactionProcessor};
    use crate::handlers::transaction_objects_handler::{
        TransactionObjectsHandler, TransactionObjectsProcessor,
    };
    use crate::handlers::wrapped_object_handler::{WrappedObjectHandler, WrappedObjectProcessor};

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

        let store = ObjectStore::new(object_store, None);

        // Create package cache for handlers that need it
        let package_cache = Arc::new(PackageCache::new(
            &config.job_config.package_cache_path,
            &config.job_config.rest_url,
        ));

        // Create the indexer args from config
        let indexer_args = sui_indexer_alt_framework::IndexerArgs {
            first_checkpoint: config.first_checkpoint,
            last_checkpoint: config.last_checkpoint,
            skip_watermark: false,
            pipeline: vec![],
        };

        let client_args = sui_indexer_alt_framework::ingestion::ClientArgs {
            remote_store_url: Some(url::Url::parse(&config.job_config.remote_store_url)?),
            local_ingestion_path: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
        };

        let ingestion_config = IngestionConfig {
            checkpoint_buffer_size: config.job_config.data_limit,
            ingest_concurrency: config.job_config.batch_size,
            retry_interval_ms: 5000,
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
                pipeline_config.pipeline_name, pipeline_config.file_type
            );

            register_pipeline(
                &mut indexer,
                pipeline_config,
                Some(package_cache.clone()),
                concurrent_config.clone(),
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
    ) -> Result<()> {
        match pipeline_config.file_type {
            FileType::Checkpoint => {
                indexer
                    .concurrent_pipeline(
                        CheckpointHandler::new(CheckpointProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::Transaction => {
                indexer
                    .concurrent_pipeline(
                        TransactionHandler::new(TransactionProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::TransactionBCS => {
                indexer
                    .concurrent_pipeline(
                        TransactionBCSHandler::new(
                            TransactionBCSProcessor,
                            pipeline_config.clone(),
                        ),
                        config,
                    )
                    .await?;
            }
            FileType::Event => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for Event handler"))?;
                indexer
                    .concurrent_pipeline(
                        EventHandler::new(EventProcessor::new(cache), pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::MoveCall => {
                indexer
                    .concurrent_pipeline(
                        MoveCallHandler::new(MoveCallProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::Object => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for Object handler"))?;
                indexer
                    .concurrent_pipeline(
                        ObjectHandler::new(
                            ObjectProcessor::new(cache, &pipeline_config.package_id_filter),
                            pipeline_config.clone(),
                        ),
                        config,
                    )
                    .await?;
            }
            FileType::DynamicField => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for DynamicField handler"))?;
                indexer
                    .concurrent_pipeline(
                        DynamicFieldHandler::new(
                            DynamicFieldProcessor::new(cache),
                            pipeline_config.clone(),
                        ),
                        config,
                    )
                    .await?;
            }
            FileType::TransactionObjects => {
                indexer
                    .concurrent_pipeline(
                        TransactionObjectsHandler::new(
                            TransactionObjectsProcessor,
                            pipeline_config.clone(),
                        ),
                        config,
                    )
                    .await?;
            }
            FileType::MovePackage => {
                indexer
                    .concurrent_pipeline(
                        PackageHandler::new(PackageProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::MovePackageBCS => {
                indexer
                    .concurrent_pipeline(
                        PackageBCSHandler::new(PackageBCSProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            FileType::WrappedObject => {
                let cache = package_cache
                    .clone()
                    .ok_or_else(|| anyhow!("Package cache required for WrappedObject handler"))?;
                indexer
                    .concurrent_pipeline(
                        WrappedObjectHandler::new(
                            WrappedObjectProcessor::new(cache),
                            pipeline_config.clone(),
                        ),
                        config,
                    )
                    .await?;
            }
        }
        Ok(())
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

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
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;
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

pub mod analytics_metrics;
pub mod csv;
pub mod errors;
mod handlers;
pub mod package_store;
pub mod parquet;
pub mod tables;

pub use handlers::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata};

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
    // Convenience method to get pipeline configs
    pub fn pipeline_configs(&self) -> &[PipelineConfig] {
        &self.pipeline_configs
    }
}

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
    ) -> Result<()> {
        match self {
            Pipeline::Checkpoint => {
                indexer
                    .concurrent_pipeline(
                        CheckpointHandler::new(CheckpointProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            Pipeline::Transaction => {
                indexer
                    .concurrent_pipeline(
                        TransactionHandler::new(TransactionProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            Pipeline::TransactionBCS => {
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
            Pipeline::Event => {
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
            Pipeline::MoveCall => {
                indexer
                    .concurrent_pipeline(
                        MoveCallHandler::new(MoveCallProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            Pipeline::Object => {
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
            Pipeline::DynamicField => {
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
            Pipeline::TransactionObjects => {
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
            Pipeline::MovePackage => {
                indexer
                    .concurrent_pipeline(
                        PackageHandler::new(PackageProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            Pipeline::MovePackageBCS => {
                indexer
                    .concurrent_pipeline(
                        PackageBCSHandler::new(PackageBCSProcessor, pipeline_config.clone()),
                        config,
                    )
                    .await?;
            }
            Pipeline::WrappedObject => {
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

#[async_trait::async_trait]
pub trait MaxCheckpointReader: Send + Sync + 'static {
    async fn max_checkpoint(&self) -> Result<i64>;
}

struct SnowflakeMaxCheckpointReader {
    query: String,
    api: snowflake_api::SnowflakeApi,
}

impl SnowflakeMaxCheckpointReader {
    pub async fn new(
        account_identifier: &str,
        warehouse: &str,
        database: &str,
        schema: &str,
        user: &str,
        role: &str,
        passwd: &str,
        table_id: &str,
        col_id: &str,
    ) -> Result<Self> {
        let api = snowflake_api::SnowflakeApi::with_password_auth(
            account_identifier,
            Some(warehouse),
            Some(database),
            Some(schema),
            user,
            Some(role),
            passwd,
        )
        .expect("Failed to build sf api client");
        Ok(SnowflakeMaxCheckpointReader {
            query: format!("SELECT max({}) from {}", col_id, table_id),
            api,
        })
    }
}

#[async_trait::async_trait]
impl MaxCheckpointReader for SnowflakeMaxCheckpointReader {
    async fn max_checkpoint(&self) -> Result<i64> {
        use arrow::array::Int32Array;
        use snowflake_api::QueryResult;

        let res = self.api.exec(&self.query).await?;
        match res {
            QueryResult::Arrow(a) => {
                if let Some(record_batch) = a.first() {
                    let col = record_batch.column(0);
                    let col_array = col
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .expect("Failed to downcast arrow column");
                    Ok(col_array.value(0) as i64)
                } else {
                    Ok(-1)
                }
            }
            QueryResult::Json(_j) => Err(anyhow!("Unexpected query result")),
            QueryResult::Empty => Err(anyhow!("Unexpected query result")),
        }
    }
}

pub async fn build_analytics_indexer(
    config: JobConfig,
    registry: prometheus::Registry,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Indexer<sui_indexer_alt_object_store::ObjectStore>> {
    let object_store = create_object_store_from_config(&config.remote_store_config).await?;

    let store = ObjectStore::new(object_store.clone());

    let work_dir = if let Some(ref work_dir) = config.work_dir {
        tempfile::Builder::new()
            .prefix("sui-analytics-indexer-")
            .tempdir_in(work_dir)?
            .keep()
    } else {
        tempfile::Builder::new()
            .prefix("sui-analytics-indexer-")
            .tempdir()?
            .keep()
    };

    // Create package cache for handlers that need it
    let package_cache_path = work_dir.join("package_cache");
    let package_cache = Arc::new(PackageCache::new(&package_cache_path, &config.rest_url));

    // Create the indexer args from config
    let indexer_args = sui_indexer_alt_framework::IndexerArgs {
        first_checkpoint: config.first_checkpoint,
        last_checkpoint: config.last_checkpoint,
        pipeline: vec![],
        task: Default::default(),
    };

    let client_args = sui_indexer_alt_framework::ingestion::ClientArgs {
        ingestion: sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs {
            remote_store_url: Some(url::Url::parse(&config.remote_store_url)?),
            local_ingestion_path: None,
            rpc_api_url: config
                .rpc_api_url
                .as_ref()
                .map(|url| url.parse())
                .transpose()?,
            rpc_username: config.rpc_username.clone(),
            rpc_password: config.rpc_password.clone(),
        },
        streaming: sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs {
            streaming_url: config
                .streaming_url
                .as_ref()
                .map(|url| url.parse())
                .transpose()?,
        },
    };

    // Use framework config types directly from JobConfig
    let ingestion_config = config.ingestion.clone();
    let concurrent_config = config.concurrent.clone();

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
    for pipeline_config in config.pipeline_configs() {
        info!("Registering pipeline: {}", pipeline_config.pipeline);

        register_pipeline(
            &mut indexer,
            pipeline_config,
            Some(package_cache.clone()),
            concurrent_config.clone(),
        )
        .await?;
    }

    Ok(indexer)
}

async fn register_pipeline(
    indexer: &mut Indexer<sui_indexer_alt_object_store::ObjectStore>,
    pipeline_config: &PipelineConfig,
    package_cache: Option<Arc<PackageCache>>,
    config: sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig,
) -> Result<()> {
    pipeline_config
        .pipeline
        .register_handler(indexer, pipeline_config, package_cache, config)
        .await
}

async fn create_object_store_from_config(
    config: &ObjectStoreConfig,
) -> Result<Arc<dyn object_store::ObjectStore>> {
    use anyhow::Context;
    let store = config
        .make()
        .context("Failed to create object store from configuration")?;
    Ok(store)
}

fn load_password(path: &str) -> Result<String> {
    use std::fs;
    Ok(fs::read_to_string(path)?.trim().to_string())
}

/// Spawn background tasks to monitor Snowflake table checkpoints
pub fn spawn_snowflake_monitors(
    config: &JobConfig,
    metrics: crate::analytics_metrics::AnalyticsMetrics,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Vec<tokio::task::JoinHandle<()>>> {
    let mut handles = Vec::new();

    for pipeline_config in config.pipeline_configs() {
        if !pipeline_config.report_sf_max_table_checkpoint {
            continue;
        }

        let sf_table_id = pipeline_config
            .sf_table_id
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Missing sf_table_id for pipeline {}",
                    pipeline_config.pipeline
                )
            })?
            .clone();

        let sf_checkpoint_col_id = pipeline_config
            .sf_checkpoint_col_id
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Missing sf_checkpoint_col_id for pipeline {}",
                    pipeline_config.pipeline
                )
            })?
            .clone();

        let account_identifier = config
            .sf_account_identifier
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_account_identifier"))?
            .clone();

        let warehouse = config
            .sf_warehouse
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_warehouse"))?
            .clone();

        let database = config
            .sf_database
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_database"))?
            .clone();

        let schema = config
            .sf_schema
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_schema"))?
            .clone();

        let username = config
            .sf_username
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_username"))?
            .clone();

        let role = config
            .sf_role
            .as_ref()
            .ok_or_else(|| anyhow!("Missing sf_role"))?
            .clone();

        let password = load_password(
            config
                .sf_password_file
                .as_ref()
                .ok_or_else(|| anyhow!("Missing sf_password_file"))?,
        )?;

        let pipeline_name = pipeline_config.pipeline.to_string();
        let metrics = metrics.clone();
        let cancel = cancel.clone();

        let handle = tokio::spawn(async move {
            info!("Starting Snowflake monitor for pipeline: {}", pipeline_name);

            let reader = match SnowflakeMaxCheckpointReader::new(
                &account_identifier,
                &warehouse,
                &database,
                &schema,
                &username,
                &role,
                &password,
                &sf_table_id,
                &sf_checkpoint_col_id,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(
                        "Failed to create Snowflake reader for {}: {}",
                        pipeline_name,
                        e
                    );
                    return;
                }
            };

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        match reader.max_checkpoint().await {
                            Ok(max_cp) => {
                                metrics
                                    .max_checkpoint_on_store
                                    .with_label_values(&[&pipeline_name])
                                    .set(max_cp);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to query Snowflake max checkpoint for {}: {}",
                                    pipeline_name,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        });

        handles.push(handle);
    }

    Ok(handles)
}

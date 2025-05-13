// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use arrow_array::{Array, Int32Array};
use gcp_bigquery_client::model::query_request::QueryRequest;
use gcp_bigquery_client::Client;
use handlers::transaction_bcs_handler::TransactionBCSHandler;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use once_cell::sync::Lazy;
use package_store::{LazyPackageCache, PackageCache};
use serde::{Deserialize, Serialize};
use snowflake_api::{QueryResult, SnowflakeApi};
use strum_macros::EnumIter;
use tempfile::TempDir;
use tracing::info;

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_data_ingestion_core::Worker;
use sui_storage::object_store::util::{
    find_all_dirs_with_epoch_prefix, find_all_files_with_epoch_prefix,
};
use sui_types::base_types::EpochId;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::analytics_processor::AnalyticsProcessor;
use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::handlers::df_handler::DynamicFieldHandler;
use crate::handlers::event_handler::EventHandler;
use crate::handlers::move_call_handler::MoveCallHandler;
use crate::handlers::object_handler::ObjectHandler;
use crate::handlers::package_handler::PackageHandler;
use crate::handlers::transaction_handler::TransactionHandler;
use crate::handlers::transaction_objects_handler::TransactionObjectsHandler;
use crate::handlers::wrapped_object_handler::WrappedObjectHandler;
use crate::handlers::AnalyticsHandler;
use crate::tables::{InputObjectKind, ObjectStatus, OwnerType};
use crate::writers::csv_writer::CSVWriter;
use crate::writers::parquet_writer::ParquetWriter;
use crate::writers::AnalyticsWriter;
use gcp_bigquery_client::model::query_response::ResultSet;

pub mod analytics_metrics;
pub mod analytics_processor;
pub mod errors;
mod handlers;
pub mod package_store;
pub mod tables;
mod writers;

const EPOCH_DIR_PREFIX: &str = "epoch_";
const CHECKPOINT_DIR_PREFIX: &str = "checkpoints";
const OBJECT_DIR_PREFIX: &str = "objects";
const TRANSACTION_DIR_PREFIX: &str = "transactions";
const TRANSACTION_BCS_DIR_PREFIX: &str = "transaction_bcs";
const EVENT_DIR_PREFIX: &str = "events";
const TRANSACTION_OBJECT_DIR_PREFIX: &str = "transaction_objects";
const MOVE_CALL_PREFIX: &str = "move_call";
const MOVE_PACKAGE_PREFIX: &str = "move_package";
const DYNAMIC_FIELD_PREFIX: &str = "dynamic_field";

const WRAPPED_OBJECT_PREFIX: &str = "wrapped_object";

const TRANSACTION_CONCURRENCY_LIMIT_VAR_NAME: &str = "ASYNC_TRANSACTIONS_TO_BUFFER";
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

    // This is private to enforce using the TaskContext struct
    #[serde(rename = "tasks")]
    task_configs: Vec<TaskConfig>,
}

impl JobConfig {
    pub async fn create_checkpoint_processors(
        self,
        metrics: AnalyticsMetrics,
    ) -> Result<(Vec<Processor>, Option<Arc<PackageCache>>)> {
        use crate::package_store::LazyPackageCache;
        use std::sync::Mutex;

        let lazy_package_cache = Arc::new(Mutex::new(LazyPackageCache::new(
            self.package_cache_path.clone(),
            self.rest_url.clone(),
        )));

        let job_config = Arc::new(self);
        let mut processors = Vec::with_capacity(job_config.task_configs.len());
        let mut task_names = HashSet::new();

        for task_config in job_config.task_configs.clone() {
            let task_name = &task_config.task_name;

            if !task_names.insert(task_name.clone()) {
                return Err(anyhow!("Duplicate task_name '{}' found", task_name));
            }

            let temp_dir = tempfile::Builder::new()
                .prefix(&format!("{}-work-dir", task_name))
                .tempdir_in(&job_config.checkpoint_root)?;

            let task_context = TaskContext {
                job_config: Arc::clone(&job_config),
                config: task_config,
                checkpoint_dir: Arc::new(temp_dir),
                metrics: metrics.clone(),
                lazy_package_cache: lazy_package_cache.clone(),
            };

            processors.push(task_context.create_analytics_processor().await?);
        }

        let package_cache = lazy_package_cache
            .lock()
            .unwrap()
            .get_cache_if_initialized();

        Ok((processors, package_cache))
    }

    // Convenience method to get task configs for compatibility
    pub fn task_configs(&self) -> &[TaskConfig] {
        &self.task_configs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Name of the task. Must be unique per process. Used to identify tasks in the Progress Store.
    pub task_name: String,
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

impl TaskConfig {
    pub fn remote_store_path_prefix(&self) -> Result<Option<Path>> {
        self.remote_store_path_prefix
            .as_ref()
            .map(|pb| Ok(Path::from(pb.as_str())))
            .transpose()
    }
}

pub struct TaskContext {
    pub config: TaskConfig,
    pub job_config: Arc<JobConfig>,
    pub checkpoint_dir: Arc<TempDir>,
    pub metrics: AnalyticsMetrics,
    pub lazy_package_cache: Arc<Mutex<LazyPackageCache>>,
}

impl TaskContext {
    pub fn checkpoint_dir_path(&self) -> &std::path::Path {
        self.checkpoint_dir.path()
    }

    pub fn task_name(&self) -> &str {
        &self.config.task_name
    }

    pub async fn create_analytics_processor(self) -> Result<Processor> {
        match &self.config.file_type {
            FileType::Checkpoint => {
                self.create_processor_for_handler(Box::new(CheckpointHandler::new()))
                    .await
            }
            FileType::Object => {
                let package_id_filter = self.config.package_id_filter.clone();
                let package_cache = self
                    .lazy_package_cache
                    .lock()
                    .unwrap()
                    .initialize_or_get_cache();
                let metrics = self.metrics.clone();
                self.create_processor_for_handler(Box::new(ObjectHandler::new(
                    package_cache,
                    &package_id_filter,
                    metrics,
                )))
                .await
            }
            FileType::Transaction => {
                self.create_processor_for_handler(Box::new(TransactionHandler::new()))
                    .await
            }
            FileType::TransactionBCS => {
                self.create_processor_for_handler(Box::new(TransactionBCSHandler::new()))
                    .await
            }
            FileType::Event => {
                let package_cache = self
                    .lazy_package_cache
                    .lock()
                    .unwrap()
                    .initialize_or_get_cache();
                self.create_processor_for_handler(Box::new(EventHandler::new(package_cache)))
                    .await
            }
            FileType::TransactionObjects => {
                self.create_processor_for_handler(Box::new(TransactionObjectsHandler::new()))
                    .await
            }
            FileType::MoveCall => {
                self.create_processor_for_handler(Box::new(MoveCallHandler::new()))
                    .await
            }
            FileType::MovePackage => {
                self.create_processor_for_handler(Box::new(PackageHandler::new()))
                    .await
            }
            FileType::DynamicField => {
                let package_cache = self
                    .lazy_package_cache
                    .lock()
                    .unwrap()
                    .initialize_or_get_cache();
                self.create_processor_for_handler(Box::new(DynamicFieldHandler::new(package_cache)))
                    .await
            }
            FileType::WrappedObject => {
                let package_cache = self
                    .lazy_package_cache
                    .lock()
                    .unwrap()
                    .initialize_or_get_cache();
                let metrics = self.metrics.clone();
                self.create_processor_for_handler(Box::new(WrappedObjectHandler::new(
                    package_cache,
                    metrics,
                )))
                .await
            }
        }
    }

    async fn create_processor_for_handler<
        T: Serialize + Clone + ParquetSchema + Send + Sync + 'static,
    >(
        self,
        handler: Box<dyn AnalyticsHandler<T>>,
    ) -> Result<Processor> {
        let starting_checkpoint_seq_num = self.get_starting_checkpoint_seq_num().await?;
        let writer = self.make_writer::<T>(starting_checkpoint_seq_num)?;
        let max_checkpoint_reader = self.make_max_checkpoint_reader().await?;
        Processor::new::<T>(
            handler,
            writer,
            max_checkpoint_reader,
            starting_checkpoint_seq_num,
            self,
        )
        .await
    }

    async fn get_starting_checkpoint_seq_num(&self) -> Result<u64> {
        let remote_latest = read_store_for_checkpoint(
            &self.job_config.remote_store_config,
            self.config.file_type,
            self.config.remote_store_path_prefix()?.as_ref(),
        )
        .await?;

        Ok(self
            .config
            .starting_checkpoint_seq_num
            .map_or(remote_latest, |start| start.max(remote_latest)))
    }

    fn make_writer<S: Serialize + ParquetSchema>(
        &self,
        starting_checkpoint_seq_num: u64,
    ) -> Result<Box<dyn AnalyticsWriter<S>>> {
        Ok(match self.config.file_format {
            FileFormat::CSV => Box::new(CSVWriter::new(
                self.checkpoint_dir_path(),
                self.config.file_type,
                starting_checkpoint_seq_num,
            )?),
            FileFormat::PARQUET => Box::new(ParquetWriter::new(
                self.checkpoint_dir_path(),
                self.config.file_type,
                starting_checkpoint_seq_num,
            )?),
        })
    }

    async fn make_max_checkpoint_reader(&self) -> Result<Box<dyn MaxCheckpointReader>> {
        let res: Box<dyn MaxCheckpointReader> = if self.config.report_bq_max_table_checkpoint {
            Box::new(
                BQMaxCheckpointReader::new(
                    self.job_config
                        .bq_service_account_key_file
                        .as_ref()
                        .ok_or(anyhow!("Missing gcp key file"))?,
                    self.job_config
                        .bq_project_id
                        .as_ref()
                        .ok_or(anyhow!("Missing big query project id"))?,
                    self.job_config
                        .bq_dataset_id
                        .as_ref()
                        .ok_or(anyhow!("Missing big query dataset id"))?,
                    self.config
                        .bq_table_id
                        .as_ref()
                        .ok_or(anyhow!("Missing big query table id"))?,
                    self.config
                        .bq_checkpoint_col_id
                        .as_ref()
                        .ok_or(anyhow!("Missing big query checkpoint col id"))?,
                )
                .await?,
            )
        } else if self.config.report_sf_max_table_checkpoint {
            Box::new(
                SnowflakeMaxCheckpointReader::new(
                    self.job_config
                        .sf_account_identifier
                        .as_ref()
                        .ok_or(anyhow!("Missing sf account identifier"))?,
                    self.job_config
                        .sf_warehouse
                        .as_ref()
                        .ok_or(anyhow!("Missing sf warehouse"))?,
                    self.job_config
                        .sf_database
                        .as_ref()
                        .ok_or(anyhow!("Missing sf database"))?,
                    self.job_config
                        .sf_schema
                        .as_ref()
                        .ok_or(anyhow!("Missing sf schema"))?,
                    self.job_config
                        .sf_username
                        .as_ref()
                        .ok_or(anyhow!("Missing sf username"))?,
                    self.job_config
                        .sf_role
                        .as_ref()
                        .ok_or(anyhow!("Missing sf role"))?,
                    &load_password(
                        self.job_config
                            .sf_password_file
                            .as_ref()
                            .ok_or(anyhow!("Missing sf password"))?,
                    )?,
                    self.config
                        .sf_table_id
                        .as_ref()
                        .ok_or(anyhow!("Missing sf table id"))?,
                    self.config
                        .sf_checkpoint_col_id
                        .as_ref()
                        .ok_or(anyhow!("Missing sf checkpoint col id"))?,
                )
                .await?,
            )
        } else {
            Box::new(NoOpCheckpointReader {})
        };
        Ok(res)
    }
}

#[async_trait::async_trait]
pub trait MaxCheckpointReader: Send + Sync + 'static {
    async fn max_checkpoint(&self) -> Result<i64>;
}

struct SnowflakeMaxCheckpointReader {
    query: String,
    api: SnowflakeApi,
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
    ) -> anyhow::Result<Self> {
        let api = SnowflakeApi::with_password_auth(
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

struct BQMaxCheckpointReader {
    query: String,
    project_id: String,
    client: Client,
}

impl BQMaxCheckpointReader {
    pub async fn new(
        key_path: &str,
        project_id: &str,
        dataset_id: &str,
        table_id: &str,
        col_id: &str,
    ) -> anyhow::Result<Self> {
        Ok(BQMaxCheckpointReader {
            query: format!(
                "SELECT max({}) from `{}.{}.{}`",
                col_id, project_id, dataset_id, table_id
            ),
            client: Client::from_service_account_key_file(key_path).await?,
            project_id: project_id.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl MaxCheckpointReader for BQMaxCheckpointReader {
    async fn max_checkpoint(&self) -> Result<i64> {
        let result = self
            .client
            .job()
            .query(&self.project_id, QueryRequest::new(&self.query))
            .await?;
        let mut result_set = ResultSet::new_from_query_response(result);
        if result_set.next_row() {
            let max_checkpoint = result_set.get_i64(0)?.ok_or(anyhow!("No rows returned"))?;
            Ok(max_checkpoint)
        } else {
            Ok(-1)
        }
    }
}

struct NoOpCheckpointReader;

#[async_trait::async_trait]
impl MaxCheckpointReader for NoOpCheckpointReader {
    async fn max_checkpoint(&self) -> Result<i64> {
        Ok(-1)
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
    fn new(
        file_type: FileType,
        file_format: FileFormat,
        epoch_num: u64,
        checkpoint_seq_range: Range<u64>,
    ) -> FileMetadata {
        FileMetadata {
            file_type,
            file_format,
            epoch_num,
            checkpoint_seq_range,
        }
    }

    pub fn file_path(&self) -> Path {
        self.file_type.file_path(
            self.file_format,
            self.epoch_num,
            self.checkpoint_seq_range.clone(),
        )
    }
}

pub struct Processor {
    pub processor: Box<dyn Worker<Result = ()>>,
    pub starting_checkpoint_seq_num: CheckpointSequenceNumber,
    pub task_name: String,
    pub file_type: FileType,
}

#[async_trait::async_trait]
impl Worker for Processor {
    type Result = ();

    #[inline]
    async fn process_checkpoint_arc(&self, checkpoint_data: &Arc<CheckpointData>) -> Result<()> {
        self.processor.process_checkpoint_arc(checkpoint_data).await
    }
}

impl Processor {
    pub async fn new<S: Serialize + ParquetSchema + Send + Sync + 'static>(
        handler: Box<dyn AnalyticsHandler<S>>,
        writer: Box<dyn AnalyticsWriter<S>>,
        max_checkpoint_reader: Box<dyn MaxCheckpointReader>,
        starting_checkpoint_seq_num: CheckpointSequenceNumber,
        task: TaskContext,
    ) -> Result<Self> {
        let task_name = task.config.task_name.clone();
        let file_type = task.config.file_type;
        let processor = Box::new(
            AnalyticsProcessor::new(
                handler,
                writer,
                max_checkpoint_reader,
                starting_checkpoint_seq_num,
                task,
            )
            .await?,
        );

        Ok(Processor {
            processor,
            starting_checkpoint_seq_num,
            task_name,
            file_type,
        })
    }

    pub fn last_committed_checkpoint(&self) -> Option<u64> {
        Some(self.starting_checkpoint_seq_num.saturating_sub(1)).filter(|x| *x > 0)
    }
}

pub async fn read_store_for_checkpoint(
    remote_store_config: &ObjectStoreConfig,
    file_type: FileType,
    dir_prefix: Option<&Path>,
) -> Result<CheckpointSequenceNumber> {
    let remote_object_store = remote_store_config.make()?;
    let remote_store_is_empty = remote_object_store
        .list_with_delimiter(None)
        .await
        .expect("Failed to read remote analytics store")
        .common_prefixes
        .is_empty();
    info!("Remote store is empty: {remote_store_is_empty}");
    let file_type_prefix = file_type.dir_prefix();
    let prefix = join_paths(dir_prefix, &file_type_prefix);
    let epoch_dirs = find_all_dirs_with_epoch_prefix(&remote_object_store, Some(&prefix)).await?;
    let epoch = epoch_dirs.last_key_value().map(|(k, _v)| *k).unwrap_or(0);
    let epoch_prefix = prefix.child(format!("epoch_{}", epoch));
    let checkpoints =
        find_all_files_with_epoch_prefix(&remote_object_store, Some(&epoch_prefix)).await?;
    let next_checkpoint_seq_num = checkpoints
        .iter()
        .max_by(|x, y| x.end.cmp(&y.end))
        .map(|r| r.end)
        .unwrap_or(0);
    Ok(next_checkpoint_seq_num)
}

pub fn join_paths(base: Option<&Path>, child: &Path) -> Path {
    base.map(|p| {
        let mut out_path = p.clone();
        for part in child.parts() {
            out_path = out_path.child(part)
        }
        out_path
    })
    .unwrap_or(child.clone())
}

fn load_password(path: &str) -> anyhow::Result<String> {
    let contents = fs::read_to_string(std::path::Path::new(path))?;
    Ok(contents.trim().to_string())
}

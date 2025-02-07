// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use arrow_array::{Array, Int32Array};
use clap::*;
use gcp_bigquery_client::model::query_request::QueryRequest;
use gcp_bigquery_client::Client;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use serde::{Deserialize, Serialize};
use snowflake_api::{QueryResult, SnowflakeApi};
use strum_macros::EnumIter;
use tracing::info;

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_data_ingestion_core::Worker;
use sui_rpc_api::CheckpointData;
use sui_storage::object_store::util::{
    find_all_dirs_with_epoch_prefix, find_all_files_with_epoch_prefix,
};
use sui_types::base_types::EpochId;
use sui_types::dynamic_field::DynamicFieldType;
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
use crate::tables::{
    CheckpointEntry, DynamicFieldEntry, EventEntry, InputObjectKind, MoveCallEntry,
    MovePackageEntry, ObjectEntry, ObjectStatus, OwnerType, TransactionEntry,
    TransactionObjectEntry, WrappedObjectEntry,
};
use crate::writers::csv_writer::CSVWriter;
use crate::writers::parquet_writer::ParquetWriter;
use crate::writers::AnalyticsWriter;
use gcp_bigquery_client::model::query_response::ResultSet;

pub mod analytics_metrics;
pub mod analytics_processor;
pub mod errors;
mod handlers;
mod package_store;
pub mod tables;
mod writers;

const EPOCH_DIR_PREFIX: &str = "epoch_";
const CHECKPOINT_DIR_PREFIX: &str = "checkpoints";
const OBJECT_DIR_PREFIX: &str = "objects";
const TRANSACTION_DIR_PREFIX: &str = "transactions";
const EVENT_DIR_PREFIX: &str = "events";
const TRANSACTION_OBJECT_DIR_PREFIX: &str = "transaction_objects";
const MOVE_CALL_PREFIX: &str = "move_call";
const MOVE_PACKAGE_PREFIX: &str = "move_package";
const DYNAMIC_FIELD_PREFIX: &str = "dynamic_field";

const WRAPPED_OBJECT_PREFIX: &str = "wrapped_object";

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Analytics Indexer",
    about = "Indexer service to upload data for the analytics pipeline.",
    rename_all = "kebab-case"
)]
pub struct AnalyticsIndexerConfig {
    /// The url of the checkpoint client to connect to.
    #[clap(long)]
    pub rest_url: String,
    /// The url of the metrics client to connect to.
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
    /// Directory to contain the temporary files for checkpoint entries.
    #[clap(long, global = true, default_value = "/tmp")]
    pub checkpoint_dir: PathBuf,
    /// Number of checkpoints to process before uploading to the datastore.
    #[clap(long, default_value = "10000", global = true)]
    pub checkpoint_interval: u64,
    /// Maximum file size in mb before uploading to the datastore.
    #[clap(long, default_value = "100", global = true)]
    pub max_file_size_mb: u64,
    /// Checkpoint sequence number to start the download from
    #[clap(long, default_value = None, global = true)]
    pub starting_checkpoint_seq_num: Option<u64>,
    /// Time to process in seconds before uploading to the datastore.
    #[clap(long, default_value = "600", global = true)]
    pub time_interval_s: u64,
    // Remote object store where data gets written to
    #[command(flatten)]
    pub remote_store_config: ObjectStoreConfig,
    // Remote object store path prefix to use while writing
    #[clap(long, default_value = None, global = true)]
    pub remote_store_path_prefix: Option<Path>,
    // File format to store data in i.e. csv, parquet, etc
    #[clap(long, value_enum, default_value = "csv", global = true)]
    pub file_format: FileFormat,
    // Type of data to write i.e. checkpoint, object, transaction, etc
    #[clap(long, value_enum, long, global = true)]
    pub file_type: FileType,
    #[clap(
        long,
        default_value = "https://checkpoints.mainnet.sui.io",
        global = true
    )]
    pub remote_store_url: String,
    // Directory to contain the package cache for pipelines
    #[clap(
        long,
        value_enum,
        long,
        global = true,
        default_value = "/opt/sui/db/package_cache"
    )]
    pub package_cache_path: PathBuf,
    #[clap(long, default_value = None, global = true)]
    pub bq_service_account_key_file: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub bq_project_id: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub bq_dataset_id: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub bq_table_id: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub bq_checkpoint_col_id: Option<String>,
    #[clap(long, global = true)]
    pub report_bq_max_table_checkpoint: bool,
    #[clap(long, default_value = None, global = true)]
    pub sf_account_identifier: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_warehouse: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_database: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_schema: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_username: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_role: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_password: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_table_id: Option<String>,
    #[clap(long, default_value = None, global = true)]
    pub sf_checkpoint_col_id: Option<String>,
    #[clap(long, global = true)]
    pub report_sf_max_table_checkpoint: bool,
    #[clap(long, default_value = None, global = true)]
    pub package_id_filter: Option<String>,
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
    Parser,
    strum_macros::Display,
    ValueEnum,
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
    ValueEnum,
)]
#[repr(u8)]
pub enum FileType {
    Checkpoint = 0,
    Object,
    Transaction,
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
}

#[async_trait::async_trait]
impl Worker for Processor {
    type Result = ();

    #[inline]
    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        self.processor.process_checkpoint(checkpoint_data).await
    }
}

impl Processor {
    pub async fn new<S: Serialize + ParquetSchema + 'static>(
        handler: Box<dyn AnalyticsHandler<S>>,
        writer: Box<dyn AnalyticsWriter<S>>,
        max_checkpoint_reader: Box<dyn MaxCheckpointReader>,
        starting_checkpoint_seq_num: CheckpointSequenceNumber,
        metrics: AnalyticsMetrics,
        config: AnalyticsIndexerConfig,
    ) -> Result<Self> {
        let processor = Box::new(
            AnalyticsProcessor::new(
                handler,
                writer,
                max_checkpoint_reader,
                starting_checkpoint_seq_num,
                metrics,
                config,
            )
            .await?,
        );

        Ok(Processor {
            processor,
            starting_checkpoint_seq_num,
        })
    }

    pub fn last_committed_checkpoint(&self) -> Option<u64> {
        Some(self.starting_checkpoint_seq_num.saturating_sub(1)).filter(|x| *x > 0)
    }
}

pub async fn read_store_for_checkpoint(
    remote_store_config: ObjectStoreConfig,
    file_type: FileType,
    dir_prefix: Option<Path>,
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

pub async fn make_max_checkpoint_reader(
    config: &AnalyticsIndexerConfig,
) -> Result<Box<dyn MaxCheckpointReader>> {
    let res: Box<dyn MaxCheckpointReader> = if config.report_bq_max_table_checkpoint {
        Box::new(
            BQMaxCheckpointReader::new(
                config
                    .bq_service_account_key_file
                    .as_ref()
                    .ok_or(anyhow!("Missing gcp key file"))?,
                config
                    .bq_project_id
                    .as_ref()
                    .ok_or(anyhow!("Missing big query project id"))?,
                config
                    .bq_dataset_id
                    .as_ref()
                    .ok_or(anyhow!("Missing big query dataset id"))?,
                config
                    .bq_table_id
                    .as_ref()
                    .ok_or(anyhow!("Missing big query table id"))?,
                config
                    .bq_checkpoint_col_id
                    .as_ref()
                    .ok_or(anyhow!("Missing big query checkpoint col id"))?,
            )
            .await?,
        )
    } else if config.report_sf_max_table_checkpoint {
        Box::new(
            SnowflakeMaxCheckpointReader::new(
                config
                    .sf_account_identifier
                    .as_ref()
                    .ok_or(anyhow!("Missing sf account identifier"))?,
                config
                    .sf_warehouse
                    .as_ref()
                    .ok_or(anyhow!("Missing sf warehouse"))?,
                config
                    .sf_database
                    .as_ref()
                    .ok_or(anyhow!("Missing sf database"))?,
                config
                    .sf_schema
                    .as_ref()
                    .ok_or(anyhow!("Missing sf schema"))?,
                config
                    .sf_username
                    .as_ref()
                    .ok_or(anyhow!("Missing sf username"))?,
                config.sf_role.as_ref().ok_or(anyhow!("Missing sf role"))?,
                config
                    .sf_password
                    .as_ref()
                    .ok_or(anyhow!("Missing sf password"))?,
                config
                    .sf_table_id
                    .as_ref()
                    .ok_or(anyhow!("Missing sf table id"))?,
                config
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

pub async fn make_checkpoint_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<CheckpointEntry>> = Box::new(CheckpointHandler::new());
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Checkpoint).await?;
    let writer = make_writer::<CheckpointEntry>(
        config.clone(),
        FileType::Checkpoint,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<CheckpointEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_transaction_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<TransactionEntry>> = Box::new(TransactionHandler::new());
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Transaction).await?;
    let writer = make_writer::<TransactionEntry>(
        config.clone(),
        FileType::Transaction,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<TransactionEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_object_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<ObjectEntry>> = Box::new(ObjectHandler::new(
        &config.package_cache_path,
        &config.rest_url,
        &config.package_id_filter,
    ));
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Object).await?;
    let writer = make_writer::<ObjectEntry>(
        config.clone(),
        FileType::Object,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<ObjectEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_event_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<EventEntry>> = Box::new(EventHandler::new(
        &config.package_cache_path,
        &config.rest_url,
    ));
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Event).await?;
    let writer =
        make_writer::<EventEntry>(config.clone(), FileType::Event, starting_checkpoint_seq_num)?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<EventEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_transaction_objects_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::TransactionObjects).await?;
    let handler = Box::new(TransactionObjectsHandler::new());
    let writer = make_writer(
        config.clone(),
        FileType::TransactionObjects,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<TransactionObjectEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_move_package_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<MovePackageEntry>> = Box::new(PackageHandler::new());
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::MovePackage).await?;
    let writer = make_writer::<MovePackageEntry>(
        config.clone(),
        FileType::MovePackage,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<MovePackageEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_move_call_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::MoveCall).await?;
    let handler: Box<dyn AnalyticsHandler<MoveCallEntry>> = Box::new(MoveCallHandler::new());
    let writer = make_writer::<MoveCallEntry>(
        config.clone(),
        FileType::MoveCall,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<MoveCallEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_dynamic_field_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::DynamicField).await?;
    let handler: Box<dyn AnalyticsHandler<DynamicFieldEntry>> = Box::new(DynamicFieldHandler::new(
        &config.package_cache_path,
        &config.rest_url,
    ));
    let writer = make_writer::<DynamicFieldEntry>(
        config.clone(),
        FileType::DynamicField,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<DynamicFieldEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub async fn make_wrapped_object_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::WrappedObject).await?;
    let handler: Box<dyn AnalyticsHandler<WrappedObjectEntry>> = Box::new(
        WrappedObjectHandler::new(&config.package_cache_path, &config.rest_url),
    );
    let writer = make_writer::<WrappedObjectEntry>(
        config.clone(),
        FileType::WrappedObject,
        starting_checkpoint_seq_num,
    )?;
    let max_checkpoint_reader = make_max_checkpoint_reader(&config).await?;
    Processor::new::<WrappedObjectEntry>(
        handler,
        writer,
        max_checkpoint_reader,
        starting_checkpoint_seq_num,
        metrics,
        config,
    )
    .await
}

pub fn make_writer<S: Serialize + ParquetSchema>(
    config: AnalyticsIndexerConfig,
    file_type: FileType,
    starting_checkpoint_seq_num: u64,
) -> Result<Box<dyn AnalyticsWriter<S>>> {
    Ok(match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            file_type,
            starting_checkpoint_seq_num,
        )?),
        FileFormat::PARQUET => Box::new(ParquetWriter::new(
            &config.checkpoint_dir,
            file_type,
            starting_checkpoint_seq_num,
        )?),
    })
}

pub async fn get_starting_checkpoint_seq_num(
    config: AnalyticsIndexerConfig,
    file_type: FileType,
) -> Result<u64> {
    let remote_latest = read_store_for_checkpoint(
        config.remote_store_config,
        file_type,
        config.remote_store_path_prefix,
    )
    .await?;

    Ok(config
        .starting_checkpoint_seq_num
        .map_or(remote_latest, |start| start.max(remote_latest)))
}

pub async fn make_analytics_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    match config.file_type {
        FileType::Checkpoint => make_checkpoint_processor(config, metrics).await,
        FileType::Object => make_object_processor(config, metrics).await,
        FileType::Transaction => make_transaction_processor(config, metrics).await,
        FileType::Event => make_event_processor(config, metrics).await,
        FileType::TransactionObjects => make_transaction_objects_processor(config, metrics).await,
        FileType::MoveCall => make_move_call_processor(config, metrics).await,
        FileType::MovePackage => make_move_package_processor(config, metrics).await,
        FileType::DynamicField => make_dynamic_field_processor(config, metrics).await,
        FileType::WrappedObject => make_wrapped_object_processor(config, metrics).await,
    }
}

pub fn join_paths(base: Option<Path>, child: &Path) -> Path {
    base.map(|p| {
        let mut out_path = p.clone();
        for part in child.parts() {
            out_path = out_path.child(part)
        }
        out_path
    })
    .unwrap_or(child.clone())
}

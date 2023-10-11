// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::path::PathBuf;

use anyhow::Result;
use clap::*;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tracing::info;

use sui_indexer::framework::Handler;
use sui_rest_api::CheckpointData;
use sui_storage::object_store::util::{
    find_all_dirs_with_epoch_prefix, find_all_files_with_epoch_prefix,
};
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::base_types::EpochId;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::analytics_processor::AnalyticsProcessor;
use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::handlers::event_handler::EventHandler;
use crate::handlers::move_call_handler::MoveCallHandler;
use crate::handlers::object_handler::ObjectHandler;
use crate::handlers::package_handler::PackageHandler;
use crate::handlers::transaction_handler::TransactionHandler;
use crate::handlers::transaction_objects_handler::TransactionObjectsHandler;
use crate::handlers::AnalyticsHandler;
use crate::tables::{
    CheckpointEntry, EventEntry, MoveCallEntry, MovePackageEntry, ObjectEntry, TransactionEntry,
    TransactionObjectEntry,
};
use crate::writers::csv_writer::CSVWriter;
use crate::writers::parquet_writer::ParquetWriter;
use crate::writers::AnalyticsWriter;

pub mod analytics_metrics;
pub mod analytics_processor;
pub mod errors;
mod handlers;
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
    /// Checkpoint sequence number to start the download from
    #[clap(long, default_value = None, global = true)]
    pub starting_checkpoint_seq_num: Option<u64>,
    /// Time to process in seconds before uploading to the datastore.
    #[clap(long, default_value = "600", global = true)]
    pub time_interval_s: u64,
    // Remote object store where data gets written to
    #[command(flatten)]
    pub remote_store_config: ObjectStoreConfig,
    // File format to store data in i.e. csv, parquet, etc
    #[clap(long, value_enum, default_value = "csv", global = true)]
    pub file_format: FileFormat,
    // Type of data to write i.e. checkpoint, object, transaction, etc
    #[clap(long, value_enum, long, global = true)]
    pub file_type: FileType,
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
    OptionU64(Option<u64>),
    OptionStr(Option<String>),
}

impl From<u64> for ParquetValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
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
    pub processor: Box<dyn Handler>,
    pub starting_checkpoint_seq_num: CheckpointSequenceNumber,
}

#[async_trait::async_trait]
impl Handler for Processor {
    #[inline]
    fn name(&self) -> &str {
        self.processor.name()
    }

    #[inline]
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        self.processor.process_checkpoint(checkpoint_data).await
    }
}

impl Processor {
    pub async fn new<S: Serialize + ParquetSchema + 'static>(
        handler: Box<dyn AnalyticsHandler<S>>,
        writer: Box<dyn AnalyticsWriter<S>>,
        starting_checkpoint_seq_num: CheckpointSequenceNumber,
        metrics: AnalyticsMetrics,
        config: AnalyticsIndexerConfig,
    ) -> Result<Self> {
        let processor = Box::new(
            AnalyticsProcessor::new(
                handler,
                writer,
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
) -> Result<CheckpointSequenceNumber> {
    let remote_object_store = remote_store_config.make()?;
    let remote_store_is_empty = remote_object_store
        .list_with_delimiter(None)
        .await
        .expect("Failed to read remote analytics store")
        .common_prefixes
        .is_empty();
    info!("Remote store is empty: {remote_store_is_empty}");
    let prefix = file_type.dir_prefix();
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
    Processor::new::<CheckpointEntry>(
        handler,
        writer,
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
    Processor::new::<TransactionEntry>(
        handler,
        writer,
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
    let handler: Box<dyn AnalyticsHandler<ObjectEntry>> = Box::new(ObjectHandler::new());
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Object).await?;
    let writer = make_writer::<ObjectEntry>(
        config.clone(),
        FileType::Object,
        starting_checkpoint_seq_num,
    )?;
    Processor::new::<ObjectEntry>(
        handler,
        writer,
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
    let handler: Box<dyn AnalyticsHandler<EventEntry>> = Box::new(EventHandler::new());
    let starting_checkpoint_seq_num =
        get_starting_checkpoint_seq_num(config.clone(), FileType::Event).await?;
    let writer =
        make_writer::<EventEntry>(config.clone(), FileType::Event, starting_checkpoint_seq_num)?;
    Processor::new::<EventEntry>(
        handler,
        writer,
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
    Processor::new::<TransactionObjectEntry>(
        handler,
        writer,
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
    Processor::new::<MovePackageEntry>(
        handler,
        writer,
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
    Processor::new::<MoveCallEntry>(
        handler,
        writer,
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
    let checkpoint = if let Some(starting_checkpoint_seq_num) = config.starting_checkpoint_seq_num {
        starting_checkpoint_seq_num
    } else {
        read_store_for_checkpoint(config.remote_store_config.clone(), file_type).await?
    };
    Ok(checkpoint)
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
    }
}

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
}

impl FileFormat {
    pub fn file_suffix(&self) -> &str {
        match self {
            FileFormat::CSV => "csv",
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
    pub handler: Box<dyn Handler>,
    pub starting_checkpoint_seq_num: CheckpointSequenceNumber,
}

#[async_trait::async_trait]
impl Handler for Processor {
    #[inline]
    fn name(&self) -> &str {
        self.handler.name()
    }

    #[inline]
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        self.handler.process_checkpoint(checkpoint_data).await
    }
}

impl Processor {
    pub fn new(
        handler: Box<dyn Handler>,
        starting_checkpoint_seq_num: CheckpointSequenceNumber,
    ) -> Self {
        Processor {
            handler,
            starting_checkpoint_seq_num,
        }
    }

    pub fn last_committed_checkpoint(&self) -> Option<u64> {
        Some(self.starting_checkpoint_seq_num.saturating_sub(1)).filter(|x| *x > 0)
    }
}

pub async fn read_store_for_epoch_and_checkpoint(
    remote_store_config: ObjectStoreConfig,
    file_type: FileType,
) -> Result<(EpochId, CheckpointSequenceNumber)> {
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
    Ok((epoch, next_checkpoint_seq_num))
}

pub async fn make_checkpoint_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<CheckpointEntry>> = Box::new(CheckpointHandler::new());
    let (epoch, starting_checkpoint_seq_num) = read_store_for_epoch_and_checkpoint(
        config.remote_store_config.clone(),
        FileType::Checkpoint,
    )
    .await?;
    let writer: Box<dyn AnalyticsWriter<CheckpointEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::Checkpoint,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_transaction_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<TransactionEntry>> = Box::new(TransactionHandler::new());
    let (epoch, starting_checkpoint_seq_num) = read_store_for_epoch_and_checkpoint(
        config.remote_store_config.clone(),
        FileType::Transaction,
    )
    .await?;
    let writer: Box<dyn AnalyticsWriter<TransactionEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::Transaction,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_object_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<ObjectEntry>> = Box::new(ObjectHandler::new());
    let (epoch, starting_checkpoint_seq_num) =
        read_store_for_epoch_and_checkpoint(config.remote_store_config.clone(), FileType::Object)
            .await?;
    let writer: Box<dyn AnalyticsWriter<ObjectEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::Object,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_event_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<EventEntry>> = Box::new(EventHandler::new());
    let (epoch, starting_checkpoint_seq_num) =
        read_store_for_epoch_and_checkpoint(config.remote_store_config.clone(), FileType::Event)
            .await?;
    let writer: Box<dyn AnalyticsWriter<EventEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::Event,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_transaction_objects_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<TransactionObjectEntry>> =
        Box::new(TransactionObjectsHandler::new());
    let (epoch, starting_checkpoint_seq_num) = read_store_for_epoch_and_checkpoint(
        config.remote_store_config.clone(),
        FileType::TransactionObjects,
    )
    .await?;
    let writer: Box<dyn AnalyticsWriter<TransactionObjectEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::TransactionObjects,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_move_package_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<MovePackageEntry>> = Box::new(PackageHandler::new());
    let (epoch, starting_checkpoint_seq_num) = read_store_for_epoch_and_checkpoint(
        config.remote_store_config.clone(),
        FileType::MovePackage,
    )
    .await?;
    let writer: Box<dyn AnalyticsWriter<MovePackageEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::MovePackage,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
}

pub async fn make_move_call_processor(
    config: AnalyticsIndexerConfig,
    metrics: AnalyticsMetrics,
) -> Result<Processor> {
    let handler: Box<dyn AnalyticsHandler<MoveCallEntry>> = Box::new(MoveCallHandler::new());
    let (epoch, starting_checkpoint_seq_num) =
        read_store_for_epoch_and_checkpoint(config.remote_store_config.clone(), FileType::MoveCall)
            .await?;
    let writer: Box<dyn AnalyticsWriter<MoveCallEntry>> = match config.file_format {
        FileFormat::CSV => Box::new(CSVWriter::new(
            &config.checkpoint_dir,
            epoch,
            FileType::MoveCall,
            starting_checkpoint_seq_num,
        )?),
    };
    let handler = Box::new(
        AnalyticsProcessor::new(
            handler,
            writer,
            epoch,
            starting_checkpoint_seq_num,
            metrics,
            config,
        )
        .await?,
    );
    Ok(Processor::new(handler, starting_checkpoint_seq_num))
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

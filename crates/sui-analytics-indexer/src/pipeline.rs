// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pipeline definitions for the analytics indexer.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::package_store::PackageCache;
use anyhow::Result;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::base_types::EpochId;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::config::{FileFormat, PipelineConfig};
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

/// Available analytics pipelines.
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

impl Pipeline {
    /// Returns the directory prefix for this pipeline's output files.
    pub fn dir_prefix(&self) -> Path {
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

    /// Registers this pipeline's handler with the indexer.
    pub async fn register_handler(
        &self,
        indexer: &mut Indexer<ObjectStore>,
        pipeline_config: &PipelineConfig,
        package_cache: Arc<PackageCache>,
        metrics: AnalyticsMetrics,
        concurrent_config: ConcurrentConfig,
    ) -> Result<()> {
        match self {
            Pipeline::Checkpoint => {
                indexer
                    .concurrent_pipeline(
                        CheckpointHandler::new(
                            CheckpointProcessor,
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::Transaction => {
                indexer
                    .concurrent_pipeline(
                        TransactionHandler::new(
                            TransactionProcessor,
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::TransactionBCS => {
                indexer
                    .concurrent_pipeline(
                        TransactionBCSHandler::new(
                            TransactionBCSProcessor,
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::Event => {
                indexer
                    .concurrent_pipeline(
                        EventHandler::new(
                            EventProcessor::new(package_cache.clone()),
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::MoveCall => {
                indexer
                    .concurrent_pipeline(
                        MoveCallHandler::new(MoveCallProcessor, pipeline_config.clone(), metrics),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::Object => {
                indexer
                    .concurrent_pipeline(
                        ObjectHandler::new(
                            ObjectProcessor::new(
                                package_cache.clone(),
                                &pipeline_config.package_id_filter,
                                metrics.clone(),
                            ),
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::DynamicField => {
                indexer
                    .concurrent_pipeline(
                        DynamicFieldHandler::new(
                            DynamicFieldProcessor::new(package_cache.clone()),
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::TransactionObjects => {
                indexer
                    .concurrent_pipeline(
                        TransactionObjectsHandler::new(
                            TransactionObjectsProcessor,
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::MovePackage => {
                indexer
                    .concurrent_pipeline(
                        PackageHandler::new(PackageProcessor, pipeline_config.clone(), metrics),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::MovePackageBCS => {
                indexer
                    .concurrent_pipeline(
                        PackageBCSHandler::new(
                            PackageBCSProcessor,
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
            Pipeline::WrappedObject => {
                indexer
                    .concurrent_pipeline(
                        WrappedObjectHandler::new(
                            WrappedObjectProcessor::new(package_cache.clone()),
                            pipeline_config.clone(),
                            metrics,
                        ),
                        concurrent_config,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// Constructs a relative file path from directory prefix and metadata.
pub fn construct_file_path(
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

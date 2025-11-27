// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pipeline definitions for the analytics indexer.

use std::sync::Arc;

use crate::package_store::PackageCache;
use anyhow::Result;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::backfill::BackfillBoundaries;
use crate::config::PipelineConfig;
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
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;

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
    pub fn dir_prefix(&self) -> &'static str {
        match self {
            Pipeline::Checkpoint => "checkpoints",
            Pipeline::Transaction => "transactions",
            Pipeline::TransactionBCS => "transaction_bcs",
            Pipeline::TransactionObjects => "transaction_objects",
            Pipeline::Object => "objects",
            Pipeline::Event => "events",
            Pipeline::MoveCall => "move_call",
            Pipeline::MovePackage => "move_package",
            Pipeline::MovePackageBCS => "move_package_bcs",
            Pipeline::DynamicField => "dynamic_field",
            Pipeline::WrappedObject => "wrapped_object",
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
        backfill_cache: Option<Arc<BackfillBoundaries>>,
    ) -> Result<()> {
        match self {
            Pipeline::Checkpoint => {
                let handler = if let Some(cache) = backfill_cache {
                    CheckpointHandler::with_backfill_cache(
                        CheckpointProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    CheckpointHandler::new(CheckpointProcessor, pipeline_config.clone(), metrics)
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::Transaction => {
                let handler = if let Some(cache) = backfill_cache {
                    TransactionHandler::with_backfill_cache(
                        TransactionProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    TransactionHandler::new(TransactionProcessor, pipeline_config.clone(), metrics)
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::TransactionBCS => {
                let handler = if let Some(cache) = backfill_cache {
                    TransactionBCSHandler::with_backfill_cache(
                        TransactionBCSProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    TransactionBCSHandler::new(
                        TransactionBCSProcessor,
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::Event => {
                let handler = if let Some(cache) = backfill_cache {
                    EventHandler::with_backfill_cache(
                        EventProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    EventHandler::new(
                        EventProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::MoveCall => {
                let handler = if let Some(cache) = backfill_cache {
                    MoveCallHandler::with_backfill_cache(
                        MoveCallProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    MoveCallHandler::new(MoveCallProcessor, pipeline_config.clone(), metrics)
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::Object => {
                let handler = if let Some(cache) = backfill_cache {
                    ObjectHandler::with_backfill_cache(
                        ObjectProcessor::new(
                            package_cache.clone(),
                            &pipeline_config.package_id_filter,
                            metrics.clone(),
                        ),
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    ObjectHandler::new(
                        ObjectProcessor::new(
                            package_cache.clone(),
                            &pipeline_config.package_id_filter,
                            metrics.clone(),
                        ),
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::DynamicField => {
                let handler = if let Some(cache) = backfill_cache {
                    DynamicFieldHandler::with_backfill_cache(
                        DynamicFieldProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    DynamicFieldHandler::new(
                        DynamicFieldProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::TransactionObjects => {
                let handler = if let Some(cache) = backfill_cache {
                    TransactionObjectsHandler::with_backfill_cache(
                        TransactionObjectsProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    TransactionObjectsHandler::new(
                        TransactionObjectsProcessor,
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::MovePackage => {
                let handler = if let Some(cache) = backfill_cache {
                    PackageHandler::with_backfill_cache(
                        PackageProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    PackageHandler::new(PackageProcessor, pipeline_config.clone(), metrics)
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::MovePackageBCS => {
                let handler = if let Some(cache) = backfill_cache {
                    PackageBCSHandler::with_backfill_cache(
                        PackageBCSProcessor,
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    PackageBCSHandler::new(PackageBCSProcessor, pipeline_config.clone(), metrics)
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
            Pipeline::WrappedObject => {
                let handler = if let Some(cache) = backfill_cache {
                    WrappedObjectHandler::with_backfill_cache(
                        WrappedObjectProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                        cache,
                    )
                } else {
                    WrappedObjectHandler::new(
                        WrappedObjectProcessor::new(package_cache.clone()),
                        pipeline_config.clone(),
                        metrics,
                    )
                };
                indexer
                    .concurrent_pipeline(handler, concurrent_config)
                    .await?;
            }
        }
        Ok(())
    }
}

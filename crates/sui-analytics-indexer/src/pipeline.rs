// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pipeline definitions for the analytics indexer.

use std::sync::Arc;

use anyhow::Result;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::backfill::BackfillBoundaries;
use crate::config::PipelineConfig;
use crate::handlers::checkpoint_handler::CheckpointProcessor;
use crate::handlers::df_handler::DynamicFieldProcessor;
use crate::handlers::event_handler::EventProcessor;
use crate::handlers::move_call_handler::MoveCallProcessor;
use crate::handlers::object_handler::ObjectProcessor;
use crate::handlers::package_bcs_handler::PackageBCSProcessor;
use crate::handlers::package_handler::PackageProcessor;
use crate::handlers::transaction_bcs_handler::TransactionBCSProcessor;
use crate::handlers::transaction_handler::TransactionProcessor;
use crate::handlers::transaction_objects_handler::TransactionObjectsProcessor;
use crate::handlers::wrapped_object_handler::WrappedObjectProcessor;
use crate::handlers::{AnalyticsHandler, AnalyticsMetadata, BackfillHandler};
use crate::package_store::PackageCache;
use crate::schema::RowSchema;

/// Register a concurrent pipeline with either normal or backfill handler.
///
/// This provides compile-time enforcement that both handler types work for any processor:
/// if a processor satisfies the bounds, BOTH handlers automatically work.
async fn concurrent_pipeline<P>(
    indexer: &mut Indexer<ObjectStore>,
    processor: P,
    config: PipelineConfig,
    metrics: AnalyticsMetrics,
    concurrent_config: ConcurrentConfig,
    backfill_cache: Option<Arc<BackfillBoundaries>>,
) -> Result<()>
where
    P: Processor + Send + Sync,
    P::Value: AnalyticsMetadata + Serialize + RowSchema + Send + Sync,
{
    if let Some(cache) = backfill_cache {
        let handler = BackfillHandler::new(processor, config, metrics, cache);
        indexer
            .concurrent_pipeline(handler, concurrent_config)
            .await?;
    } else {
        let handler = AnalyticsHandler::new(processor, config, metrics);
        indexer
            .concurrent_pipeline(handler, concurrent_config)
            .await?;
    }
    Ok(())
}

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

    /// Registers this pipeline with the indexer.
    pub async fn register(
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
                concurrent_pipeline(
                    indexer,
                    CheckpointProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::Transaction => {
                concurrent_pipeline(
                    indexer,
                    TransactionProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::TransactionBCS => {
                concurrent_pipeline(
                    indexer,
                    TransactionBCSProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::Event => {
                concurrent_pipeline(
                    indexer,
                    EventProcessor::new(package_cache.clone()),
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::MoveCall => {
                concurrent_pipeline(
                    indexer,
                    MoveCallProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::Object => {
                concurrent_pipeline(
                    indexer,
                    ObjectProcessor::new(
                        package_cache.clone(),
                        &pipeline_config.package_id_filter,
                        metrics.clone(),
                    ),
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::DynamicField => {
                concurrent_pipeline(
                    indexer,
                    DynamicFieldProcessor::new(package_cache.clone()),
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::TransactionObjects => {
                concurrent_pipeline(
                    indexer,
                    TransactionObjectsProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::MovePackage => {
                concurrent_pipeline(
                    indexer,
                    PackageProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::MovePackageBCS => {
                concurrent_pipeline(
                    indexer,
                    PackageBCSProcessor,
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
            Pipeline::WrappedObject => {
                concurrent_pipeline(
                    indexer,
                    WrappedObjectProcessor::new(package_cache.clone()),
                    pipeline_config.clone(),
                    metrics,
                    concurrent_config,
                    backfill_cache,
                )
                .await
            }
        }
    }
}

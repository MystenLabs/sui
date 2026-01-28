// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pipeline definitions for the analytics indexer.

use std::sync::Arc;

use anyhow::Result;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use serde::Deserialize;
use serde::Serialize;
use strum_macros::EnumIter;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;

use crate::config::PipelineConfig;
use crate::handlers::AnalyticsHandler;
use crate::handlers::Row;
use crate::handlers::tables::CheckpointProcessor;
use crate::handlers::tables::DynamicFieldProcessor;
use crate::handlers::tables::EventProcessor;
use crate::handlers::tables::MoveCallProcessor;
use crate::handlers::tables::ObjectProcessor;
use crate::handlers::tables::PackageBCSProcessor;
use crate::handlers::tables::PackageProcessor;
use crate::handlers::tables::TransactionBCSProcessor;
use crate::handlers::tables::TransactionObjectsProcessor;
use crate::handlers::tables::TransactionProcessor;
use crate::handlers::tables::WrappedObjectProcessor;
use crate::metrics::Metrics;
use crate::package_store::PackageCache;
use crate::store::AnalyticsStore;

/// Register a sequential pipeline with the analytics handler.
async fn register_sequential_pipeline<P, T>(
    indexer: &mut Indexer<AnalyticsStore>,
    processor: P,
    sequential_config: SequentialConfig,
) -> Result<()>
where
    P: Processor<Value = T> + Send + Sync,
    T: Row + 'static,
{
    indexer.store().register_schema::<P, T>();

    let handler = AnalyticsHandler::new(processor);
    indexer
        .sequential_pipeline(handler, sequential_config)
        .await?;
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
    /// Returns the pipeline name used for watermarks and metrics.
    /// This must match the corresponding `Processor::NAME` constant.
    /// Names match the enum variant names (PascalCase).
    pub const fn name(&self) -> &'static str {
        match self {
            Pipeline::Checkpoint => "Checkpoint",
            Pipeline::Transaction => "Transaction",
            Pipeline::TransactionBCS => "TransactionBCS",
            Pipeline::TransactionObjects => "TransactionObjects",
            Pipeline::Object => "Object",
            Pipeline::Event => "Event",
            Pipeline::MoveCall => "MoveCall",
            Pipeline::MovePackage => "MovePackage",
            Pipeline::MovePackageBCS => "MovePackageBCS",
            Pipeline::DynamicField => "DynamicField",
            Pipeline::WrappedObject => "WrappedObject",
        }
    }

    /// Returns the default output path for this pipeline in the object store.
    /// Used when `output_prefix` is not configured. Uses snake_case for
    /// backwards compatibility with existing data.
    pub const fn default_path(&self) -> &'static str {
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
        indexer: &mut Indexer<AnalyticsStore>,
        pipeline_config: &PipelineConfig,
        package_cache: Arc<PackageCache>,
        metrics: Metrics,
        sequential_config: SequentialConfig,
    ) -> Result<()> {
        match self {
            Pipeline::Checkpoint => {
                register_sequential_pipeline(indexer, CheckpointProcessor, sequential_config).await
            }
            Pipeline::Transaction => {
                register_sequential_pipeline(indexer, TransactionProcessor, sequential_config).await
            }
            Pipeline::TransactionBCS => {
                register_sequential_pipeline(indexer, TransactionBCSProcessor, sequential_config)
                    .await
            }
            Pipeline::Event => {
                register_sequential_pipeline(
                    indexer,
                    EventProcessor::new(package_cache.clone()),
                    sequential_config,
                )
                .await
            }
            Pipeline::MoveCall => {
                register_sequential_pipeline(indexer, MoveCallProcessor, sequential_config).await
            }
            Pipeline::Object => {
                register_sequential_pipeline(
                    indexer,
                    ObjectProcessor::new(
                        package_cache.clone(),
                        &pipeline_config.package_id_filter,
                        metrics,
                    ),
                    sequential_config,
                )
                .await
            }
            Pipeline::DynamicField => {
                register_sequential_pipeline(
                    indexer,
                    DynamicFieldProcessor::new(package_cache.clone()),
                    sequential_config,
                )
                .await
            }
            Pipeline::TransactionObjects => {
                register_sequential_pipeline(
                    indexer,
                    TransactionObjectsProcessor,
                    sequential_config,
                )
                .await
            }
            Pipeline::MovePackage => {
                register_sequential_pipeline(indexer, PackageProcessor, sequential_config).await
            }
            Pipeline::MovePackageBCS => {
                register_sequential_pipeline(indexer, PackageBCSProcessor, sequential_config).await
            }
            Pipeline::WrappedObject => {
                register_sequential_pipeline(
                    indexer,
                    WrappedObjectProcessor::new(package_cache.clone()),
                    sequential_config,
                )
                .await
            }
        }
    }
}

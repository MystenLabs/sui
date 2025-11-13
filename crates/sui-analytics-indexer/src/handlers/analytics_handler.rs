// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use sui_types::base_types::EpochId;

use crate::parquet::ParquetBatch;
use crate::{ParquetSchema, Pipeline, PipelineConfig};

/// Trait for entry types that provide analytics metadata
pub trait AnalyticsMetadata {
    const FILE_TYPE: Pipeline;

    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint_sequence_number(&self) -> u64;
}

/// Generic batch struct that works for all entry types
pub struct AnalyticsBatch<T: AnalyticsMetadata + Serialize + ParquetSchema> {
    pub inner: ParquetBatch<T>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching
pub struct AnalyticsHandler<P, B> {
    processor: P,
    config: PipelineConfig,
    _batch: PhantomData<B>,
}

impl<T: AnalyticsMetadata + Serialize + ParquetSchema + 'static> Default for AnalyticsBatch<T> {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(T::FILE_TYPE.dir_prefix().as_ref().to_string(), 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
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
        let Some(file_bytes) = batch.inner.current_file_bytes() else {
            return Ok(0);
        };

        let row_count = batch.inner.row_count()?;
        let object_path = batch.inner.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.clone().into())
            .await?;

        Ok(row_count)
    }
}

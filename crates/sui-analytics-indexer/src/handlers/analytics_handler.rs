// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_types::base_types::EpochId;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::config::PipelineConfig;
use crate::pipeline::Pipeline;
use crate::schema::RowSchema;

/// Trait for entry types that provide analytics metadata
pub trait AnalyticsMetadata {
    const PIPELINE: Pipeline;

    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint_sequence_number(&self) -> u64;
}

/// Generic batch struct that buffers raw entry rows for later serialization.
/// Serialization is deferred to commit() where errors can be properly handled.
pub struct AnalyticsBatch<T: AnalyticsMetadata + Serialize + RowSchema> {
    /// Buffered rows to be serialized during commit
    rows: Mutex<Vec<T>>,
    pub(crate) dir_prefix: String,
    /// Track the epoch for this batch - used to detect epoch boundaries
    current_epoch: Mutex<Option<EpochId>>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching
pub struct AnalyticsHandler<P, B> {
    processor: P,
    config: PipelineConfig,
    metrics: AnalyticsMetrics,
    _marker: PhantomData<B>,
}

impl<T: AnalyticsMetadata + Serialize + RowSchema + 'static> Default for AnalyticsBatch<T> {
    fn default() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            dir_prefix: T::PIPELINE.dir_prefix().as_ref().to_string(),
            current_epoch: Mutex::new(None),
        }
    }
}

impl<P, B> AnalyticsHandler<P, B> {
    pub fn new(processor: P, config: PipelineConfig, metrics: AnalyticsMetrics) -> Self {
        Self {
            processor,
            config,
            metrics,
            _marker: PhantomData,
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
    P::Value: AnalyticsMetadata + Serialize + RowSchema + Send + Sync,
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
    ) -> BatchStatus {
        // Peek at first value to determine incoming epoch
        let values_slice = values.as_slice();
        if values_slice.is_empty() {
            return BatchStatus::Pending;
        }

        let incoming_epoch = values_slice[0].get_epoch();

        // Check if batch already has data from a different epoch
        {
            let current_epoch_guard = batch.current_epoch.lock().unwrap();
            if let Some(current_batch_epoch) = *current_epoch_guard
                && current_batch_epoch != incoming_epoch
            {
                // Epoch boundary detected - commit current batch before accepting new data
                return BatchStatus::Ready;
            }
        }

        // Set epoch if this is the first data in the batch
        {
            let mut guard = batch.current_epoch.lock().unwrap();
            if guard.is_none() {
                *guard = Some(incoming_epoch);
            }
        }

        batch.rows.lock().unwrap().extend(values.by_ref());

        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        watermarks: &[sui_indexer_alt_framework::pipeline::WatermarkPart],
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        // Take the buffered rows for serialization
        let rows = std::mem::take(&mut *batch.rows.lock().unwrap());
        if rows.is_empty() {
            return Ok(0);
        }

        let row_count = rows.len();

        // Serialize the rows.
        let file_bytes = self
            .config
            .file_format
            .serialize_rows::<P::Value>(rows)?
            .ok_or_else(|| anyhow::anyhow!("No data after serialization"))?;

        // Extract checkpoint range from watermarks (guaranteed to be contiguous)
        let checkpoint_range =
            sui_indexer_alt_framework::pipeline::WatermarkPart::checkpoint_range(watermarks)
                .ok_or_else(|| anyhow::anyhow!("No watermarks provided"))?;

        // Use the tracked epoch from batch (guaranteed to be single epoch due to epoch boundary detection)
        let epoch = batch
            .current_epoch
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("No epoch set for batch"))?;

        let object_path = crate::construct_file_path(
            &batch.dir_prefix,
            epoch,
            checkpoint_range,
            self.config.file_format,
        );

        let object_store_path =
            object_store::path::Path::from(object_path.to_string_lossy().as_ref());

        // Record file size metric before uploading
        let file_size = file_bytes.len() as f64;
        self.metrics
            .file_size_bytes
            .with_label_values(&[P::NAME])
            .observe(file_size);

        conn.object_store()
            .put(&object_store_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}

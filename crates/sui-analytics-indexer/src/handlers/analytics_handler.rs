// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use object_store::PutMode;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;
use tracing::warn;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::backfill::BackfillBoundaries;
use crate::config::{FileFormat, PipelineConfig};
use crate::schema::RowSchema;

/// Entry types implement this to provide epoch and checkpoint information for batching.
/// Batches are committed at epoch boundaries to ensure files don't span epochs.
/// In backfill mode, checkpoint info is used to align with existing file boundaries.
pub trait AnalyticsMetadata {
    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint(&self) -> u64;
}

/// Generic batch struct that buffers raw entry rows for later serialization.
/// Serialization is deferred to commit() where errors can be properly handled.
pub struct AnalyticsBatch<T: AnalyticsMetadata + Serialize + RowSchema> {
    /// Buffered rows to be serialized during commit
    rows: Mutex<Vec<T>>,
    /// Track the epoch for this batch - used to detect epoch boundaries
    epoch: Mutex<Option<EpochId>>,
    /// In backfill mode, the target checkpoint boundary (exclusive end)
    target_checkpoint_end: Mutex<Option<u64>>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching.
///
/// This adapter wraps a `Processor` and provides the common batching and commit
/// logic for writing analytics data to object stores. The batch type is automatically
/// derived as `AnalyticsBatch<P::Value>`.
pub struct AnalyticsHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: AnalyticsMetrics,
    /// In backfill mode, targets to match for file boundaries
    backfill_targets: Option<Arc<BackfillBoundaries>>,
}

impl<T: AnalyticsMetadata + Serialize + RowSchema + 'static> Default for AnalyticsBatch<T> {
    fn default() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            epoch: Mutex::new(None),
            target_checkpoint_end: Mutex::new(None),
        }
    }
}

impl<P> AnalyticsHandler<P> {
    pub fn new(processor: P, config: PipelineConfig, metrics: AnalyticsMetrics) -> Self {
        Self {
            processor,
            config,
            metrics,
            backfill_targets: None,
        }
    }

    /// Create a new handler with backfill targets for matching existing file boundaries.
    pub fn with_backfill_targets(
        processor: P,
        config: PipelineConfig,
        metrics: AnalyticsMetrics,
        backfill_targets: Arc<BackfillBoundaries>,
    ) -> Self {
        Self {
            processor,
            config,
            metrics,
            backfill_targets: Some(backfill_targets),
        }
    }
}

#[async_trait]
impl<P> Processor for AnalyticsHandler<P>
where
    P: Processor + Send + Sync,
    P::Value: Send + Sync,
{
    const NAME: &'static str = P::NAME;
    const FANOUT: usize = P::FANOUT;
    type Value = P::Value;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        self.processor.process(checkpoint).await
    }
}

#[async_trait]
impl<P> concurrent::Handler for AnalyticsHandler<P>
where
    P: Processor + Send + Sync,
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
        // Peek at first value to determine incoming epoch and checkpoint
        let values_slice = values.as_slice();
        if values_slice.is_empty() {
            return BatchStatus::Pending;
        }

        let incoming_epoch = values_slice[0].get_epoch();
        let incoming_checkpoint = values_slice[0].get_checkpoint();

        // Check if batch already has data from a different epoch
        {
            let current_epoch_guard = batch.epoch.lock().unwrap();
            if let Some(current_batch_epoch) = *current_epoch_guard
                && current_batch_epoch != incoming_epoch
            {
                // Epoch boundary detected - commit current batch before accepting new data
                return BatchStatus::Ready;
            }
        }

        // In backfill mode, check if we've reached a target boundary before accepting new data
        {
            let target_end = batch.target_checkpoint_end.lock().unwrap();
            if let Some(end) = *target_end {
                if incoming_checkpoint >= end {
                    return BatchStatus::Ready;
                }
            }
        }

        // Set epoch if this is the first data in the batch
        {
            let mut guard = batch.epoch.lock().unwrap();
            if guard.is_none() {
                *guard = Some(incoming_epoch);
            }
        }

        // In backfill mode, set target boundary from the first checkpoint we see
        if let Some(ref targets) = self.backfill_targets {
            let mut target_end = batch.target_checkpoint_end.lock().unwrap();
            if target_end.is_none() {
                if let Some(target) = targets.find_target(incoming_epoch, incoming_checkpoint) {
                    *target_end = Some(target.checkpoint_range.end);
                }
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
            .epoch
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("No epoch set for batch"))?;

        let object_path = construct_file_path(
            self.config.dir_prefix(),
            epoch,
            checkpoint_range.clone(),
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

        // In backfill mode, validate target and use conditional PUT
        if let Some(ref targets) = self.backfill_targets {
            // Verify target exists for this range
            let target = targets.get(epoch, checkpoint_range.start).ok_or_else(|| {
                anyhow::anyhow!(
                    "No target file for epoch {} range {:?}. \
                     Backfill can only update existing files.",
                    epoch,
                    checkpoint_range
                )
            })?;

            // Verify range matches exactly
            if target.checkpoint_range != checkpoint_range {
                return Err(anyhow::anyhow!(
                    "Range mismatch: target {:?}, got {:?}. \
                     Batch boundaries must align with existing files.",
                    target.checkpoint_range,
                    checkpoint_range
                ));
            }

            // Get current e_tag (may have been refreshed on previous retry)
            let (e_tag, version) = targets.get_etag(epoch, checkpoint_range.start);

            // Use conditional update
            let result = conn
                .object_store()
                .put_opts(
                    &object_store_path,
                    file_bytes.into(),
                    PutMode::Update(object_store::UpdateVersion { e_tag, version }).into(),
                )
                .await;

            match result {
                Ok(_) => Ok(row_count),
                Err(e) => {
                    // On conflict, refresh e_tag for next retry
                    if let Err(refresh_err) = targets
                        .refresh_etag(conn.object_store().as_ref(), epoch, checkpoint_range.start)
                        .await
                    {
                        warn!(
                            pipeline = P::NAME,
                            epoch,
                            start = checkpoint_range.start,
                            "Failed to refresh e_tag: {}",
                            refresh_err
                        );
                    }
                    Err(anyhow::anyhow!(
                        "Conditional update failed for {:?}: {}. Will retry.",
                        object_store_path,
                        e
                    ))
                }
            }
        } else {
            // Normal mode: unconditional put
            conn.object_store()
                .put(&object_store_path, file_bytes.into())
                .await?;
            Ok(row_count)
        }
    }
}

fn construct_file_path(
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

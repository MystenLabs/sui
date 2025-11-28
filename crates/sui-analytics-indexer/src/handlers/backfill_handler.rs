// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill handler for re-processing analytics data with existing file boundaries.
//!
//! This handler is used when `backfill_mode` is enabled. Unlike the normal
//! `AnalyticsHandler`, it:
//! - Pre-loads epoch boundaries from existing files before processing
//! - Forces batch boundaries to align with existing file ranges
//! - Uses conditional PUT operations (e_tag) to detect concurrent modifications
//! - Prunes old epochs from cache as processing advances

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use object_store::PutMode;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_types::full_checkpoint_content::Checkpoint;
use tracing::warn;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::backfill::{BackfillBoundaries, EpochBoundaries};
use crate::config::PipelineConfig;
use crate::handlers::analytics_handler::{AnalyticsBatch, AnalyticsMetadata};
use crate::handlers::{construct_file_path, record_file_metrics};
use crate::schema::RowSchema;

/// Backfill-specific batch that wraps AnalyticsBatch with additional boundary tracking.
pub struct BackfillBatch<T: AnalyticsMetadata + Serialize + RowSchema> {
    /// Inner batch for rows and epoch tracking
    inner: AnalyticsBatch<T>,
    /// First checkpoint in this batch - used to look up target file boundary
    first_checkpoint: Mutex<Option<u64>>,
    /// Holds the loaded epoch boundaries - keeps data alive while batch is in-flight
    epoch_boundaries: Mutex<Option<Arc<EpochBoundaries>>>,
}

/// Handler for backfill mode - aligns batches with existing file boundaries.
pub struct BackfillHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: AnalyticsMetrics,
    /// Lazy-loading cache for file boundaries
    backfill_cache: Arc<BackfillBoundaries>,
}

impl<T: AnalyticsMetadata + Serialize + RowSchema + 'static> Default for BackfillBatch<T> {
    fn default() -> Self {
        Self {
            inner: AnalyticsBatch::default(),
            first_checkpoint: Mutex::new(None),
            epoch_boundaries: Mutex::new(None),
        }
    }
}

impl<P> BackfillHandler<P> {
    pub fn new(
        processor: P,
        config: PipelineConfig,
        metrics: AnalyticsMetrics,
        backfill_cache: Arc<BackfillBoundaries>,
    ) -> Self {
        Self {
            processor,
            config,
            metrics,
            backfill_cache,
        }
    }
}

#[async_trait]
impl<P> Processor for BackfillHandler<P>
where
    P: Processor + Send + Sync,
    P::Value: Send + Sync,
{
    const NAME: &'static str = P::NAME;
    const FANOUT: usize = P::FANOUT;
    type Value = P::Value;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        // Pre-load epoch boundaries before processing.
        // This ensures the epoch is in cache before batch() is called (which is sync).
        self.backfill_cache
            .ensure_epoch_loaded(checkpoint.summary.epoch)
            .await?;

        self.processor.process(checkpoint).await
    }
}

#[async_trait]
impl<P> concurrent::Handler for BackfillHandler<P>
where
    P: Processor + Send + Sync,
    P::Value: AnalyticsMetadata + Serialize + RowSchema + Send + Sync,
{
    type Store = sui_indexer_alt_object_store::ObjectStore;
    type Batch = BackfillBatch<P::Value>;

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
        let values_slice = values.as_slice();
        if values_slice.is_empty() {
            return BatchStatus::Pending;
        }

        let incoming_epoch = values_slice[0].get_epoch();
        let incoming_checkpoint = values_slice[0].get_checkpoint();

        // Check if batch already has data from a different epoch
        {
            let current_epoch_guard = batch.inner.epoch.lock().unwrap();
            if let Some(current_batch_epoch) = *current_epoch_guard
                && current_batch_epoch != incoming_epoch
            {
                // Epoch boundary detected - commit current batch before accepting new data
                // Prune old epochs from cache
                self.backfill_cache.prune_epochs_before(incoming_epoch);
                return BatchStatus::Ready;
            }
        }

        // Check if we've reached a target file boundary by looking up from epoch_boundaries
        {
            let epoch_bounds = batch.epoch_boundaries.lock().unwrap();
            let first_cp = batch.first_checkpoint.lock().unwrap();
            if let (Some(bounds), Some(start_cp)) = (epoch_bounds.as_ref(), *first_cp)
                && let Some(target) = bounds.find_target(start_cp)
                && incoming_checkpoint >= target.checkpoint_range.end
            {
                return BatchStatus::Ready;
            }
        }

        // Set epoch if this is the first data in the batch
        {
            let mut guard = batch.inner.epoch.lock().unwrap();
            if guard.is_none() {
                *guard = Some(incoming_epoch);
            }
        }

        // Clone epoch boundaries Arc into batch and set first checkpoint
        {
            let mut epoch_bounds_guard = batch.epoch_boundaries.lock().unwrap();
            if epoch_bounds_guard.is_none() {
                // Get the pre-loaded epoch boundaries from cache
                // (guaranteed to be loaded by process())
                if let Some(epoch_bounds) = self.backfill_cache.get_epoch(incoming_epoch) {
                    *batch.first_checkpoint.lock().unwrap() = Some(incoming_checkpoint);
                    *epoch_bounds_guard = Some(epoch_bounds);
                }
            }
        }

        batch.inner.rows.lock().unwrap().extend(values.by_ref());

        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        watermarks: &[sui_indexer_alt_framework::pipeline::WatermarkPart],
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        let rows = std::mem::take(&mut *batch.inner.rows.lock().unwrap());

        if rows.is_empty() {
            return Ok(0);
        }

        let row_count = rows.len();

        let file_bytes = self
            .config
            .file_format
            .serialize_rows::<P::Value>(rows)?
            .ok_or_else(|| anyhow::anyhow!("No data after serialization"))?;

        let checkpoint_range =
            sui_indexer_alt_framework::pipeline::WatermarkPart::checkpoint_range(watermarks)
                .ok_or_else(|| anyhow::anyhow!("No watermarks provided"))?;

        let epoch = batch
            .inner
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

        record_file_metrics(&self.metrics, P::NAME, file_bytes.len());

        // Validate target and use conditional PUT
        let epoch_bounds = batch
            .epoch_boundaries
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No epoch boundaries loaded for epoch {}. This should not happen in backfill mode.",
                    epoch
                )
            })?;

        // Verify target exists for this range
        let target = epoch_bounds.get(checkpoint_range.start).ok_or_else(|| {
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
        let (e_tag, version) = epoch_bounds.get_etag(checkpoint_range.start);

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
                if let Err(refresh_err) = epoch_bounds
                    .refresh_etag(conn.object_store().as_ref(), checkpoint_range.start)
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
    }
}

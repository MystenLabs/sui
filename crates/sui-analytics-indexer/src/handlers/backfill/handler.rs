// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill handler for re-processing analytics data with existing file boundaries.
//!
//! This handler is used when `backfill_mode` is enabled. Unlike the normal
//! `AnalyticsHandler`, it:
//! - Forces batch boundaries to align with existing file ranges
//! - Uses conditional PUT operations (e_tag) to detect concurrent modifications

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use object_store::PutMode;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_types::full_checkpoint_content::Checkpoint;
use tracing::warn;

use crate::config::PipelineConfig;
use crate::handlers::handler::AnalyticsMetadata;
use crate::handlers::{construct_file_path, record_file_metrics};
use crate::metrics::Metrics;
use crate::schema::RowSchema;

use super::boundaries::BackfillBoundaries;

/// Backfill-specific batch - tracks rows and target file key.
pub struct BackfillBatch<T> {
    rows: Mutex<Vec<T>>,
    /// Target file key (epoch, start_checkpoint) - set on first value
    target_key: Mutex<Option<(u64, u64)>>,
}

/// Handler for backfill mode - aligns batches with existing file boundaries.
pub struct BackfillHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: Metrics,
    /// Pre-loaded boundaries for file alignment
    boundaries: Arc<BackfillBoundaries>,
}

impl<T> Default for BackfillBatch<T> {
    fn default() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            target_key: Mutex::new(None),
        }
    }
}

impl<P> BackfillHandler<P> {
    pub fn new(
        processor: P,
        config: PipelineConfig,
        metrics: Metrics,
        boundaries: Arc<BackfillBoundaries>,
    ) -> Self {
        Self {
            processor,
            config,
            metrics,
            boundaries,
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
        assert!(!values_slice.is_empty(), "batch() called with empty values");

        let incoming_checkpoint = values_slice[0].get_checkpoint();

        let mut target_key = batch.target_key.lock().unwrap();
        match *target_key {
            Some((epoch, start)) => {
                // Check if we've reached the target file boundary
                let target = self.boundaries.get_target(epoch, start)
                    .expect("target_key set but target not found");
                if incoming_checkpoint >= target.checkpoint_range.end {
                    return BatchStatus::Ready;
                }
            }
            None => {
                // First value - find and store the target
                let target = self.boundaries.find_target(incoming_checkpoint)
                    .unwrap_or_else(|| panic!(
                        "No target file for checkpoint {}. Backfill requires existing files.",
                        incoming_checkpoint
                    ));
                *target_key = Some((target.epoch, target.checkpoint_range.start));
            }
        }
        drop(target_key);

        batch.rows.lock().unwrap().extend(values.by_ref());

        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        watermarks: &[sui_indexer_alt_framework::pipeline::WatermarkPart],
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        let rows = std::mem::take(&mut *batch.rows.lock().unwrap());

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

        // Get target using the key we stored in batch()
        let (epoch, start) = batch.target_key.lock().unwrap()
            .ok_or_else(|| anyhow::anyhow!("No target_key set - batch() was never called?"))?;

        let target = self.boundaries.get_target(epoch, start)
            .expect("target_key set but target not found");

        // Verify range matches exactly
        if target.checkpoint_range != checkpoint_range {
            return Err(anyhow::anyhow!(
                "Range mismatch: target {:?}, got {:?}. \
                 Batch boundaries must align with existing files.",
                target.checkpoint_range,
                checkpoint_range
            ));
        }

        let object_path = construct_file_path(
            self.config.dir_prefix(),
            target.epoch,
            checkpoint_range.clone(),
            self.config.file_format,
        );

        let object_store_path =
            object_store::path::Path::from(object_path.to_string_lossy().as_ref());

        record_file_metrics(&self.metrics, P::NAME, file_bytes.len());

        // Get current e_tag (may have been refreshed on previous retry)
        let (e_tag, version) = target.get_etag();

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
                if let Err(refresh_err) = target.refresh_etag(conn.object_store().as_ref()).await {
                    warn!(
                        pipeline = P::NAME,
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

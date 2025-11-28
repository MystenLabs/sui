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
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_indexer_alt_framework::pipeline::{Processor, WatermarkPart};
use sui_indexer_alt_framework::store::Store;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::config::PipelineConfig;
use crate::handlers::handler::AnalyticsMetadata;
use crate::handlers::{construct_object_store_path, record_file_metrics};
use crate::metrics::Metrics;
use crate::schema::RowSchema;

use super::boundaries::BackfillTargets;

/// Backfill-specific batch - tracks rows and target file boundaries.
pub struct BackfillBatch<T> {
    rows: Mutex<Vec<T>>,
    /// Target file checkpoint range (start, end) - set on first value
    target_range: Mutex<Option<(u64, u64)>>,
}

/// Handler for backfill mode - aligns batches with existing file boundaries.
pub struct BackfillHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: Metrics,
    targets: Arc<BackfillTargets>,
}

impl<T> Default for BackfillBatch<T> {
    fn default() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            target_range: Mutex::new(None),
        }
    }
}

impl<P> BackfillHandler<P> {
    pub fn new(
        processor: P,
        config: PipelineConfig,
        metrics: Metrics,
        targets: Arc<BackfillTargets>,
    ) -> Self {
        Self {
            processor,
            config,
            metrics,
            targets,
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
    P::Value: AnalyticsMetadata + Serialize + RowSchema + Clone + Send + Sync,
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
        let checkpoint_seq_num = values_slice[0].get_checkpoint();
        {
            let mut target_range = batch.target_range.lock().unwrap();
            match *target_range {
                Some((start, end)) => {
                    // Check if we've reached the file boundary (end is exclusive)
                    if checkpoint_seq_num == end {
                        return BatchStatus::Ready;
                    } else if checkpoint_seq_num > end {
                        panic!(
                            "Checkpoint {} is past target range {}..{}",
                            checkpoint_seq_num, start, end
                        );
                    }
                }
                None => {
                    // First value - look up target by start checkpoint
                    let target = self.targets.get(&checkpoint_seq_num).unwrap_or_else(|| {
                        panic!(
                            "No target file for checkpoint {}. Backfill requires existing files.",
                            checkpoint_seq_num
                        )
                    });
                    *target_range =
                        Some((target.checkpoint_range.start, target.checkpoint_range.end));
                }
            }
        }

        batch.rows.lock().unwrap().extend(values.by_ref());

        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        watermarks: &[WatermarkPart],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let rows = batch.rows.lock().unwrap().clone();
        assert!(!rows.is_empty(), "commit() called with empty batch");

        let row_count = rows.len();

        let file_bytes = self
            .config
            .file_format
            .serialize_rows::<P::Value>(rows)?
            .ok_or_else(|| anyhow::anyhow!("No data after serialization"))?;

        let checkpoint_range = WatermarkPart::checkpoint_range(watermarks)
            .ok_or_else(|| anyhow::anyhow!("No watermarks provided"))?;

        let (start, _end) =
            batch.target_range.lock().unwrap().ok_or_else(|| {
                anyhow::anyhow!("No target_range set - batch() was never called?")
            })?;

        let target = self
            .targets
            .get(&start)
            .expect("target_range set but target not found");

        // Verify range matches exactly
        if target.checkpoint_range != checkpoint_range {
            return Err(anyhow::anyhow!(
                "Range mismatch: target {:?}, got {:?}. \
                 Batch boundaries must align with existing files.",
                target.checkpoint_range,
                checkpoint_range
            ));
        }

        let object_store_path = construct_object_store_path(
            self.config.dir_prefix(),
            target.epoch,
            checkpoint_range.clone(),
            self.config.file_format,
        );

        record_file_metrics(&self.metrics, P::NAME, file_bytes.len());

        let result = conn
            .object_store()
            .put_opts(
                &object_store_path,
                file_bytes.into(),
                PutMode::Update(object_store::UpdateVersion {
                    e_tag: target.e_tag.clone(),
                    version: target.version.clone(),
                })
                .into(),
            )
            .await;

        result.map_err(|e| {
            anyhow::anyhow!(
                "Conditional update failed for {:?}: {}. \
                 File was modified during backfill.",
                object_store_path,
                e
            )
        })?;

        Ok(row_count)
    }
}

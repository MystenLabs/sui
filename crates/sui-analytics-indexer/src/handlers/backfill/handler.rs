// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backfill handler for re-processing analytics data while preserving file boundaries.
//!
//! Use backfill mode to regenerate files (e.g., schema changes, bug fixes) without
//! changing checkpoint ranges. Requires `backfill_mode: true`, `task_name` (for watermark
//! isolation), `last_checkpoint`, and exactly one pipeline.
//!
//! On startup, lists existing files, parses `{epoch}/{start}_{end}.{format}` names into
//! [`BackfillTargets`], and sets `first_checkpoint` to the minimum. Batches are committed
//! only at file boundaries, not by row count. This ensures the new file atomically replaces
//! the old one with identical checkpoint coverage - no overlap or gaps with neighboring files.
//! Uses conditional PUT (e_tag/version) to detect concurrent modifications - fails loudly
//! rather than retrying (single-writer assumed).

use std::ops::Range;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use object_store::PutMode;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_indexer_alt_framework::store::Store;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::config::PipelineConfig;
use crate::handlers::handler::Row;
use crate::handlers::{construct_object_store_path, record_file_metrics};
use crate::metrics::Metrics;
use crate::schema::RowSchema;

use super::metadata::BackfillTargets;

/// Backfill-specific batch - tracks rows and target file boundaries.
pub struct Batch<T> {
    inner: Mutex<BatchInner<T>>,
}

struct BatchInner<T> {
    rows: Vec<T>,
    /// Target file checkpoint range [start, end) - set on first value
    target_range: Option<Range<u64>>,
}

/// Handler for backfill mode - aligns batches with existing file boundaries.
pub struct BackfillHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: Metrics,
    targets: BackfillTargets,
}

impl<T> Default for Batch<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(BatchInner {
                rows: Vec::new(),
                target_range: None,
            }),
        }
    }
}

impl<P> BackfillHandler<P> {
    pub fn new(
        processor: P,
        config: PipelineConfig,
        metrics: Metrics,
        targets: BackfillTargets,
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
    P::Value: Row + Serialize + RowSchema + Clone + Send + Sync,
{
    type Store = sui_indexer_alt_object_store::ObjectStore;
    type Batch = Batch<P::Value>;

    // Backfill batches are bounded by file boundaries, not row/checkpoint counts.
    // Disable framework batch-cutting so we control boundaries via batch() returning BatchStatus::Ready.
    const MAX_WATERMARK_UPDATES: usize = usize::MAX;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        let values_slice = values.as_slice();
        if values_slice.is_empty() {
            return BatchStatus::Pending;
        }
        let checkpoint_seq_num = values_slice[0].get_checkpoint();
        let inner = batch.inner.get_mut().unwrap();

        match &inner.target_range {
            Some(range) => {
                // Check if we've reached the file boundary (end is exclusive)
                if checkpoint_seq_num == range.end {
                    return BatchStatus::Ready;
                } else if checkpoint_seq_num > range.end {
                    panic!(
                        "Checkpoint {} is past target range {:?}",
                        checkpoint_seq_num, range
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
                inner.target_range = Some(target.checkpoint_range.clone());
            }
        }

        inner.rows.extend(values.by_ref());

        // Return NotReady to signal "don't commit until we hit the file boundary".
        // This causes the collector to carry over the batch to subsequent ticks
        // until Ready is returned (when we reach the target range end).
        BatchStatus::NotReady
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let (file_bytes, row_count, target_range, checkpoint_range) = {
            let inner = batch.inner.lock().unwrap();
            assert!(!inner.rows.is_empty(), "commit() called with empty batch");
            let file_bytes = self
                .config
                .file_format
                .serialize_rows::<P::Value>(&inner.rows)?
                .ok_or_else(|| anyhow::anyhow!("No data after serialization"))?;
            let first_checkpoint = inner.rows.first().unwrap().get_checkpoint();
            let last_checkpoint = inner.rows.last().unwrap().get_checkpoint();
            (
                file_bytes,
                inner.rows.len(),
                inner.target_range.clone(),
                first_checkpoint..last_checkpoint + 1,
            )
        };

        let target_range = target_range
            .ok_or_else(|| anyhow::anyhow!("No target_range set - batch() was never called?"))?;

        let target = self
            .targets
            .get(&target_range.start)
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
            P::NAME,
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

        // We intentionally do not refresh the e_tag/version and retry on conflict.
        // Backfill assumes single-writer with no concurrent modifications.
        // If this assumption is violated, we want to fail loudly rather than silently
        // overwrite changes or enter a retry loop.
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

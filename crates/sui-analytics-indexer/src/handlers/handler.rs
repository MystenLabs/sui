// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{self, BatchStatus};
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::config::PipelineConfig;
use crate::handlers::{construct_object_store_path, record_file_metrics};
use crate::metrics::Metrics;
use crate::schema::RowSchema;

/// Row types implement this to provide epoch and checkpoint information for batching.
/// Batches are committed at epoch boundaries to ensure files don't span epochs.
pub trait Row {
    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint(&self) -> u64;
}

/// Generic batch struct that buffers row objects for later serialization.
/// Serialization is deferred to commit() where errors can be properly handled.
pub struct Batch<T> {
    inner: Mutex<BatchInner<T>>,
}

pub(super) struct BatchInner<T> {
    /// Buffered rows to be serialized during commit
    pub(super) rows: Vec<T>,
    /// Track the epoch for this batch - used to detect epoch boundaries
    pub(super) epoch: Option<EpochId>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching.
///
/// This adapter wraps a `Processor` and provides the common batching and commit
/// logic for writing analytics data to object stores. The batch type is automatically
/// derived as `AnalyticsBatch<P::Value>`.
pub struct AnalyticsHandler<P> {
    processor: P,
    config: PipelineConfig,
    metrics: Metrics,
}

impl<T> Default for Batch<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(BatchInner {
                rows: Vec::new(),
                epoch: None,
            }),
        }
    }
}

impl<P> AnalyticsHandler<P> {
    pub fn new(processor: P, config: PipelineConfig, metrics: Metrics) -> Self {
        Self {
            processor,
            config,
            metrics,
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
    P::Value: Row + Serialize + RowSchema + Clone + Send + Sync,
{
    type Store = sui_indexer_alt_object_store::ObjectStore;
    type Batch = Batch<P::Value>;

    // Disable framework batch-cutting: we control file boundaries via batch() returning BatchStatus::Ready
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

        let incoming_epoch = values_slice[0].get_epoch();
        let inner = batch.inner.get_mut().unwrap();

        // Check if batch already has data from a different epoch
        if let Some(epoch) = inner.epoch {
            if epoch != incoming_epoch {
                // Epoch boundary detected - commit current batch before accepting new data
                return BatchStatus::Ready;
            }
        } else {
            inner.epoch = Some(incoming_epoch);
        }

        inner.rows.extend(values.by_ref());

        // Signal ready when we've reached max_rows_per_file to split files by row count
        if inner.rows.len() >= self.config.max_rows_per_file {
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        let (file_bytes, row_count, epoch, checkpoint_range) = {
            let inner = batch.inner.lock().unwrap();
            assert!(!inner.rows.is_empty(), "commit() called with empty batch");
            let file_bytes = self
                .config
                .file_format
                .serialize_rows::<P::Value>(&inner.rows)?
                .expect("non-empty rows should serialize to bytes");
            let first_checkpoint = inner.rows.first().unwrap().get_checkpoint();
            let last_checkpoint = inner.rows.last().unwrap().get_checkpoint();
            (
                file_bytes,
                inner.rows.len(),
                inner.epoch.expect("non-empty batch should have epoch"),
                first_checkpoint..last_checkpoint + 1,
            )
        };

        let object_store_path = construct_object_store_path(
            P::NAME,
            epoch,
            checkpoint_range.clone(),
            self.config.file_format,
        );

        record_file_metrics(&self.metrics, P::NAME, file_bytes.len());

        conn.object_store()
            .put(&object_store_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}

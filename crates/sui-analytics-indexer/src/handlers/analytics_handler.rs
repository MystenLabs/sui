// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::marker::PhantomData;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;
use sui_types::base_types::EpochId;

use crate::csv::CsvWriter;
use crate::parquet::ParquetWriter;
use crate::{FileFormat, ParquetSchema, Pipeline, PipelineConfig};

/// Trait for entry types that provide analytics metadata
pub trait AnalyticsMetadata {
    const PIPELINE: Pipeline;

    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint_sequence_number(&self) -> u64;
}

/// Enum to hold either CSV or Parquet writer
enum WriterVariant {
    Csv(CsvWriter),
    Parquet(ParquetWriter),
}

impl WriterVariant {
    fn write<S: Serialize + ParquetSchema>(
        &mut self,
        rows: Box<dyn Iterator<Item = S> + Send + Sync>,
    ) -> Result<()> {
        match self {
            WriterVariant::Csv(w) => w.write(rows),
            WriterVariant::Parquet(w) => w.write(rows),
        }
    }

    fn flush<S: Serialize + ParquetSchema>(&mut self) -> Result<Option<Vec<u8>>> {
        match self {
            WriterVariant::Csv(w) => w.flush::<S>(),
            WriterVariant::Parquet(w) => w.flush::<S>(),
        }
    }

    fn rows(&self) -> Result<usize> {
        match self {
            WriterVariant::Csv(w) => w.rows(),
            WriterVariant::Parquet(w) => w.rows(),
        }
    }
}

/// Batching mode for analytics handler
#[derive(Clone, Default)]
pub enum BatchingMode {
    /// Live mode - normal forward processing (current behavior)
    #[default]
    Live,
    /// Backfill mode - match existing checkpoint ranges
    Backfill {
        target_ranges: Arc<Vec<(Range<u64>, String)>>,
        current_range_idx: Arc<Mutex<usize>>,
    },
}

/// Generic batch struct that works for all entry types
pub struct AnalyticsBatch<T: AnalyticsMetadata + Serialize + ParquetSchema> {
    inner: Mutex<Option<WriterVariant>>,
    pub(crate) dir_prefix: String,
    current_file_bytes: Mutex<Option<Bytes>>,
    batching_mode: Arc<Mutex<Option<BatchingMode>>>,
    _phantom: PhantomData<T>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching
pub struct AnalyticsHandler<P, B> {
    processor: P,
    config: PipelineConfig,
    batching_mode: Arc<Mutex<Option<BatchingMode>>>,
    _batch: PhantomData<B>,
}

impl<T: AnalyticsMetadata + Serialize + ParquetSchema + 'static> Default for AnalyticsBatch<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            dir_prefix: T::PIPELINE.dir_prefix().as_ref().to_string(),
            current_file_bytes: Mutex::new(None),
            batching_mode: Arc::new(Mutex::new(None)),
            _phantom: PhantomData,
        }
    }
}

impl<T: AnalyticsMetadata + Serialize + ParquetSchema> AnalyticsBatch<T> {
    /// Write rows to the batch (initializes writer on first call)
    fn write_rows<I>(&self, rows: I, format: FileFormat) -> Result<()>
    where
        I: Iterator<Item = T>,
        T: Send + Sync + 'static,
    {
        let mut inner = self.inner.lock().unwrap();

        let writer = match inner.as_mut() {
            Some(w) => w,
            None => {
                let w = match format {
                    FileFormat::Csv => WriterVariant::Csv(CsvWriter::new()?),
                    FileFormat::Parquet => WriterVariant::Parquet(ParquetWriter::new()?),
                };
                inner.insert(w)
            }
        };

        let collected: Vec<T> = rows.collect();
        writer.write(Box::new(collected.into_iter()))?;
        Ok(())
    }

    /// Get the current file bytes if available (cloned to avoid holding the lock)
    fn current_file_bytes(&self) -> Option<Bytes> {
        self.current_file_bytes.lock().unwrap().clone()
    }

    /// Get the row count
    fn row_count(&self) -> Result<usize> {
        let inner = self.inner.lock().unwrap();
        match &*inner {
            Some(writer) => writer.rows(),
            None => Ok(0),
        }
    }

    /// Flush the current batch and store the bytes
    fn flush(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref mut writer) = *inner
            && let Some(bytes) = writer.flush::<T>()?
        {
            *self.current_file_bytes.lock().unwrap() = Some(Bytes::from(bytes));
        }
        Ok(())
    }
}

impl<P, B> AnalyticsHandler<P, B> {
    pub fn new(processor: P, config: PipelineConfig) -> Self {
        Self {
            processor,
            config,
            batching_mode: Arc::new(Mutex::new(Some(BatchingMode::Live))),
            _batch: PhantomData,
        }
    }

    pub fn new_backfill(
        processor: P,
        config: PipelineConfig,
        target_ranges: Vec<(Range<u64>, String)>,
    ) -> Self {
        let batching_mode = BatchingMode::Backfill {
            target_ranges: Arc::new(target_ranges),
            current_range_idx: Arc::new(Mutex::new(0)),
        };

        Self {
            processor,
            config,
            batching_mode: Arc::new(Mutex::new(Some(batching_mode))),
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
        // Initialize batch with handler's batching mode if not set
        {
            let mut batch_mode = batch.batching_mode.lock().unwrap();
            if batch_mode.is_none() {
                *batch_mode = self.batching_mode.lock().unwrap().clone();
            }
        }

        let batch_mode = batch
            .batching_mode
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(BatchingMode::Live);
        match &batch_mode {
            BatchingMode::Live => {
                let Some(first) = values.next() else {
                    return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
                };

                // Write all rows to batch
                if let Err(e) = batch.write_rows(
                    std::iter::once(first).chain(values.by_ref()),
                    self.config.file_format,
                ) {
                    tracing::error!("Failed to write rows to batch: {}", e);
                    return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
                }

                sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending
            }
            BatchingMode::Backfill {
                target_ranges,
                current_range_idx,
            } => {
                let current_idx = *current_range_idx.lock().unwrap();

                if current_idx >= target_ranges.len() {
                    return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
                }

                let target = &target_ranges[current_idx];
                let target_end = target.0.end;

                let mut rows_to_write = Vec::new();
                for value in values.by_ref() {
                    let checkpoint = value.get_checkpoint_sequence_number();

                    if checkpoint >= target_end {
                        // Reached target boundary - force batch commit
                        if !rows_to_write.is_empty() {
                            if let Err(e) =
                                batch.write_rows(rows_to_write.into_iter(), self.config.file_format)
                            {
                                tracing::error!("Failed to write rows to batch: {}", e);
                            }
                        }
                        return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Ready;
                    }

                    rows_to_write.push(value);
                }

                // Write accumulated rows
                if !rows_to_write.is_empty() {
                    if let Err(e) =
                        batch.write_rows(rows_to_write.into_iter(), self.config.file_format)
                    {
                        tracing::error!("Failed to write rows to batch: {}", e);
                    }
                }

                sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending
            }
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        watermarks: &[sui_indexer_alt_framework::pipeline::WatermarkPart],
        conn: &mut <Self::Store as sui_indexer_alt_framework::store::Store>::Connection<'a>,
    ) -> Result<usize> {
        // Ensure the batch is flushed before committing
        batch.flush()?;

        let Some(file_bytes) = batch.current_file_bytes() else {
            return Ok(0);
        };

        let row_count = batch.row_count()?;

        // Determine file path based on batching mode
        let batch_mode = batch
            .batching_mode
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(BatchingMode::Live);
        let file_path = match &batch_mode {
            BatchingMode::Live => {
                // Extract checkpoint range from watermarks (guaranteed to be contiguous)
                let checkpoint_range =
                    sui_indexer_alt_framework::pipeline::WatermarkPart::checkpoint_range(
                        watermarks,
                    )
                    .ok_or_else(|| anyhow::anyhow!("No watermarks provided"))?;

                let epoch = watermarks
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("No watermarks provided"))?
                    .watermark
                    .epoch_hi_inclusive;

                let object_path = crate::construct_file_path(
                    &batch.dir_prefix,
                    epoch,
                    checkpoint_range,
                    self.config.file_format,
                );

                object_path.to_string_lossy().to_string()
            }
            BatchingMode::Backfill {
                target_ranges,
                current_range_idx,
            } => {
                // Use exact path from target ranges
                let current_idx = *current_range_idx.lock().unwrap();
                if current_idx >= target_ranges.len() {
                    anyhow::bail!("Current range index out of bounds");
                }
                target_ranges[current_idx].1.clone()
            }
        };

        let object_store_path = object_store::path::Path::from(file_path.as_str());

        conn.object_store()
            .put(&object_store_path, file_bytes.into())
            .await?;

        // Increment range index in backfill mode after successful commit
        if let Some(BatchingMode::Backfill {
            current_range_idx, ..
        }) = batch.batching_mode.lock().unwrap().as_ref()
        {
            let mut idx = current_range_idx.lock().unwrap();
            *idx += 1;
        }

        Ok(row_count)
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;
use sui_types::base_types::EpochId;

use crate::config::{FileFormat, PipelineConfig};
use crate::pipeline::Pipeline;
use crate::schema::RowSchema;
use crate::writers::{CsvWriter, ParquetWriter};

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
    fn write<S: Serialize + RowSchema>(
        &mut self,
        rows: Box<dyn Iterator<Item = S> + Send + Sync>,
    ) -> Result<()> {
        match self {
            WriterVariant::Csv(w) => w.write(rows),
            WriterVariant::Parquet(w) => w.write(rows),
        }
    }

    fn flush<S: Serialize + RowSchema>(&mut self) -> Result<Option<Vec<u8>>> {
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

/// Generic batch struct that works for all entry types
pub struct AnalyticsBatch<T: AnalyticsMetadata + Serialize + RowSchema> {
    inner: Mutex<Option<WriterVariant>>,
    pub(crate) dir_prefix: String,
    current_file_bytes: Mutex<Option<Bytes>>,
    _phantom: PhantomData<T>,
}

/// Generic wrapper that implements Handler for any Processor with analytics batching
pub struct AnalyticsHandler<P, B> {
    processor: P,
    config: PipelineConfig,
    _batch: PhantomData<B>,
}

impl<T: AnalyticsMetadata + Serialize + RowSchema + 'static> Default for AnalyticsBatch<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            dir_prefix: T::PIPELINE.dir_prefix().as_ref().to_string(),
            current_file_bytes: Mutex::new(None),
            _phantom: PhantomData,
        }
    }
}

impl<T: AnalyticsMetadata + Serialize + RowSchema> AnalyticsBatch<T> {
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
    ) -> sui_indexer_alt_framework::pipeline::concurrent::BatchStatus {
        let Some(first) = values.next() else {
            return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
        };

        // Write all rows to batch (no more side-effect checkpoint tracking!)
        if let Err(e) = batch.write_rows(
            std::iter::once(first).chain(values.by_ref()),
            self.config.file_format,
        ) {
            tracing::error!("Failed to write rows to batch: {}", e);
            return sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending;
        }

        sui_indexer_alt_framework::pipeline::concurrent::BatchStatus::Pending
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

        // Extract checkpoint range from watermarks (guaranteed to be contiguous)
        let checkpoint_range =
            sui_indexer_alt_framework::pipeline::WatermarkPart::checkpoint_range(watermarks)
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

        let object_store_path =
            object_store::path::Path::from(object_path.to_string_lossy().as_ref());

        conn.object_store()
            .put(&object_store_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::{handlers::Handler, models::watermarks::CommitterWatermark};

pub mod concurrent;
mod processor;

/// Extra buffer added to channels between tasks in a pipeline. There does not need to be a huge
/// capacity here because tasks already buffer rows to insert internally.
const PIPELINE_BUFFER: usize = 5;

/// The maximum number of watermarks that can show up in a single batch. This limit exists to deal
/// with pipelines that produce no data for a majority of checkpoints -- the size of these
/// pipeline's batches will be dominated by watermark updates.
const MAX_WATERMARK_UPDATES: usize = 10_000;

#[derive(clap::Args, Debug, Clone)]
pub struct PipelineConfig {
    /// Number of concurrent writers per pipeline
    #[arg(long, default_value_t = 5)]
    write_concurrency: usize,

    /// The collector will check for pending data at least this often
    #[arg(
        long,
        default_value = "500",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis),
    )]
    collect_interval: Duration,

    /// Watermark task will check for pending watermarks this often
    #[arg(
        long,
        default_value = "500",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis),
    )]
    watermark_interval: Duration,

    /// Avoid writing to the watermark table
    #[arg(long)]
    skip_watermark: bool,
}

/// Processed values associated with a single checkpoint. This is an internal type used to
/// communicate between the processor and the collector parts of the pipeline.
struct Indexed<H: Handler> {
    /// Values to be inserted into the database from this checkpoint
    values: Vec<H::Value>,
    /// The watermark associated with this checkpoint and the part of it that is left to commit
    watermark: WatermarkPart,
}

/// Values ready to be written to the database. This is an internal type used to communicate
/// between the collector and the committer parts of the pipeline.
struct Batched<H: Handler> {
    /// The rows to write
    values: Vec<H::Value>,
    /// Proportions of all the watermarks that are represented in this chunk
    watermark: Vec<WatermarkPart>,
}

/// A representation of the proportion of a watermark.
#[derive(Debug)]
struct WatermarkPart {
    /// The watermark itself
    watermark: CommitterWatermark<'static>,
    /// The number of rows from this watermark that are in this part
    batch_rows: usize,
    /// The total number of rows from this watermark
    total_rows: usize,
}

/// Internal type used by workers to propagate errors or shutdown signals up to their
/// supervisor.
#[derive(thiserror::Error, Debug)]
enum Break {
    #[error("Shutdown received")]
    Cancel,

    #[error(transparent)]
    Err(#[from] anyhow::Error),
}

impl<H: Handler> Indexed<H> {
    fn new(epoch: u64, cp_sequence_number: u64, tx_hi: u64, values: Vec<H::Value>) -> Self {
        Self {
            watermark: WatermarkPart {
                watermark: CommitterWatermark {
                    pipeline: H::NAME.into(),
                    epoch_hi_inclusive: epoch as i64,
                    checkpoint_hi_inclusive: cp_sequence_number as i64,
                    tx_hi: tx_hi as i64,
                },
                batch_rows: values.len(),
                total_rows: values.len(),
            },
            values,
        }
    }

    /// The checkpoint sequence number that this data is from
    fn checkpoint(&self) -> u64 {
        self.watermark.watermark.checkpoint_hi_inclusive as u64
    }

    /// Whether there are values left to commit from this indexed checkpoint.
    fn is_empty(&self) -> bool {
        debug_assert!(self.watermark.batch_rows == 0);
        self.values.is_empty()
    }

    /// Adds data from this indexed checkpoint to the `batch`, honoring the handler's bounds on
    /// chunk size.
    fn batch_into(&mut self, batch: &mut Batched<H>) {
        if batch.values.len() + self.values.len() > H::CHUNK_SIZE {
            let mut for_batch = self.values.split_off(H::CHUNK_SIZE - batch.values.len());
            std::mem::swap(&mut self.values, &mut for_batch);
            batch.watermark.push(self.watermark.take(for_batch.len()));
            batch.values.extend(for_batch);
        } else {
            batch.watermark.push(self.watermark.take(self.values.len()));
            batch.values.extend(std::mem::take(&mut self.values));
        }
    }
}

impl<H: Handler> Batched<H> {
    fn new() -> Self {
        Self {
            values: vec![],
            watermark: vec![],
        }
    }

    /// Number of rows in this batch.
    fn len(&self) -> usize {
        self.values.len()
    }

    /// The batch is full if it has more than enough values to write to the database, or more than
    /// enough watermarks to update.
    fn is_full(&self) -> bool {
        self.values.len() >= H::CHUNK_SIZE || self.watermark.len() >= MAX_WATERMARK_UPDATES
    }
}

impl WatermarkPart {
    fn checkpoint(&self) -> u64 {
        self.watermark.checkpoint_hi_inclusive as u64
    }

    /// Check if all the rows from this watermark are represented in this part.
    fn is_complete(&self) -> bool {
        self.batch_rows == self.total_rows
    }

    /// Add the rows from `other` to this part.
    fn add(&mut self, other: WatermarkPart) {
        debug_assert_eq!(self.checkpoint(), other.checkpoint());
        self.batch_rows += other.batch_rows;
    }

    /// Record that `rows` have been taken from this part.
    fn take(&mut self, rows: usize) -> WatermarkPart {
        debug_assert!(
            self.batch_rows >= rows,
            "Can't take more rows than are available"
        );

        self.batch_rows -= rows;
        WatermarkPart {
            watermark: self.watermark.clone(),
            batch_rows: rows,
            total_rows: self.total_rows,
        }
    }
}

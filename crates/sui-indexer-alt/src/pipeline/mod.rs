// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::models::watermarks::CommitterWatermark;

pub use processor::Processor;
use sui_default_config::DefaultConfig;

pub mod concurrent;
mod processor;
pub(crate) mod sequential;

/// Tracing message for the watermark update will be logged at info level at least this many
/// checkpoints.
const LOUD_WATERMARK_UPDATE_INTERVAL: i64 = 5 * 10;

/// Extra buffer added to channels between tasks in a pipeline. There does not need to be a huge
/// capacity here because tasks already buffer rows to insert internally.
const PIPELINE_BUFFER: usize = 5;

/// Issue a warning every time the number of pending watermarks exceeds this number. This can
/// happen if the pipeline was started with its initial checkpoint overridden to be strictly
/// greater than its current watermark -- in that case, the pipeline will never be able to update
/// its watermarks.
///
/// This may be a legitimate thing to do when backfilling a table, but in that case
/// `--skip-watermarks` should be used.
const WARN_PENDING_WATERMARKS: usize = 10000;

#[DefaultConfig]
#[derive(Clone)]
pub struct CommitterConfig {
    /// Number of concurrent writers per pipeline.
    write_concurrency: usize,

    /// The collector will check for pending data at least this often, in milliseconds.
    collect_interval_ms: u64,

    /// Watermark task will check for pending watermarks this often, in milliseconds.
    watermark_interval_ms: u64,

    /// Avoid writing to the watermark table
    pub skip_watermark: bool,
}

/// Like a [CommitterConfig] but with all its fields optional. This type is accepted in configs
/// when we want to support layering overrides on top of a [CommitterConfig].
#[DefaultConfig]
#[derive(Clone, Default)]
pub struct CommitterLayer {
    write_concurrency: Option<usize>,
    collect_interval_ms: Option<u64>,
    watermark_interval_ms: Option<u64>,
    skip_watermark: Option<bool>,
}

/// Processed values associated with a single checkpoint. This is an internal type used to
/// communicate between the processor and the collector parts of the pipeline.
struct Indexed<P: Processor> {
    /// Values to be inserted into the database from this checkpoint
    values: Vec<P::Value>,
    /// The watermark associated with this checkpoint
    watermark: CommitterWatermark<'static>,
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

impl CommitterConfig {
    pub fn collect_interval(&self) -> Duration {
        Duration::from_millis(self.collect_interval_ms)
    }

    pub fn watermark_interval(&self) -> Duration {
        Duration::from_millis(self.watermark_interval_ms)
    }
}

impl CommitterLayer {
    /// Apply the overrides in this layer on top of the base `committer_config`, and return the
    /// result.
    pub fn finish(self, committer_config: &CommitterConfig) -> CommitterConfig {
        CommitterConfig {
            write_concurrency: self
                .write_concurrency
                .unwrap_or(committer_config.write_concurrency),
            collect_interval_ms: self
                .collect_interval_ms
                .unwrap_or(committer_config.collect_interval_ms),
            watermark_interval_ms: self
                .watermark_interval_ms
                .unwrap_or(committer_config.watermark_interval_ms),
            skip_watermark: self
                .skip_watermark
                .unwrap_or(committer_config.skip_watermark),
        }
    }
}

impl<P: Processor> Indexed<P> {
    fn new(
        epoch: u64,
        cp_sequence_number: u64,
        tx_hi: u64,
        timestamp_ms: u64,
        values: Vec<P::Value>,
    ) -> Self {
        Self {
            watermark: CommitterWatermark {
                pipeline: P::NAME.into(),
                epoch_hi_inclusive: epoch as i64,
                checkpoint_hi_inclusive: cp_sequence_number as i64,
                tx_hi: tx_hi as i64,
                timestamp_ms_hi_inclusive: timestamp_ms as i64,
            },
            values,
        }
    }

    /// Number of rows from this checkpoint
    fn len(&self) -> usize {
        self.values.len()
    }

    /// The checkpoint sequence number that this data is from
    fn checkpoint(&self) -> u64 {
        self.watermark.checkpoint_hi_inclusive as u64
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

impl Default for CommitterConfig {
    fn default() -> Self {
        Self {
            write_concurrency: 5,
            collect_interval_ms: 500,
            watermark_interval_ms: 500,
            skip_watermark: false,
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub use processor::Processor;
use serde::{Deserialize, Serialize};

use crate::store::CommitterWatermark;

pub mod concurrent;
mod logging;
mod processor;
pub mod sequential;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitterConfig {
    /// Number of concurrent writers per pipeline.
    pub write_concurrency: usize,

    /// The collector will check for pending data at least this often, in milliseconds.
    pub collect_interval_ms: u64,

    /// Watermark task will check for pending watermarks this often, in milliseconds.
    pub watermark_interval_ms: u64,
}

/// Processed values associated with a single checkpoint. This is an internal type used to
/// communicate between the processor and the collector parts of the pipeline.
struct IndexedCheckpoint<P: Processor> {
    /// Values to be inserted into the database from this checkpoint
    values: Vec<P::Value>,
    /// The watermark associated with this checkpoint
    watermark: CommitterWatermark,
}

/// A representation of the proportion of a watermark.
#[derive(Debug)]
struct WatermarkPart {
    /// The watermark itself
    watermark: CommitterWatermark,
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

impl<P: Processor> IndexedCheckpoint<P> {
    fn new(
        epoch: u64,
        cp_sequence_number: u64,
        tx_hi: u64,
        timestamp_ms: u64,
        values: Vec<P::Value>,
    ) -> Self {
        Self {
            watermark: CommitterWatermark {
                epoch_hi_inclusive: epoch,
                checkpoint_hi_inclusive: cp_sequence_number,
                tx_hi,
                timestamp_ms_hi_inclusive: timestamp_ms,
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
        self.watermark.checkpoint_hi_inclusive
    }
}

impl WatermarkPart {
    fn checkpoint(&self) -> u64 {
        self.watermark.checkpoint_hi_inclusive
    }

    fn timestamp_ms(&self) -> u64 {
        self.watermark.timestamp_ms_hi_inclusive
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
            watermark: self.watermark,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sui_types::full_checkpoint_content::CheckpointData;

    // Test implementation of Processor
    struct TestProcessor;
    impl Processor for TestProcessor {
        const NAME: &'static str = "test";
        type Value = i32;

        fn process(&self, _checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![1, 2, 3])
        }
    }

    #[test]
    fn test_watermark_part_getters() {
        let watermark = CommitterWatermark {
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: 100,
            tx_hi: 1000,
            timestamp_ms_hi_inclusive: 1234567890,
        };

        let part = WatermarkPart {
            watermark,
            batch_rows: 50,
            total_rows: 200,
        };

        assert_eq!(part.checkpoint(), 100);
        assert_eq!(part.timestamp_ms(), 1234567890);
    }

    #[test]
    fn test_watermark_part_is_complete() {
        let part = WatermarkPart {
            watermark: CommitterWatermark::default(),
            batch_rows: 200,
            total_rows: 200,
        };

        assert!(part.is_complete());
    }

    #[test]
    fn test_watermark_part_is_not_complete() {
        let part = WatermarkPart {
            watermark: CommitterWatermark::default(),
            batch_rows: 199,
            total_rows: 200,
        };

        assert!(!part.is_complete());
    }

    #[test]
    fn test_watermark_part_becomes_complete_after_adding_new_batch() {
        let mut part = WatermarkPart {
            watermark: CommitterWatermark::default(),
            batch_rows: 199,
            total_rows: 200,
        };

        // Add a batch that makes it complete
        part.add(WatermarkPart {
            watermark: CommitterWatermark::default(),
            batch_rows: 1,
            total_rows: 200,
        });

        assert!(part.is_complete());
        assert_eq!(part.batch_rows, 200);
    }

    #[test]
    fn test_watermark_part_becomes_incomplete_after_taking_away_batch() {
        let mut part = WatermarkPart {
            watermark: CommitterWatermark::default(),
            batch_rows: 200,
            total_rows: 200,
        };
        assert!(part.is_complete(), "Initial part should be complete");

        // Take away a portion of the batch
        let extracted_part = part.take(10);

        // Verify state of extracted part
        assert!(!extracted_part.is_complete());
        assert_eq!(extracted_part.batch_rows, 10);
        assert_eq!(extracted_part.total_rows, 200);
    }

    #[test]
    fn test_indexed_checkpoint() {
        let epoch = 1;
        let cp_sequence_number = 100;
        let tx_hi = 1000;
        let timestamp_ms = 1234567890;
        let values = vec![1, 2, 3];

        let checkpoint = IndexedCheckpoint::<TestProcessor>::new(
            epoch,
            cp_sequence_number,
            tx_hi,
            timestamp_ms,
            values,
        );

        assert_eq!(checkpoint.len(), 3);
        assert_eq!(checkpoint.checkpoint(), 100);
    }

    #[test]
    fn test_indexed_checkpoint_with_empty_values() {
        let epoch = 1;
        let cp_sequence_number = 100;
        let tx_hi = 1000;
        let timestamp_ms = 1234567890;
        let values: Vec<<TestProcessor as Processor>::Value> = vec![];

        let checkpoint = IndexedCheckpoint::<TestProcessor>::new(
            epoch,
            cp_sequence_number,
            tx_hi,
            timestamp_ms,
            values,
        );

        assert_eq!(checkpoint.len(), 0);
        assert_eq!(checkpoint.checkpoint(), 100);
    }
}

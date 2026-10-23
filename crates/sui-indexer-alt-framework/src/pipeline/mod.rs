// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub use crate::config::ConcurrencyConfig;
use crate::store::CommitterWatermark;
pub use processor::Processor;
use rand::Rng;
use serde::Deserialize;
use serde::Serialize;

pub mod concurrent;
mod flush;
mod logging;
mod processor;
pub mod sequential;

/// Issue a warning every time the number of pending watermarks exceeds this number. This can
/// happen if the pipeline was started with its initial checkpoint overridden to be strictly
/// greater than its current watermark -- in that case, the pipeline will never be able to update
/// its watermarks.
const WARN_PENDING_WATERMARKS: usize = 10000;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitterConfig {
    /// Number of concurrent writers per pipeline.
    pub write_concurrency: usize,

    /// Under [`FlushPolicy::Interval`], the collector will check for pending data at least this
    /// often, in milliseconds. Not used under [`FlushPolicy::WhenIdle`].
    pub collect_interval_ms: u64,

    /// Minimum time between two watermark writes of a concurrent pipeline, in milliseconds. A
    /// watermark that advances once this much time has passed since the previous write is written
    /// immediately; one that advances sooner is written when the gap has passed.
    pub watermark_interval_ms: u64,

    /// Maximum random jitter to add to the watermark interval, in milliseconds.
    pub watermark_interval_jitter_ms: u64,

    /// When the collector cuts a batch from pending rows that are fewer than the pipeline's
    /// `min_eager_rows`.
    #[serde(default)]
    pub flush_policy: FlushPolicy,
}

/// When a pipeline's collector cuts a batch out of its pending rows and hands it to the committer.
///
/// Under every policy, a batch is cut as soon as at least `min_eager_rows` rows are pending. For
/// sequential pipelines that count includes rows buffered for checkpoints that arrived ahead of
/// their predecessors, but a batch only ever holds the in-order run starting at the next checkpoint
/// to commit. The policies differ in when fewer rows than `min_eager_rows` are written.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FlushPolicy {
    /// Write whatever is pending every `collect_interval_ms`.
    #[default]
    Interval,

    /// Write whatever is pending as soon as no batch the collector handed over earlier is still
    /// being written. When the pipeline keeps up with its input, each checkpoint is written as soon
    /// as it has been processed. When writes are the bottleneck, one is always in flight, so rows
    /// accumulate until `min_eager_rows` are pending. `collect_interval_ms` is not used.
    WhenIdle,
}

/// Per-pipeline ingestion settings.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct IngestionConfig {
    /// Capacity of this pipeline's bounded subscriber channel. If `None`, the built-in default
    /// is used (see [`IngestionConfig::subscriber_channel_size`]).
    pub subscriber_channel_size: Option<usize>,
}

impl IngestionConfig {
    /// Resolves `subscriber_channel_size` to its final value, substituting the built-in default
    /// if unset.
    ///
    /// The default is small on purpose: the adaptive controller does the real backpressure work,
    /// and larger values just pin more decoded checkpoints in memory without throughput benefit.
    /// Scales with CPU count for fetch parallelism headroom, with a floor of 4 so the
    /// controller's dead band (0.6..0.85) has integer room to maneuver on small machines.
    pub fn subscriber_channel_size(&self) -> usize {
        self.subscriber_channel_size
            .unwrap_or_else(|| (num_cpus::get() / 2).max(4))
    }
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
#[derive(Debug, Clone)]
struct WatermarkPart {
    /// The watermark itself
    watermark: CommitterWatermark,
    /// The number of rows from this watermark that are in this part
    batch_rows: usize,
    /// The total number of rows from this watermark
    total_rows: usize,
}

impl CommitterConfig {
    pub fn collect_interval(&self) -> Duration {
        Duration::from_millis(self.collect_interval_ms)
    }

    pub fn watermark_interval(&self) -> Duration {
        Duration::from_millis(self.watermark_interval_ms)
    }

    /// Returns the earliest instant at which the next watermark write may start: now plus the
    /// watermark interval plus a random jitter between 0 and `watermark_interval_jitter_ms`.
    pub fn watermark_interval_with_jitter(&self) -> tokio::time::Instant {
        let jitter = if self.watermark_interval_jitter_ms == 0 {
            0
        } else {
            rand::thread_rng().gen_range(0..=self.watermark_interval_jitter_ms)
        };
        tokio::time::Instant::now() + Duration::from_millis(self.watermark_interval_ms + jitter)
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
        assert_eq!(self.checkpoint(), other.checkpoint());
        self.batch_rows += other.batch_rows;
        assert!(
            self.batch_rows <= self.total_rows,
            "batch_rows ({}) exceeded total_rows ({})",
            self.batch_rows,
            self.total_rows,
        );
    }

    /// Record that `rows` have been taken from this part.
    fn take(&mut self, rows: usize) -> WatermarkPart {
        assert!(
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
            watermark_interval_jitter_ms: 0,
            flush_policy: FlushPolicy::Interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use sui_types::full_checkpoint_content::Checkpoint;

    // Test implementation of Processor
    struct TestProcessor;
    #[async_trait]
    impl Processor for TestProcessor {
        const NAME: &'static str = "test";
        type Value = i32;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
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

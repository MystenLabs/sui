// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

pub use processor::Processor;
use rand::Rng;
use serde::Deserialize;
use serde::Serialize;
pub use sui_concurrency_limiter::ConcurrencyLimit;
use sui_concurrency_limiter::Limiter;

use crate::ingestion::ingestion_client::IngestionMode;
use crate::store::CommitterWatermark;

pub mod concurrent;
mod logging;
mod processor;
pub mod sequential;

/// Small slack buffer added to channels between pipeline stages. Override at runtime with
/// `CHANNEL_BUFFER` env var. Default: 5.
fn channel_buffer() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("CHANNEL_BUFFER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5)
    })
}

/// Channel size for the processor→collector (concurrent) or processor→committer (sequential)
/// handoff. Override at runtime with `PROCESSOR_CHANNEL_SIZE` env var.
/// Default: `Processor::FANOUT + channel_buffer()`.
fn processor_channel_size(fanout: usize) -> usize {
    static CACHED: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            std::env::var("PROCESSOR_CHANNEL_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(fanout + channel_buffer())
}

/// Channel size for the collector→committer handoff, where batched rows wait to be committed.
/// Override at runtime with `COMMITTER_CHANNEL_SIZE` env var. Default: 10.
fn committer_channel_size() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("COMMITTER_CHANNEL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10)
    })
}

/// Channel size for the committer→watermark handoff in concurrent pipelines.
/// Override at runtime with `WATERMARK_CHANNEL_SIZE` env var.
/// Default: `num_cpus::get() + channel_buffer()`.
fn watermark_channel_size() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("WATERMARK_CHANNEL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(num_cpus::get() + channel_buffer())
    })
}

/// Issue a warning every time the number of pending watermarks exceeds this number. This can
/// happen if the pipeline was started with its initial checkpoint overridden to be strictly
/// greater than its current watermark -- in that case, the pipeline will never be able to update
/// its watermarks.
const WARN_PENDING_WATERMARKS: usize = 10000;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitterConfig {
    /// Concurrency limit for writers in this pipeline.
    pub write_concurrency: ConcurrencyLimit,

    /// The collector will check for pending data at least this often, in milliseconds.
    pub collect_interval_ms: u64,

    /// Watermark task will check for pending watermarks this often, in milliseconds.
    pub watermark_interval_ms: u64,

    /// Maximum random jitter to add to the watermark interval, in milliseconds.
    pub watermark_interval_jitter_ms: u64,

    /// Target weight for individual commit batches when capacity batching is enabled. The dispatch
    /// loop splits available limiter capacity into chunks of this size, allowing multiple smaller
    /// batches to be committed in parallel. If not set, each batch consumes all available capacity
    /// (up to MAX_BATCH_WEIGHT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_batch_weight: Option<usize>,
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
    /// Build the concurrency limiter from config.
    pub fn build_limiter(&self) -> Limiter {
        self.write_concurrency.build()
    }

    pub fn collect_interval(&self) -> Duration {
        Duration::from_millis(self.collect_interval_ms)
    }

    pub fn watermark_interval(&self) -> Duration {
        Duration::from_millis(self.watermark_interval_ms)
    }

    /// Returns the next watermark update instant with a random jitter added. The jitter is a
    /// random value between 0 and `watermark_interval_jitter_ms`.
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

impl CommitterConfig {
    pub(crate) fn for_mode(_mode: IngestionMode) -> Self {
        Self::default()
    }
}

impl Default for CommitterConfig {
    fn default() -> Self {
        Self {
            write_concurrency: ConcurrencyLimit::Fixed { limit: 10 },
            collect_interval_ms: 500,
            watermark_interval_ms: 500,
            watermark_interval_jitter_ms: 0,
            target_batch_weight: None,
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

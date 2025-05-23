// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scoped_futures::ScopedBoxFuture;
use std::time::Duration;

/// Represents a database connection that can be used by the indexer framework to manage watermark
/// operations, agnostic of the underlying store implementation.
#[async_trait]
pub trait Connection: Send + Sync {
    /// Given a pipeline, return the committer watermark from the `Store`. This is used by the
    /// indexer on startup to determine which checkpoint to resume processing from.
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>>;

    /// Given a pipeline, return the reader watermark from the database. This is used by the indexer
    /// to determine the new `reader_lo` or inclusive lower bound of available data.
    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>>;

    /// Get the bounds for the region that the pruner is allowed to prune, and the time in
    /// milliseconds the pruner must wait before it can begin pruning data for the given `pipeline`.
    /// The pruner is allowed to prune the region between the returned `pruner_hi` (inclusive) and
    /// `reader_lo` (exclusive) after waiting until `pruner_timestamp + delay` has passed. This
    /// minimizes the possibility for the pruner to delete data still expected by inflight read
    /// requests.
    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>>;

    /// Upsert the high watermark as long as it raises the watermark stored in the database. Returns
    /// a boolean indicating whether the watermark was actually updated or not.
    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool>;

    /// Update the `reader_lo` of an existing watermark entry only if it raises `reader_lo`. Readers
    /// will reference this as the inclusive lower bound of available data for the corresponding
    /// pipeline.
    ///
    /// If an update is to be made, some timestamp (i.e `pruner_timestamp`) should also be set on
    /// the watermark entry to the current time. Ideally, this would be from the perspective of the
    /// store. If this is not possible, then it should come from some other common source of time
    /// between the indexer and its readers. This timestamp is critical to the indexer's operations,
    /// as it determines when the pruner can safely begin pruning data. When `pruner_watermark` is
    /// called by the indexer, it will retrieve this timestamp to determine how much longer to wait
    /// before beginning to prune.
    ///
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool>;

    /// Update the pruner watermark, returns true if the watermark was actually updated
    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool>;
}

/// A storage-agnostic interface that provides database connections for both watermark management
/// and arbitrary writes. The indexer framework accepts this `Store` implementation to manage
/// watermarks operations through its associated `Connection` type. This store is also passed to the
/// pipeline handlers to perform arbitrary writes to the store.
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: Connection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;
}

/// Extends the Store trait with transactional capabilities, to be used within the framework for
/// atomic or transactional writes.
#[async_trait]
pub trait TransactionalStore: Store {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>;
}

/// Represents the highest checkpoint for some pipeline that has been processed by the indexer
/// framework. When read from the `Store`, this represents the inclusive upper bound checkpoint of
/// data that has been written to the Store for a pipeline.
#[derive(Default, Debug, Clone, Copy)]
pub struct CommitterWatermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
}

/// Represents the inclusive lower bound of available data in the Store for some pipeline.
#[derive(Default, Debug, Clone, Copy)]
pub struct ReaderWatermark {
    /// Within the framework, this value is used to determine the new `reader_lo`.
    pub checkpoint_hi_inclusive: u64,
    /// Within the framework, this value is used to check whether to actually make an update
    /// transaction to the database.
    pub reader_lo: u64,
}

/// A watermark that represents the bounds for the region that the pruner is allowed to prune, and
/// the time in milliseconds the pruner must wait before it can begin pruning data.
#[derive(Default, Debug, Clone, Copy)]
pub struct PrunerWatermark {
    /// The remaining time in milliseconds that the pruner must wait before it can begin pruning.
    ///
    /// This is calculated by finding the difference between the time when it becomes safe to prune
    /// and the current time: `(pruner_timestamp + delay) - current_time`.
    ///
    /// The pruner will wait for this duration before beginning to delete data if it is positive.
    /// When this value is zero or negative, it means the waiting period has already passed and
    /// pruning can begin immediately.
    pub wait_for_ms: i64,

    /// The pruner can delete up to this checkpoint (exclusive).
    pub reader_lo: u64,

    /// The pruner has already deleted up to this checkpoint (exclusive), so can continue from this
    /// point.
    pub pruner_hi: u64,
}

impl CommitterWatermark {
    pub fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive as i64).unwrap_or_default()
    }

    /// Convenience function for testing, instantiates a CommitterWatermark with the given
    /// `checkpoint_hi_inclusive` and sets all other values to 0.
    pub fn new_for_testing(checkpoint_hi_inclusive: u64) -> Self {
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }
    }
}

impl PrunerWatermark {
    /// Returns the duration that the pruner must wait before it can begin pruning data.
    pub fn wait_for(&self) -> Option<Duration> {
        (self.wait_for_ms > 0).then(|| Duration::from_millis(self.wait_for_ms as u64))
    }

    /// The next chunk of checkpoints that the pruner should work on, to advance the watermark. If
    /// no more checkpoints to prune, returns `None`. Otherwise, returns a tuple (from,
    /// to_exclusive) where `from` is inclusive and `to_exclusive` is exclusive. Advance the
    /// watermark as well.
    pub fn next_chunk(&mut self, size: u64) -> Option<(u64, u64)> {
        if self.pruner_hi >= self.reader_lo {
            return None;
        }

        let from = self.pruner_hi;
        let to_exclusive = (from + size).min(self.reader_lo);
        self.pruner_hi = to_exclusive;
        Some((from, to_exclusive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_pruner_watermark_wait_for_positive() {
        let watermark = PrunerWatermark {
            wait_for_ms: 5000, // 5 seconds
            reader_lo: 1000,
            pruner_hi: 500,
        };

        assert_eq!(watermark.wait_for(), Some(Duration::from_millis(5000)));
    }

    #[test]
    fn test_pruner_watermark_wait_for_zero() {
        let watermark = PrunerWatermark {
            wait_for_ms: 0,
            reader_lo: 1000,
            pruner_hi: 500,
        };

        assert_eq!(watermark.wait_for(), None);
    }

    #[test]
    fn test_pruner_watermark_wait_for_negative() {
        let watermark = PrunerWatermark {
            wait_for_ms: -5000,
            reader_lo: 1000,
            pruner_hi: 500,
        };

        assert_eq!(watermark.wait_for(), None);
    }

    #[test]
    fn test_pruner_watermark_no_more_chunks() {
        let mut watermark = PrunerWatermark {
            wait_for_ms: 0,
            reader_lo: 1000,
            pruner_hi: 1000,
        };

        assert_eq!(watermark.next_chunk(100), None);
    }

    #[test]
    fn test_pruner_watermark_chunk_boundaries() {
        let mut watermark = PrunerWatermark {
            wait_for_ms: 0,
            reader_lo: 1000,
            pruner_hi: 100,
        };

        assert_eq!(watermark.next_chunk(100), Some((100, 200)));
        assert_eq!(watermark.pruner_hi, 200);
        assert_eq!(watermark.next_chunk(100), Some((200, 300)));

        // Reset and test oversized chunk
        let mut watermark = PrunerWatermark {
            wait_for_ms: 0,
            reader_lo: 1000,
            pruner_hi: 500,
        };

        // Chunk larger than remaining range
        assert_eq!(watermark.next_chunk(2000), Some((500, 1000)));
        assert_eq!(watermark.pruner_hi, 1000);
        assert_eq!(watermark.next_chunk(2000), None);
    }
}

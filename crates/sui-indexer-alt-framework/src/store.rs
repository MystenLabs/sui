// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use crate::pipeline::sequential::Handler as SequentialHandler;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scoped_futures::ScopedBoxFuture;
use std::time::Duration;

pub use scoped_futures;

/// Represents a database connection that can manage watermarks for pipeline operations in the
/// framework.
///
/// This trait provides methods for the necessary reads and updates for the committer, reader, and
/// pruner components of a data pipeline, allowing for tracking progress and coordination between
/// different pipeline stages.
#[async_trait]
pub trait DbConnection: Send + Sync {
    /// Given a pipeline, return the committer watermark from the database. This is used on indexer
    /// startup to determine which checkpoint to start or continue processing from.
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>>;

    /// Given a pipeline, return the reader watermark from the database. This is used by the indexer
    /// to report progress to the database.
    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>>;

    /// Upsert the high watermark as long as it raises the watermark stored in the database.
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool>;

    /// Update the reader low watermark for an existing watermark row, as long as this raises the
    /// watermark, and updates the timestamp this update happened to the database's current time.
    ///
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool>;

    /// Get the bounds for the region that the pruner still has to prune for the given `pipeline`,
    /// along with a duration to wait before acting on this information, based on the time at which
    /// the pruner last updated the bounds, and the configured `delay`. More specifically, this is
    /// the result of delay + (pruner_timestamp - current_database_time)
    ///
    /// The pruner is allowed to prune the region between the returned `pruner_hi` (inclusive) and
    /// `reader_lo` (exclusive) after `wait_for` milliseconds have passed since this response was
    /// returned.
    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>>; // (pruner_hi, reader_lo, wait_for_ms)

    /// Update the pruner watermark, returns true if the watermark was actually updated
    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool>;
}

/// A storage-agnostic interface for managing pipeline watermarks.
///
/// This trait abstracts away the underlying storage implementation, providing
/// a consistent way to connect to the database regardless of the specific
/// storage technology being used.
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;
}

pub type HandlerBatch<H> = <H as SequentialHandler>::Batch;

/// Extends the Store trait with transactional capabilities.
///
/// This trait provides methods to execute operations within a database transaction,
/// ensuring atomicity when committing handler batches and updating watermarks.
/// It allows for safely combining multiple database operations that must succeed
/// or fail together.
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

#[derive(Default, Debug, Clone, Copy)]
pub struct CommitterWatermark {
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ReaderWatermark {
    /// Within the framework, this value is used to determine the new `reader_lo`.
    pub checkpoint_hi_inclusive: i64,
    /// Within the framework, this value is used to check whether to actually make an update
    /// transaction to the database.
    pub reader_lo: i64,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct PrunerWatermark {
    /// How long to wait from when this query ran on the database until this information can be
    /// used to prune the database. This number could be negative, meaning no waiting is necessary.
    pub wait_for: i64,

    /// The pruner can delete up to this checkpoint, (exclusive).
    pub reader_lo: i64,

    /// The pruner has already deleted up to this checkpoint (exclusive), so can continue from this
    /// point.
    pub pruner_hi: i64,
}

impl CommitterWatermark {
    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive).unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn new_for_testing(checkpoint_hi_inclusive: u64) -> Self {
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: checkpoint_hi_inclusive as i64,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }
    }
}

impl PrunerWatermark {
    pub(crate) fn wait_for(&self) -> Option<Duration> {
        (self.wait_for > 0).then(|| Duration::from_millis(self.wait_for as u64))
    }

    /// The next chunk of checkpoints that the pruner should work on, to advance the watermark.
    /// If no more checkpoints to prune, returns `None`.
    /// Otherwise, returns a tuple (from, to_exclusive) where `from` is inclusive and `to_exclusive` is exclusive.
    /// Advance the watermark as well.
    pub(crate) fn next_chunk(&mut self, size: u64) -> Option<(u64, u64)> {
        if self.pruner_hi >= self.reader_lo {
            return None;
        }

        let from = self.pruner_hi as u64;
        let to_exclusive = (from + size).min(self.reader_lo as u64);
        self.pruner_hi = to_exclusive as i64;
        Some((from, to_exclusive))
    }
}

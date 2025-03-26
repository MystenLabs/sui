// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use crate::pipeline::sequential::Handler as SequentialHandler;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scoped_futures::ScopedBoxFuture;
use std::time::Duration;

pub use scoped_futures;

#[async_trait]
pub trait DbConnection: Send + Sync {
    /// Given a pipeline, return the `epoch_hi_inclusive`, `checkpoint_hi_inclusive`, `tx_hi`, and `timestamp_ms_hi_inclusive` from the database
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>>;

    /// Update the committer watermark, returns true if the watermark was actually updated.
    /// Watermark update managed by the framework ...
    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool>;

    /// Given a pipeline, return the `checkpoint_hi_inclusive` and `reader_lo` from the database. Checkpoint hi used to determine new reader lo and reader lo used to check whether to actually make
    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>>;

    /// Update the reader watermark, returns true if the watermark was actually updated
    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool>;

    /// Get the pruner watermark with wait_for calculated
    ///
    /// # Implementation Requirements
    /// This method MUST:
    /// 1. Calculate wait_for as: delay + (pruner_timestamp - current_database_time)
    /// 2. Return (pruner_hi, reader_lo, wait_for)
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

/// Public trait for storage-agnostic watermark operations
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;
}

pub type HandlerBatch<H> = <H as SequentialHandler>::Batch;

#[async_trait]
pub trait TransactionalStore: Store {
    /// Execute a handler's commit function and update the watermark within a transaction
    async fn transactional_commit_with_watermark<'a, H>(
        &'a self,
        pipeline: &'static str,
        watermark: &'a CommitterWatermark,
        batch: &'a HandlerBatch<H>,
    ) -> anyhow::Result<usize>
    where
        H: SequentialHandler<Store = Self> + Send + Sync + 'a;

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
    pub checkpoint_hi_inclusive: i64,
    pub reader_lo: i64,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct PrunerWatermark {
    pub pruner_hi: i64,
    pub reader_lo: i64,
    pub wait_for: i64,
}

impl CommitterWatermark {
    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive).unwrap_or_default()
    }
}

impl PrunerWatermark {
    pub(crate) fn wait_for(&self) -> Option<Duration> {
        (self.wait_for > 0).then(|| Duration::from_millis(self.wait_for as u64))
    }

    pub(crate) fn next_chunk(&self, size: u64) -> Option<(u64, u64)> {
        if self.pruner_hi >= self.reader_lo {
            return None;
        }

        let from = self.pruner_hi as u64;
        let to_exclusive = (from + size).min(self.reader_lo as u64);
        Some((from, to_exclusive))
    }
}

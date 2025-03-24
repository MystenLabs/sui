// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::watermarks::CommitterWatermark;
pub use crate::pipeline::sequential::Handler as SequentialHandler;
use async_trait::async_trait;
use std::time::Duration;

pub trait DbConnection: Send + Sync {}

/// Public trait for storage-agnostic watermark operations
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;

    /// Given a pipeline, return the `checkpoint_hi_inclusive` and `timestamp_ms` from the database
    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64)>>;

    /// Update the committer watermark, returns true if the watermark was actually updated.
    /// Watermark update managed by the framework ...
    async fn update_committer_watermark(
        &self,
        pipeline: &'static str,
        epoch_hi_inclusive: i64,
        checkpoint_hi_inclusive: i64,
        tx_hi: i64,
        timestamp_ms_hi_inclusive: i64,
    ) -> anyhow::Result<bool>;

    /// Given a pipeline, return the `checkpoint_hi_inclusive` and `reader_lo` from the database
    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64)>>;

    /// Update the reader watermark, returns true if the watermark was actually updated
    async fn update_reader_watermark(
        &self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool>;

    /// Get the pruner watermark with wait_for calculated
    ///
    /// # Implementation Requirements
    /// This method MUST:
    /// 1. Calculate wait_for as: delay + (pruner_timestamp - current_database_time)
    /// 2. Return (pruner_hi, reader_lo, wait_for)
    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<(i64, i64, i64)>>; // (pruner_hi, reader_lo, wait_for_ms)

    /// Update the pruner watermark, returns true if the watermark was actually updated
    async fn update_pruner_watermark(
        &self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool>;
}

pub type HandlerBatch<H> = <H as SequentialHandler>::Batch;

#[async_trait]
pub trait TransactionalStore: Store {
    /// Execute a handler's commit function and update the watermark within a transaction
    async fn transactional_commit_with_watermark<'a, H>(
        &'a self,
        watermark: &'a CommitterWatermark<'static>,
        batch: &'a HandlerBatch<H>,
    ) -> anyhow::Result<usize>
    where
        H: SequentialHandler<Store = Self> + Send + Sync + 'a;
}

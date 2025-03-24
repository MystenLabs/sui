// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use crate::pipeline::sequential::Handler as SequentialHandler;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::time::Duration;

pub trait DbConnection: Send + Sync {}

/// Public trait for storage-agnostic watermark operations
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;

    /// Given a pipeline, return the `epoch_hi_inclusive`, `checkpoint_hi_inclusive`, `tx_hi`, and `timestamp_ms_hi_inclusive` from the database
    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64, i64, i64)>>;

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

#[async_trait]
pub(crate) trait StoreExt: Store {
    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let watermark = Store::get_committer_watermark(self, pipeline).await?;
        Ok(watermark.map(
            |(epoch_hi_inclusive, checkpoint_hi_inclusive, tx_hi, timestamp_ms_hi_inclusive)| {
                CommitterWatermark {
                    epoch_hi_inclusive,
                    checkpoint_hi_inclusive,
                    tx_hi,
                    timestamp_ms_hi_inclusive,
                }
            },
        ))
    }

    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let watermark = Store::get_reader_watermark(self, pipeline).await?;
        Ok(
            watermark.map(|(checkpoint_hi_inclusive, reader_lo)| ReaderWatermark {
                checkpoint_hi_inclusive,
                reader_lo,
            }),
        )
    }

    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        let watermark = Store::get_pruner_watermark(self, pipeline, delay).await?;
        Ok(
            watermark.map(|(pruner_hi, reader_lo, wait_for)| PrunerWatermark {
                pruner_hi,
                reader_lo,
                wait_for,
            }),
        )
    }
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
}

#[derive(Default, Debug, Clone)]
pub struct CommitterWatermark {
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
}

pub struct ReaderWatermark {
    pub checkpoint_hi_inclusive: i64,
    pub reader_lo: i64,
}

pub struct PrunerWatermark {
    pub pruner_hi: i64,
    pub reader_lo: i64,
    pub wait_for: i64,
}

impl<S: Store> StoreExt for S {}

impl CommitterWatermark {
    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive).unwrap_or_default()
    }
}

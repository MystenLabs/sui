// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::watermarks::{
    CommitterWatermark, PrunerWatermark, ReaderWatermark, StoredWatermark,
};
pub use crate::pipeline::sequential::Handler as SequentialHandler;
use async_trait::async_trait;
use std::time::Duration;

pub trait DbConnection: Send + Sync {}

/// Trait for storage-agnostic watermark operations
#[async_trait]
pub trait Store: Send + Sync + 'static + Clone {
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;

    /// Get the current stored watermark for a pipeline
    async fn get_stored_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<StoredWatermark>>;

    /// Get the committer watermark for a pipeline
    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark<'static>>>;

    /// Update the committer watermark, returns true if the watermark was actually updated
    async fn update_committer_watermark(
        &self,
        watermark: &CommitterWatermark<'_>,
    ) -> anyhow::Result<bool>;

    /// Get the reader watermark for a pipeline
    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<StoredWatermark>>;

    /// Update the reader watermark, returns true if the watermark was actually updated
    async fn update_reader_watermark(
        &self,
        watermark: &ReaderWatermark<'_>,
    ) -> anyhow::Result<bool>;

    /// Get the pruner watermark for a pipeline with the specified delay
    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark<'static>>>;

    /// Update the pruner watermark, returns true if the watermark was actually updated
    async fn update_pruner_watermark(
        &self,
        watermark: &PrunerWatermark<'_>,
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

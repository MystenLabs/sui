// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db::{Connection, Db};
use crate::models::watermarks::{
    CommitterWatermark, PrunerWatermark, ReaderWatermark, StoredWatermark,
};
use async_trait::async_trait;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Trait for db connections
#[async_trait]
pub trait DbConnection: Send + Sync {
    /// Execute a function within a transaction
    async fn transaction<F, T>(&mut self, f: F) -> Result<T, anyhow::Error>
    where
        F: FnOnce(&mut Self) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send + '_>>
            + Send,
        T: Send + 'static;
}

/// Trait for database providers
#[async_trait]
pub trait Database: Send + Sync + Clone + 'static {
    /// The connection type returned by this database
    type Connection<'c>: DbConnection
    where
        Self: 'c;

    /// Create a new connection to the database
    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error>;
}

/// Trait for storage-agnostic watermark operations
#[async_trait]
pub trait WatermarkStore: Send + Sync + 'static + Clone {
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

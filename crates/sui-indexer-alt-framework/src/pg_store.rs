// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use diesel_async::scoped_futures::ScopedFuture;
use diesel_async::AsyncConnection;

use crate::db::{Connection as PgConnection, Db as PgDb};
use crate::models::watermarks::{
    CommitterWatermark, PrunerWatermark, ReaderWatermark, StoredWatermark,
};
use crate::store::{Database, DbConnection, WatermarkStore};

/// PostgreSQL implementation of Store
#[derive(Clone)]
pub struct PgStore {
    pub db: PgDb,
}

impl PgStore {
    pub fn new(db: PgDb) -> Self {
        Self { db }
    }
}

#[async_trait]
impl<'c> DbConnection for PgConnection<'c> {
    async fn transaction<F, T>(&mut self, f: F) -> Result<T, anyhow::Error>
    where
        F: FnOnce(
                &mut Self,
            )
                -> Pin<Box<dyn ScopedFuture<Output = Result<T, anyhow::Error>> + Send + '_>>
            + Send,
        T: Send + 'static,
    {
        use diesel_async::scoped_futures::ScopedFutureExt;

        AsyncConnection::transaction(&mut self, |mut conn| {
            async move { f(&mut conn).await.map_err(|e| e.into()) }.scope_boxed()
        })
        .await
    }
}

// Implement the Database trait for Db
#[async_trait]
impl Database for PgDb {
    type Connection<'c> = PgConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        let conn = self.connect().await?;
        Ok(conn)
    }
}
#[async_trait]
impl WatermarkStore for PgStore {
    async fn get_stored_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<StoredWatermark>> {
        let mut conn = self.db.connect().await?;
        StoredWatermark::get(&mut conn, pipeline)
            .await
            .map_err(Into::into)
    }

    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark<'static>>> {
        let mut conn = self.db.connect().await?;
        CommitterWatermark::get(&mut conn, pipeline)
            .await
            .map_err(Into::into)
    }

    async fn update_committer_watermark(
        &self,
        watermark: &CommitterWatermark<'_>,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        watermark.update(&mut conn).await.map_err(Into::into)
    }

    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<StoredWatermark>> {
        let mut conn = self.db.connect().await?;
        StoredWatermark::get(&mut conn, pipeline)
            .await
            .map_err(Into::into)
    }

    async fn update_reader_watermark(
        &self,
        watermark: &ReaderWatermark<'_>,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        watermark.update(&mut conn).await.map_err(Into::into)
    }

    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark<'static>>> {
        let mut conn = self.db.connect().await?;
        PrunerWatermark::get(&mut conn, pipeline, delay)
            .await
            .map_err(Into::into)
    }

    async fn update_pruner_watermark(
        &self,
        watermark: &PrunerWatermark<'_>,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        watermark.update(&mut conn).await.map_err(Into::into)
    }
}

#[async_trait]
impl Database for PgStore {
    type Connection<'c> = PgConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        let conn = self.db.connect().await?;
        Ok(conn)
    }
}

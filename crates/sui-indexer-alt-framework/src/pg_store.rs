// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use async_trait::async_trait;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;

use crate::db::{Connection as PgConnection, Db as PgDb};
use crate::models::watermarks::{
    CommitterWatermark, PrunerWatermark, ReaderWatermark, StoredWatermark,
};
use crate::store::{DbConnection, HandlerBatch, SequentialHandler, Store, TransactionalStore};

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

impl<'c> DbConnection for PgConnection<'c> {}

#[async_trait]
impl Store for PgStore {
    type Connection<'c> = PgConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        let conn = self.connect().await?;
        Ok(conn)
    }

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
impl TransactionalStore for PgStore {
    async fn transactional_commit_with_watermark<'a, H>(
        &'a self,
        watermark: &'a CommitterWatermark<'static>,
        batch: &'a HandlerBatch<H>,
    ) -> anyhow::Result<usize>
    where
        H: SequentialHandler<Store = Self> + Send + Sync + 'a,
    {
        let mut conn = self.db.connect().await?;

        let result = AsyncConnection::transaction(&mut conn, |conn| {
            async {
                watermark.update(conn).await?;
                H::commit(batch, conn).await
            }
            .scope_boxed()
        })
        .await?;

        Ok(result)
    }
}

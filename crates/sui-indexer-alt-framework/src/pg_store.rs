use async_trait::async_trait;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::db::{Connection, Db};
use crate::models::watermarks::{
    CommitterWatermark, PrunerWatermark, ReaderWatermark, StoredWatermark,
};
use crate::store::{ConnectionStore, Database, DbConnection, Store, WatermarkStore};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;

/// PostgreSQL implementation of Store
#[derive(Clone)]
pub struct PgStore {
    pub db: Db,
}

impl PgStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

/// A wrapper around the PostgreSQL connection
pub struct PgConnectionWrapper<'c> {
    pub conn: Connection<'c>,
}

#[async_trait]
impl<'c> DbConnection for Connection<'c> {
    async fn transaction<F, T>(&mut self, f: F) -> Result<T, anyhow::Error>
    where
        F: FnOnce(&mut Self) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send + '_>>
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
impl Database for Db {
    type Connection<'c> = Connection<'c>;

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

        // Get the watermark with the connection's lifetime
        let watermark_result = CommitterWatermark::get(&mut conn, pipeline).await?;

        // Convert to 'static lifetime if there's a watermark
        let static_watermark = watermark_result.map(|watermark| {
            // Create a new watermark with 'static lifetime
            // This requires cloning any data that depends on lifetimes
            CommitterWatermark::new(
                pipeline.to_string(), // Clone the string to make it 'static
                watermark.sequence_number(),
                watermark.timestamp(),
                // Add any other fields that need to be copied
            )
        });

        Ok(static_watermark)
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

        // Get the watermark with the connection's lifetime
        let watermark_result = PrunerWatermark::get(&mut conn, pipeline, delay).await?;

        // Convert to 'static lifetime if there's a watermark
        let static_watermark = watermark_result.map(|watermark| {
            // Create a new watermark with 'static lifetime
            // This requires cloning any data that depends on lifetimes
            PrunerWatermark::new(
                pipeline.to_string(), // Clone the string to make it 'static
                watermark.sequence_number(),
                watermark.timestamp(),
                // Add any other fields that need to be copied
            )
        });

        Ok(static_watermark)
    }

    async fn update_pruner_watermark(
        &self,
        watermark: &PrunerWatermark<'_>,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        watermark.update(&mut conn).await.map_err(Into::into)
    }
}

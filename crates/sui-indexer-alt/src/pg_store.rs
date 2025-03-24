// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use async_trait::async_trait;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use sui_indexer_alt_metrics::stats::{DbConnectionStats, DbConnectionStatsSnapshot};

use crate::db::{Connection as PgConnection, Db as PgDb};
use crate::store::{
    CommitterWatermark, DbConnection, HandlerBatch, SequentialHandler, Store, TransactionalStore,
};
use sui_indexer_alt_schema::watermarks::{
    PgCommitterWatermark, PgPrunerWatermark, PgReaderWatermark, StoredWatermark,
};

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

    async fn get_committer_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64, i64, i64)>> {
        let mut conn = self.db.connect().await?;
        let watermark = PgCommitterWatermark::get(&mut conn, pipeline)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some((
                watermark.epoch_hi_inclusive,
                watermark.checkpoint_hi_inclusive,
                watermark.tx_hi,
                watermark.timestamp_ms_hi_inclusive,
            )))
        } else {
            Ok(None)
        }
    }

    async fn get_reader_watermark(
        &self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<(i64, i64)>> {
        let mut conn = self.db.connect().await?;
        let watermark = StoredWatermark::get(&mut conn, pipeline)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some((
                watermark.checkpoint_hi_inclusive,
                watermark.reader_lo,
            )))
        } else {
            Ok(None)
        }
    }

    async fn update_committer_watermark(
        &self,
        pipeline: &'static str,
        epoch_hi_inclusive: i64,
        checkpoint_hi_inclusive: i64,
        tx_hi: i64,
        timestamp_ms_hi_inclusive: i64,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        let watermark = PgCommitterWatermark {
            pipeline: pipeline.into(),
            epoch_hi_inclusive,
            checkpoint_hi_inclusive,
            tx_hi,
            timestamp_ms_hi_inclusive,
        };
        watermark.update(&mut conn).await.map_err(Into::into)
    }

    async fn update_reader_watermark(
        &self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        let watermark = PgReaderWatermark {
            pipeline: pipeline.into(),
            reader_lo,
        };
        watermark.update(&mut conn).await.map_err(Into::into)
    }

    async fn get_pruner_watermark(
        &self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<(i64, i64, i64)>> {
        let mut conn = self.db.connect().await?;
        let watermark = PgPrunerWatermark::get(&mut conn, pipeline, delay)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some((
                watermark.pruner_hi,
                watermark.reader_lo,
                watermark.wait_for,
            )))
        } else {
            Ok(None)
        }
    }

    async fn update_pruner_watermark(
        &self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool> {
        let mut conn = self.db.connect().await?;
        let watermark = PgPrunerWatermark {
            pipeline: pipeline.into(),
            pruner_hi,
            // These values are ignored by the update method
            reader_lo: 0,
            wait_for: 0,
        };
        watermark.update(&mut conn).await.map_err(Into::into)
    }
}

#[async_trait]
impl TransactionalStore for PgStore {
    async fn transactional_commit_with_watermark<'a, H>(
        &'a self,
        pipeline: &'static str,
        watermark: &'a CommitterWatermark,
        batch: &'a HandlerBatch<H>,
    ) -> anyhow::Result<usize>
    where
        H: SequentialHandler<Store = Self> + Send + Sync + 'a,
    {
        let mut conn = self.db.connect().await?;

        let result = AsyncConnection::transaction(&mut conn, |conn| {
            async {
                let watermark = PgCommitterWatermark {
                    pipeline: pipeline.into(),
                    epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                    checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                    tx_hi: watermark.tx_hi,
                    timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
                };
                watermark.update(conn).await.map_err(anyhow::Error::from)?;
                H::commit(batch, conn).await
            }
            .scope_boxed()
        })
        .await?;

        Ok(result)
    }
}

impl DbConnectionStats for PgStore {
    fn get_connection_stats(&self) -> DbConnectionStatsSnapshot {
        let state = self.db.state();
        let stats = state.statistics;
        DbConnectionStatsSnapshot {
            connections: state.connections as usize,
            idle_connections: state.idle_connections as usize,
            get_direct: stats.get_direct as u64,
            get_waited: stats.get_waited as u64,
            get_timed_out: stats.get_timed_out as u64,
            get_wait_time_ms: stats.get_wait_time.as_millis() as u64,
            connections_created: stats.connections_created as u64,
            connections_closed_broken: stats.connections_closed_broken,
            connections_closed_invalid: stats.connections_closed_invalid,
            connections_closed_max_lifetime: stats.connections_closed_max_lifetime,
            connections_closed_idle_timeout: stats.connections_closed_idle_timeout,
        }
    }
}

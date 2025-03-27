// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use async_trait::async_trait;
use diesel_async::AsyncConnection;
use scoped_futures::ScopedBoxFuture;

use crate::db::{Connection as PgConnection, Db as PgDb};
use crate::models::watermarks::{
    PgCommitterWatermark, PgPrunerWatermark, PgReaderWatermark, StoredWatermark,
};
use crate::store::{
    CommitterWatermark, DbConnection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};

#[async_trait]
impl<'c> DbConnection for PgConnection<'c> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let watermark = PgCommitterWatermark::get(self, pipeline)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            }))
        } else {
            Ok(None)
        }
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let watermark = StoredWatermark::get(self, pipeline)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(ReaderWatermark {
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
                reader_lo: watermark.reader_lo,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let watermark = PgCommitterWatermark {
            pipeline: pipeline.into(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            tx_hi: watermark.tx_hi,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
        };
        watermark.update(self).await.map_err(Into::into)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: i64,
    ) -> anyhow::Result<bool> {
        let watermark = PgReaderWatermark {
            pipeline: pipeline.into(),
            reader_lo,
        };
        watermark.update(self).await.map_err(Into::into)
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        let watermark = PgPrunerWatermark::get(self, pipeline, delay)
            .await
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(PrunerWatermark {
                pruner_hi: watermark.pruner_hi,
                reader_lo: watermark.reader_lo,
                wait_for: watermark.wait_for,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: i64,
    ) -> anyhow::Result<bool> {
        let watermark = PgPrunerWatermark {
            pipeline: pipeline.into(),
            pruner_hi,
            // These values are ignored by the update method
            reader_lo: 0,
            wait_for: 0,
        };
        watermark.update(self).await.map_err(Into::into)
    }
}

#[async_trait]
impl Store for PgDb {
    type Connection<'c> = PgConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        let conn = self.connect().await?;
        Ok(conn)
    }
}

#[async_trait]
impl TransactionalStore for PgDb {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = self.connect().await?;
        AsyncConnection::transaction(&mut conn, |conn| f(conn)).await
    }
}

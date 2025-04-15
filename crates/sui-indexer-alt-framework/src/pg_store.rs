// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel_async::{AsyncConnection, RunQueryDsl};
use scoped_futures::ScopedBoxFuture;
use sui_sql_macro::sql;

use crate::db::{Connection as PgConnection, Db as PgDb};
use crate::models::watermarks::StoredWatermark;
use crate::schema::watermarks;
use crate::store::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store, TransactionalStore,
};

#[async_trait]
impl Connection for PgConnection<'_> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let watermark = StoredWatermark::get(self, pipeline).await?;

        if let Some(watermark) = watermark {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive as u64,
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as u64,
                tx_hi: watermark.tx_hi as u64,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive as u64,
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
                checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as u64,
                reader_lo: watermark.reader_lo as u64,
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
        // Create a StoredWatermark directly from CommitterWatermark
        let stored_watermark = StoredWatermark {
            pipeline: pipeline.to_string(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive as i64,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as i64,
            tx_hi: watermark.tx_hi as i64,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive as i64,
            reader_lo: 0,
            pruner_timestamp: NaiveDateTime::UNIX_EPOCH,
            pruner_hi: 0,
        };

        use diesel::query_dsl::methods::FilterDsl;
        Ok(diesel::insert_into(watermarks::table)
            .values(&stored_watermark)
            // There is an existing entry, so only write the new `hi` values
            .on_conflict(watermarks::pipeline)
            .do_update()
            .set((
                watermarks::epoch_hi_inclusive.eq(stored_watermark.epoch_hi_inclusive),
                watermarks::checkpoint_hi_inclusive.eq(stored_watermark.checkpoint_hi_inclusive),
                watermarks::tx_hi.eq(stored_watermark.tx_hi),
                watermarks::timestamp_ms_hi_inclusive
                    .eq(stored_watermark.timestamp_ms_hi_inclusive),
            ))
            .filter(
                watermarks::checkpoint_hi_inclusive.lt(stored_watermark.checkpoint_hi_inclusive),
            )
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set((
                watermarks::reader_lo.eq(reader_lo as i64),
                watermarks::pruner_timestamp.eq(diesel::dsl::now),
            ))
            .filter(watermarks::pipeline.eq(pipeline))
            .filter(watermarks::reader_lo.lt(reader_lo as i64))
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        //     |---------- + delay ---------------------|
        //                             |--- wait_for ---|
        //     |-----------------------|----------------|
        //     ^                       ^
        //     pruner_timestamp        NOW()
        let wait_for = sql!(as BigInt,
            "CAST({BigInt} + 1000 * EXTRACT(EPOCH FROM pruner_timestamp - NOW()) AS BIGINT)",
            delay.as_millis() as i64,
        );

        let watermark: Option<(i64, i64, i64)> = watermarks::table
            .select((wait_for, watermarks::pruner_hi, watermarks::reader_lo))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await
            .optional()
            .map_err(anyhow::Error::from)?;

        if let Some(watermark) = watermark {
            Ok(Some(PrunerWatermark {
                wait_for_ms: watermark.0 as u64,
                pruner_hi: watermark.1 as u64,
                reader_lo: watermark.2 as u64,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set(watermarks::pruner_hi.eq(pruner_hi as i64))
            .filter(watermarks::pipeline.eq(pipeline))
            .execute(self)
            .await
            .map_err(anyhow::Error::from)?
            > 0)
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

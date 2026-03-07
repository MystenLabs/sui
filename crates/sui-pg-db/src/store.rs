// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework_store_traits as store;
use sui_sql_macro::sql;

use crate::Connection;
use crate::Db;
use crate::model::StoredWatermark;
use crate::schema::watermarks;

pub use sui_indexer_alt_framework_store_traits::Store;

#[async_trait]
impl store::Connection for Connection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        default_next_checkpoint: u64,
    ) -> anyhow::Result<Option<u64>> {
        let stored_watermark = StoredWatermark {
            pipeline: pipeline_task.to_string(),
            epoch_hi_inclusive: 0,
            checkpoint_hi: default_next_checkpoint as i64,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
            reader_lo: default_next_checkpoint as i64,
            pruner_timestamp: Utc::now().naive_utc(),
            pruner_hi: default_next_checkpoint as i64,
        };

        use diesel::pg::upsert::excluded;
        let checkpoint_hi: i64 = diesel::insert_into(watermarks::table)
            .values(&stored_watermark)
            .on_conflict(watermarks::pipeline)
            .do_update()
            .set(watermarks::pipeline.eq(excluded(watermarks::pipeline)))
            .returning(watermarks::checkpoint_hi)
            .get_result(self)
            .await?;

        Ok(Some(u64::try_from(checkpoint_hi)?))
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<store::CommitterWatermark>> {
        let watermark: Option<(i64, i64, i64, i64)> = watermarks::table
            .select((
                watermarks::epoch_hi_inclusive,
                watermarks::checkpoint_hi,
                watermarks::tx_hi,
                watermarks::timestamp_ms_hi_inclusive,
            ))
            .filter(watermarks::pipeline.eq(pipeline_task))
            .first(self)
            .await
            .optional()?;

        if let Some(watermark) = watermark {
            Ok(Some(store::CommitterWatermark {
                epoch_hi_inclusive: u64::try_from(watermark.0)?,
                checkpoint_hi: u64::try_from(watermark.1)?,
                tx_hi: u64::try_from(watermark.2)?,
                timestamp_ms_hi_inclusive: u64::try_from(watermark.3)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<store::ReaderWatermark>> {
        let watermark: Option<(i64, i64)> = watermarks::table
            .select((watermarks::checkpoint_hi, watermarks::reader_lo))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await
            .optional()?;

        if let Some(watermark) = watermark {
            Ok(Some(store::ReaderWatermark {
                checkpoint_hi: u64::try_from(watermark.0)?,
                reader_lo: u64::try_from(watermark.1)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<store::PrunerWatermark>> {
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
            .optional()?;

        if let Some(watermark) = watermark {
            Ok(Some(store::PrunerWatermark {
                wait_for_ms: watermark.0,
                pruner_hi: u64::try_from(watermark.1)?,
                reader_lo: u64::try_from(watermark.2)?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: store::CommitterWatermark,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set((
                watermarks::epoch_hi_inclusive.eq(watermark.epoch_hi_inclusive as i64),
                watermarks::checkpoint_hi.eq(watermark.checkpoint_hi as i64),
                watermarks::tx_hi.eq(watermark.tx_hi as i64),
                watermarks::timestamp_ms_hi_inclusive
                    .eq(watermark.timestamp_ms_hi_inclusive as i64),
            ))
            .filter(watermarks::pipeline.eq(pipeline_task))
            .filter(watermarks::checkpoint_hi.lt(watermark.checkpoint_hi as i64))
            .execute(self)
            .await?
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
            .await?
            > 0)
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
            .await?
            > 0)
    }
}

#[async_trait]
impl store::Store for Db {
    type Connection<'c> = Connection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        self.connect().await
    }
}

#[async_trait]
impl store::TransactionalStore for Db {
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel::sql_types::Nullable;
use diesel::sql_types::SingleValue;
use diesel::sql_types::SqlType;
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

define_sql_function! {
    fn coalesce<T: SqlType + SingleValue>(x: Nullable<T>, y: T) -> Nullable<T>;
}

#[async_trait]
impl store::Connection for Connection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<store::InitWatermark>> {
        let checkpoint_hi_inclusive = checkpoint_hi_inclusive.map_or(-1, |c| c as i64);
        let stored_watermark = StoredWatermark::for_init(
            pipeline_task,
            checkpoint_hi_inclusive,
            checkpoint_hi_inclusive + 1,
        );

        use diesel::pg::upsert::excluded;
        let (checkpoint_hi_inclusive, reader_lo): (i64, i64) =
            diesel::insert_into(watermarks::table)
                .values(&stored_watermark)
                // If there is an existing row, return it without updating it.
                .on_conflict(watermarks::pipeline)
                // Use `do_update` instead of `do_nothing` to return the existing row with `returning`.
                .do_update()
                // When using `do_update`, at least one change needs to be set, so set the pipeline to itself (nothing changes).
                // `excluded` is a virtual table containing the existing row that there was a conflict with.
                .set(watermarks::pipeline.eq(excluded(watermarks::pipeline)))
                .returning((watermarks::checkpoint_hi_inclusive, watermarks::reader_lo))
                .get_result(self)
                .await?;

        Ok(Some(store::InitWatermark {
            checkpoint_hi_inclusive: u64::try_from(checkpoint_hi_inclusive).ok(),
            reader_lo: Some(reader_lo as u64),
        }))
    }

    async fn accepts_chain_id(
        &mut self,
        pipeline_task: &str,
        chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        let stored_chain_id: Option<Vec<u8>> = diesel::update(watermarks::table)
            .filter(watermarks::pipeline.eq(pipeline_task))
            // "coalesce" only updates the value if it is null in the existing row
            .set(watermarks::chain_id.eq(coalesce(watermarks::chain_id, chain_id)))
            .returning(watermarks::chain_id)
            .get_result(self)
            .await?;

        let stored_chain_id = stored_chain_id.context("missing chain id after update")?;
        let stored_chain_id: [u8; 32] = stored_chain_id
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("chain id has wrong length: {}", v.len()))?;
        Ok(stored_chain_id == chain_id)
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<store::CommitterWatermark>> {
        let (
            epoch_hi_inclusive,
            checkpoint_hi_inclusive,
            tx_hi,
            timestamp_ms_hi_inclusive,
            reader_lo,
        ): (i64, i64, i64, i64, i64) = watermarks::table
            .select((
                watermarks::epoch_hi_inclusive,
                watermarks::checkpoint_hi_inclusive,
                watermarks::tx_hi,
                watermarks::timestamp_ms_hi_inclusive,
                watermarks::reader_lo,
            ))
            .filter(watermarks::pipeline.eq(pipeline_task))
            .first(self)
            .await?;

        Ok(
            (reader_lo <= checkpoint_hi_inclusive).then_some(store::CommitterWatermark {
                epoch_hi_inclusive: epoch_hi_inclusive as u64,
                checkpoint_hi_inclusive: checkpoint_hi_inclusive as u64,
                tx_hi: tx_hi as u64,
                timestamp_ms_hi_inclusive: timestamp_ms_hi_inclusive as u64,
            }),
        )
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: store::CommitterWatermark,
    ) -> anyhow::Result<bool> {
        Ok(diesel::update(watermarks::table)
            .set((
                watermarks::epoch_hi_inclusive.eq(watermark.epoch_hi_inclusive as i64),
                watermarks::checkpoint_hi_inclusive.eq(watermark.checkpoint_hi_inclusive as i64),
                watermarks::tx_hi.eq(watermark.tx_hi as i64),
                watermarks::timestamp_ms_hi_inclusive
                    .eq(watermark.timestamp_ms_hi_inclusive as i64),
            ))
            .filter(watermarks::pipeline.eq(pipeline_task))
            .filter(
                watermarks::checkpoint_hi_inclusive.lt(watermark.checkpoint_hi_inclusive as i64),
            )
            .execute(self)
            .await?
            > 0)
    }
}

#[async_trait]
impl store::ConcurrentConnection for Connection<'_> {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> anyhow::Result<Option<store::ReaderWatermark>> {
        let (checkpoint_hi_inclusive, reader_lo): (i64, i64) = watermarks::table
            .select((watermarks::checkpoint_hi_inclusive, watermarks::reader_lo))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await?;

        Ok(
            (reader_lo <= checkpoint_hi_inclusive).then_some(store::ReaderWatermark {
                checkpoint_hi_inclusive: checkpoint_hi_inclusive as u64,
                reader_lo: reader_lo as u64,
            }),
        )
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

        let (wait_for_ms, pruner_hi, reader_lo): (i64, i64, i64) = watermarks::table
            .select((wait_for, watermarks::pruner_hi, watermarks::reader_lo))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(self)
            .await?;

        Ok(Some(store::PrunerWatermark {
            wait_for_ms,
            pruner_hi: pruner_hi as u64,
            reader_lo: reader_lo as u64,
        }))
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
impl store::SequentialConnection for Connection<'_> {}

#[async_trait]
impl store::Store for Db {
    type Connection<'c> = Connection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        self.connect().await
    }
}

#[async_trait]
impl store::ConcurrentStore for Db {
    type ConcurrentConnection<'c> = Connection<'c>;
}

#[async_trait]
impl store::SequentialStore for Db {
    type SequentialConnection<'c> = Connection<'c>;
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

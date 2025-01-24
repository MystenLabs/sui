// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Cow, time::Duration};

use chrono::{naive::NaiveDateTime, DateTime, Utc};
use diesel::{dsl::sql, prelude::*, sql_types};
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_pg_db::Connection;

use crate::schema::watermarks;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub(crate) struct StoredWatermark {
    pub pipeline: String,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
    pub reader_lo: i64,
    pub pruner_timestamp: NaiveDateTime,
    pub pruner_hi: i64,
}

/// Fields that the committer is responsible for setting.
#[derive(AsChangeset, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub(crate) struct CommitterWatermark<'p> {
    pub pipeline: Cow<'p, str>,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
}

#[derive(AsChangeset, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub(crate) struct ReaderWatermark<'p> {
    pub pipeline: Cow<'p, str>,
    pub reader_lo: i64,
}

#[derive(Queryable, Debug, Clone, FieldCount, PartialEq, Eq)]
#[diesel(table_name = watermarks)]
pub(crate) struct PrunerWatermark<'p> {
    /// The pipeline in question
    pub pipeline: Cow<'p, str>,

    /// How long to wait from when this query ran on the database until this information can be
    /// used to prune the database. This number could be negative, meaning no waiting is necessary.
    pub wait_for: i64,

    /// The pruner can delete up to this checkpoint, (exclusive).
    pub reader_lo: i64,

    /// The pruner has already deleted up to this checkpoint (exclusive), so can continue from this
    /// point.
    pub pruner_hi: i64,
}

impl StoredWatermark {
    pub(crate) async fn get(
        conn: &mut Connection<'_>,
        pipeline: &'static str,
    ) -> QueryResult<Option<Self>> {
        watermarks::table
            .select(StoredWatermark::as_select())
            .filter(watermarks::pipeline.eq(pipeline))
            .first(conn)
            .await
            .optional()
    }
}

impl CommitterWatermark<'static> {
    /// Get the current high watermark for the pipeline.
    pub(crate) async fn get(
        conn: &mut Connection<'_>,
        pipeline: &'static str,
    ) -> QueryResult<Option<Self>> {
        watermarks::table
            .select(CommitterWatermark::as_select())
            .filter(watermarks::pipeline.eq(pipeline))
            .first(conn)
            .await
            .optional()
    }
}

impl<'p> CommitterWatermark<'p> {
    /// A new watermark with the given pipeline name indicating zero progress.
    pub(crate) fn initial(pipeline: Cow<'p, str>) -> Self {
        CommitterWatermark {
            pipeline,
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 0,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_testing(pipeline: &'p str, checkpoint_hi_inclusive: u64) -> Self {
        CommitterWatermark {
            pipeline: pipeline.into(),
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: checkpoint_hi_inclusive as i64,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }
    }

    /// The consensus timestamp associated with this checkpoint.
    pub(crate) fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive).unwrap_or_default()
    }

    /// Upsert the high watermark as long as it raises the watermark stored in the database.
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    ///
    /// TODO(amnn): Test this (depends on supporting migrations and tempdb).
    pub(crate) async fn update(&self, conn: &mut Connection<'_>) -> QueryResult<bool> {
        use diesel::query_dsl::methods::FilterDsl;
        Ok(diesel::insert_into(watermarks::table)
            .values(StoredWatermark::from(self.clone()))
            .on_conflict(watermarks::pipeline)
            .do_update()
            .set(self)
            .filter(watermarks::checkpoint_hi_inclusive.lt(self.checkpoint_hi_inclusive))
            .execute(conn)
            .await?
            > 0)
    }
}

impl<'p> ReaderWatermark<'p> {
    pub(crate) fn new(pipeline: impl Into<Cow<'p, str>>, reader_lo: u64) -> Self {
        ReaderWatermark {
            pipeline: pipeline.into(),
            reader_lo: reader_lo as i64,
        }
    }

    /// Update the reader low watermark for an existing watermark row, as long as this raises the
    /// watermark, and updates the timestamp this update happened to the database's current time.
    ///
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    pub(crate) async fn update(&self, conn: &mut Connection<'_>) -> QueryResult<bool> {
        Ok(diesel::update(watermarks::table)
            .set((self, watermarks::pruner_timestamp.eq(diesel::dsl::now)))
            .filter(watermarks::pipeline.eq(&self.pipeline))
            .filter(watermarks::reader_lo.lt(self.reader_lo))
            .execute(conn)
            .await?
            > 0)
    }
}

impl PrunerWatermark<'static> {
    /// Get the bounds for the region that the pruner still has to prune for the given `pipeline`,
    /// along with a duration to wait before acting on this information, based on the time at which
    /// the pruner last updated the bounds, and the configured `delay`.
    ///
    /// The pruner is allowed to prune the region between the returned `pruner_hi` (inclusive) and
    /// `reader_lo` (exclusive) after `wait_for` milliseconds have passed since this response was
    /// returned.
    pub(crate) async fn get(
        conn: &mut Connection<'_>,
        pipeline: &'static str,
        delay: Duration,
    ) -> QueryResult<Option<Self>> {
        //     |---------- + delay ---------------------|
        //                             |--- wait_for ---|
        //     |-----------------------|----------------|
        //     ^                       ^
        //     pruner_timestamp        NOW()
        let wait_for = sql::<sql_types::BigInt>(&format!(
            "CAST({} + 1000 * EXTRACT(EPOCH FROM pruner_timestamp - NOW()) AS BIGINT)",
            delay.as_millis(),
        ));

        watermarks::table
            .select((
                watermarks::pipeline,
                wait_for,
                watermarks::reader_lo,
                watermarks::pruner_hi,
            ))
            .filter(watermarks::pipeline.eq(pipeline))
            .first(conn)
            .await
            .optional()
    }
}

impl<'p> PrunerWatermark<'p> {
    #[cfg(test)]
    pub(crate) fn new_for_testing(pipeline: &'p str, pruner_hi: u64) -> Self {
        PrunerWatermark {
            pipeline: pipeline.into(),
            wait_for: 0,
            reader_lo: 0,
            pruner_hi: pruner_hi as i64,
        }
    }

    /// How long to wait before the pruner can act on this information, or `None`, if there is no
    /// need to wait.
    pub(crate) fn wait_for(&self) -> Option<Duration> {
        (self.wait_for > 0).then(|| Duration::from_millis(self.wait_for as u64))
    }

    /// The next chunk of checkpoints that the pruner should work on, to advance the watermark.
    /// If no more checkpoints to prune, returns `None`.
    /// Otherwise, returns a tuple (from, to_exclusive) where `from` is inclusive and `to_exclusive` is exclusive.
    /// Advance the watermark as well.
    pub(crate) fn next_chunk(&mut self, size: u64) -> Option<(u64, u64)> {
        if self.pruner_hi >= self.reader_lo {
            return None;
        }

        let from = self.pruner_hi as u64;
        let to_exclusive = (from + size).min(self.reader_lo as u64);
        self.pruner_hi = to_exclusive as i64;
        Some((from, to_exclusive))
    }

    /// Update the pruner high watermark (only) for an existing watermark row, as long as this
    /// raises the watermark.
    ///
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    pub(crate) async fn update(&self, conn: &mut Connection<'_>) -> QueryResult<bool> {
        Ok(diesel::update(watermarks::table)
            .set(watermarks::pruner_hi.eq(self.pruner_hi))
            .filter(watermarks::pipeline.eq(&self.pipeline))
            .execute(conn)
            .await?
            > 0)
    }
}

impl<'p> From<CommitterWatermark<'p>> for StoredWatermark {
    fn from(watermark: CommitterWatermark<'p>) -> Self {
        StoredWatermark {
            pipeline: watermark.pipeline.into_owned(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            tx_hi: watermark.tx_hi,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            reader_lo: 0,
            pruner_timestamp: NaiveDateTime::UNIX_EPOCH,
            pruner_hi: 0,
        }
    }
}

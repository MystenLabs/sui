// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Cow, cmp};

use crate::{db::Connection, schema::watermarks};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = watermarks)]
pub struct StoredWatermark {
    pub pipeline: String,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub epoch_lo: i64,
    pub reader_lo: i64,
    pub timestamp_ms: i64,
    pub pruner_hi: i64,
}

/// Fields that the committer is responsible for setting.
#[derive(AsChangeset, Selectable, Queryable, Debug, Clone)]
#[diesel(table_name = watermarks)]
pub struct CommitterWatermark<'p> {
    pub pipeline: Cow<'p, str>,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
}

/// Outcomes from extending one watermark with another.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Ordering {
    /// The watermark was in the future, so could not be added.
    Future,

    /// The added watermark was in the past, so the current watermark didn't change.
    Past,

    /// The added watermark was the successor to the current watermark, so was used in the update.
    Next,
}

impl CommitterWatermark<'static> {
    /// Get the current high watermark for the pipeline.
    pub async fn get(
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
    pub fn initial(pipeline: Cow<'p, str>) -> Self {
        CommitterWatermark {
            pipeline,
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 0,
            tx_hi: 0,
        }
    }

    /// Upsert the high watermark as long as it raises the watermark stored in the database.
    /// Returns a boolean indicating whether the watermark was actually updated or not.
    ///
    /// TODO(amnn): Test this (depends on supporting migrations and tempdb).
    pub async fn update(&self, conn: &mut Connection<'_>) -> QueryResult<bool> {
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

    /// Compare `other` with the immediate successor of this watermark.
    pub fn next_cmp(&self, other: &CommitterWatermark<'_>) -> Ordering {
        let next = self.checkpoint_hi_inclusive + 1;
        match other.checkpoint_hi_inclusive.cmp(&next) {
            cmp::Ordering::Equal => Ordering::Next,
            cmp::Ordering::Less => Ordering::Past,
            cmp::Ordering::Greater => Ordering::Future,
        }
    }
}

impl<'p> From<CommitterWatermark<'p>> for StoredWatermark {
    fn from(watermark: CommitterWatermark<'p>) -> Self {
        StoredWatermark {
            pipeline: watermark.pipeline.into_owned(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            tx_hi: watermark.tx_hi,
            epoch_lo: 0,
            reader_lo: 0,
            timestamp_ms: 0,
            pruner_hi: 0,
        }
    }
}

// Ordering for watermarks is driven solely by their checkpoints.

impl PartialEq for CommitterWatermark<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.checkpoint_hi_inclusive == other.checkpoint_hi_inclusive
    }
}

impl Eq for CommitterWatermark<'_> {}

impl Ord for CommitterWatermark<'_> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.checkpoint_hi_inclusive
            .cmp(&other.checkpoint_hi_inclusive)
    }
}

impl PartialOrd for CommitterWatermark<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use std::borrow::Cow;

use chrono::naive::NaiveDateTime;
use sui_field_count::FieldCount;

use crate::schema::watermarks;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub struct StoredWatermark {
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
#[derive(AsChangeset, Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub struct PgCommitterWatermark<'p> {
    pub pipeline: Cow<'p, str>,
    pub epoch_hi_inclusive: i64,
    pub checkpoint_hi_inclusive: i64,
    pub tx_hi: i64,
    pub timestamp_ms_hi_inclusive: i64,
}

#[derive(AsChangeset, Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = watermarks)]
pub struct PgReaderWatermark<'p> {
    pub pipeline: Cow<'p, str>,
    pub reader_lo: i64,
}

#[derive(Queryable, Debug, Clone, FieldCount, PartialEq, Eq)]
#[diesel(table_name = watermarks)]
pub struct PgPrunerWatermark<'p> {
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

impl<'p> From<PgCommitterWatermark<'p>> for StoredWatermark {
    fn from(watermark: PgCommitterWatermark<'p>) -> Self {
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

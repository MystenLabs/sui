// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_field_count::FieldCount;

use chrono::NaiveDateTime;
use diesel::prelude::*;

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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDateTime;
use chrono::Utc;
use diesel::prelude::*;
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
    pub chain_id: Option<Vec<u8>>,
}

impl StoredWatermark {
    pub fn for_init(pipeline: &str, checkpoint_hi_inclusive: i64, reader_lo: i64) -> Self {
        Self {
            pipeline: pipeline.to_string(),
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
            reader_lo,
            pruner_timestamp: Utc::now().naive_utc(),
            pruner_hi: reader_lo,
            chain_id: None,
        }
    }
}

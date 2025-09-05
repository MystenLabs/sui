// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_sequence_numbers;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = cp_sequence_numbers)]
pub struct StoredCpSequenceNumbers {
    pub cp_sequence_number: i64,
    pub tx_lo: i64,
    pub epoch: i64,
}

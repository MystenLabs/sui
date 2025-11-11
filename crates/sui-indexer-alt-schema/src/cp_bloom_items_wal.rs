// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_bloom_items_wal;

/// Write-Ahead Log for checkpoint bloom items.
/// Accumulates items across multiple commits until enough checkpoints are available.
#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount)]
#[diesel(table_name = cp_bloom_items_wal)]
pub struct StoredCpBloomItemsWal {
    pub cp_block_id: i64,
    pub cp_sequence_number: i64,
    pub items: Vec<Option<Vec<u8>>>,
}

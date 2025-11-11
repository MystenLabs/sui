// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_blooms;

/// Number of bits in the checkpoint bloom filter (131,072 bits = 16KB before folding)
pub const CP_BLOOM_NUM_BITS: usize = 131_072;
/// Number of hash functions for checkpoint bloom filter
pub const CP_BLOOM_NUM_HASHES: u32 = 6;
/// Global seed for checkpointbloom filter hashing
pub const BLOOM_FILTER_SEED: u128 = 67;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount, QueryableByName)]
#[diesel(table_name = cp_blooms)]
pub struct StoredCpBlooms {
    pub cp_sequence_number: i64,
    pub bloom_filter: Vec<u8>,
    pub num_items: Option<i64>,
}

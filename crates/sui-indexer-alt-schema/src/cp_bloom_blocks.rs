// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_bloom_blocks;

/// Number of checkpoints per checkpoint block.
pub const CP_BLOCK_SIZE: u64 = 1000;

/// Compute the checkpoint block ID for a given checkpoint number.
pub fn cp_block_id(cp_num: u64) -> i64 {
    (cp_num / CP_BLOCK_SIZE) as i64
}

/// Compute the seed for a checkpoint block (unique per block to avoid hot spots).
pub fn cp_block_seed(cp_block_id: i64) -> u128 {
    cp_block_id as u128
}

/// Stored bloom block in the database (one row per bloom block per checkpoint block).
#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount, QueryableByName)]
#[diesel(table_name = cp_bloom_blocks)]
pub struct StoredCpBloomBlock {
    /// Checkpoint block ID (cp_num / CP_BLOCK_SIZE).
    pub cp_block_id: i64,
    /// Index of this bloom block within the 128-block filter (0-127).
    pub bloom_block_index: i16,
    /// Lowest checkpoint number included in this block's bloom filter.
    pub cp_sequence_number_lo: i64,
    /// Highest checkpoint number included in this block's bloom filter.
    pub cp_sequence_number_hi: i64,
    /// Bloom filter bytes for this block.
    pub bloom_filter: Vec<u8>,
    /// Approximate count of items (may overcount due to merges).
    pub num_items: Option<i64>,
}

/// Temporary struct to hold checkpoint items before building bloom filter.
/// This is what the processor returns - the framework batches these before commit().
#[derive(Clone, Debug, FieldCount)]
pub struct CheckpointItems {
    pub cp_sequence_number: i64,
    pub items: Vec<Vec<u8>>,
}

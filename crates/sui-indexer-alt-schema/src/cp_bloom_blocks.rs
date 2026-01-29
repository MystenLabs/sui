// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::define_sql_function;
use diesel::prelude::*;
use diesel::sql_types::Binary;
use sui_field_count::FieldCount;

use crate::blooms::blocked::BlockedBloomFilter;
use crate::schema::cp_bloom_blocks;

define_sql_function! {
    /// Performs bitwise OR on two bytea values. Used for merging bloom filters.
    fn function_bytea_or(a: Binary, b: Binary) -> Binary;
}

/// Number of checkpoints per checkpoint block.
pub const CP_BLOCK_SIZE: u64 = 1000;

/// Size of each bloom block in bytes.
pub const BLOOM_BLOCK_BYTES: usize = 2048;

/// Number of bits per bloom block.
pub const BLOOM_BLOCK_BITS: usize = BLOOM_BLOCK_BYTES * 8;

/// Number of blocks in the bloom filter (stored as separate database rows).
pub const NUM_BLOOM_BLOCKS: usize = 128;

/// Total bits in the bloom filter (256KB).
pub const TOTAL_BLOOM_BITS: usize = NUM_BLOOM_BLOCKS * BLOOM_BLOCK_BITS;

/// Number of hash functions (k) used per key.
pub const NUM_HASHES: u32 = 5;

/// Blocked bloom filter with checkpoint block dimensions.
pub type CpBlockedBloomFilter = BlockedBloomFilter<BLOOM_BLOCK_BYTES, NUM_BLOOM_BLOCKS, NUM_HASHES>;

/// Stored bloom block in the database (one row per bloom block per checkpoint block).
#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount, QueryableByName)]
#[diesel(table_name = cp_bloom_blocks)]
pub struct StoredCpBloomBlock {
    /// Checkpoint block ID (cp_num / CP_BLOCK_SIZE).
    pub cp_block_index: i64,
    /// Index of this bloom block within the 128-block filter (0-127).
    pub bloom_block_index: i16,
    /// Bloom filter bytes for this block.
    pub bloom_filter: Vec<u8>,
}

/// The block a checkpoint belongs to. Checkpoints in a block share the same bloom filter and the block
/// id is used as the seed for the blocked bloom filter hash functions.
pub fn cp_block_index(cp_num: u64) -> i64 {
    (cp_num / CP_BLOCK_SIZE) as i64
}

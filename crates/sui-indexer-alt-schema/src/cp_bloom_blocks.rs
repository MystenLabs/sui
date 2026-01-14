// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_bloom_blocks;

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
}

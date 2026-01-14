// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::cp_blooms;

/// Size of the checkpoint bloom filter in bytes (16KB before folding).
pub const CP_BLOOM_NUM_BYTES: usize = 16_384;

/// Number of bits in the checkpoint bloom filter.
pub const CP_BLOOM_NUM_BITS: usize = CP_BLOOM_NUM_BYTES * 8;

/// Number of hash functions for checkpoint bloom filter.
pub const CP_BLOOM_NUM_HASHES: u32 = 6;

/// Global seed for checkpoint bloom filter hashing.
pub const BLOOM_FILTER_SEED: u128 = 67;

/// Minimum size after folding (1024 bytes = 1KB).
///
/// This prevents over-folding which causes correlated bits (from common items like
/// popular packages) to concentrate and create hot spots with high false positive rates.
pub const MIN_FOLD_BYTES: usize = 1024;

/// Stop folding when bit density exceeds this threshold.
pub const MAX_FOLD_DENSITY: f64 = 0.40;

#[derive(Insertable, Selectable, Queryable, Debug, Clone, FieldCount, QueryableByName)]
#[diesel(table_name = cp_blooms)]
pub struct StoredCpBlooms {
    /// Checkpoint sequence number.
    pub cp_sequence_number: i64,
    /// Folded bloom filter bytes.
    pub bloom_filter: Vec<u8>,
}

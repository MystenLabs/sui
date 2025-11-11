// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! # Bloom Filter Module
//!
//! Provides bloom filter implementations optimized for checkpoint-based transaction scanning.
//!
//! ## Filter Types
//!
//! - [`BloomFilter`]: Standard bloom filter with optional folding compression
//! - [`BlockedBloomFilter`]: I/O-optimized filter with 128 separately-stored blocks
//!
//! ## Usage
//!
//! Bloom filters are used in two stages of transaction scanning:
//! 1. **Stage 1 (Blocked)**: Query `cp_bloom_blocks` to find candidate checkpoint ranges
//! 2. **Stage 2 (Standard)**: Query `cp_blooms` to narrow down to specific checkpoints
//!
//! ## Hashing
//!
//! All filters use the fastbloom-style single-hash approach with SipHash-1-3,
//! deriving multiple bit positions from a single hash call for efficiency.
//!
//! Reference: <https://github.com/tomtomwombat/fastbloom>

pub mod blocked;
pub mod bloom;
pub mod hash;

// Re-exports for convenience
pub use blocked::{
    BLOOM_BLOCK_BITS, BLOOM_BLOCK_BYTES, BlockedBloomFilter, NUM_BLOOM_BLOCKS, NUM_HASHES,
    TOTAL_BLOOM_BITS, compute_key_hash_positions,
};
pub use bloom::{BloomFilter, MAX_FOLD_DENSITY, MIN_FOLD_BITS, compute_original_bit_positions};

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Double hashing for bloom filters.
//!
//! a single SipHash-1-3 call produces one 64-bit hash, which is split
//! into h1 and h2 components. Subsequent positions use
//! double hashing with rotation: `h1 = h1 + h2; rotate_left(5)`.
//!
//! Reference: <https://github.com/tomtomwombat/fastbloom>

use siphasher::sip::SipHasher13;
use std::hash::Hasher;

/// Constant for deriving h2 from upper bits of hash.
///
/// Chosen as 2^64 / π for a large number with mixed bits for good distribution.
const H2_MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;

/// Compute bit positions for a key in a bloom filter.
pub fn compute_positions(key: &[u8], num_bits: usize, num_hashes: u32, seed: u128) -> Vec<usize> {
    let (mut h1, h2) = compute_hashes(key, seed);
    let mut positions = Vec::with_capacity(num_hashes as usize);
    for _ in 0..num_hashes {
        positions.push((h1 as usize) % num_bits);
        h1 = h1.wrapping_add(h2).rotate_left(5);
    }
    positions
}

/// Compute block index and bit positions within that block.
pub fn compute_blocked_positions(
    key: &[u8],
    num_blocks: usize,
    bits_per_block: usize,
    num_hashes: u32,
    seed: u128,
) -> (usize, Vec<usize>) {
    // Block selection with base seed
    let block_idx = compute_block_index(key, num_blocks, seed);

    // Position generation with seed+1
    let seed2 = seed.wrapping_add(1);
    let (mut h1, h2) = compute_hashes(key, seed2);

    let mut positions = Vec::with_capacity(num_hashes as usize);
    for _ in 0..num_hashes {
        positions.push((h1 as usize) % bits_per_block);
        h1 = h1.wrapping_add(h2).rotate_left(5);
    }

    (block_idx, positions)
}

/// Compute block index for blocked bloom filters.
pub(super) fn compute_block_index(key: &[u8], num_blocks: usize, seed: u128) -> usize {
    let mut hasher = SipHasher13::new_with_keys(seed as u64, (seed >> 64) as u64);
    hasher.write(key);
    (hasher.finish() as usize) % num_blocks
}

/// Compute base hashes from a single SipHash call.
///
/// Returns (h1, h2) derived from one hash:
/// - h1: full 64-bit hash value
/// - h2: upper 32 bits multiplied by H2_MULTIPLIER for good distribution across 64 bits.
fn compute_hashes(key: &[u8], seed: u128) -> (u64, u64) {
    let mut hasher = SipHasher13::new_with_keys(seed as u64, (seed >> 64) as u64);
    hasher.write(key);
    let hash = hasher.finish();

    let h1 = hash;
    let h2 = hash.wrapping_shr(32).wrapping_mul(H2_MULTIPLIER);
    (h1, h2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hashes_deterministic() {
        let key = b"test_key";
        let seed = 42u128;

        let (h1a, h2a) = compute_hashes(key, seed);
        let (h1b, h2b) = compute_hashes(key, seed);

        assert_eq!(h1a, h1b);
        assert_eq!(h2a, h2b);
    }

    #[test]
    fn test_different_seeds_produce_different_hashes() {
        let key = b"test_key";

        let (h1a, h2a) = compute_hashes(key, 1);
        let (h1b, h2b) = compute_hashes(key, 2);

        assert_ne!(h1a, h1b);
        assert_ne!(h2a, h2b);
    }

    #[test]
    fn test_compute_positions_count() {
        let key = b"test_key";
        let positions = compute_positions(key, 8192, 5, 42);
        assert_eq!(positions.len(), 5);
    }

    #[test]
    fn test_compute_positions_within_bounds() {
        let key = b"test_key";
        let num_bits = 8192;
        let positions = compute_positions(key, num_bits, 5, 42);

        for pos in positions {
            assert!(pos < num_bits);
        }
    }

    #[test]
    fn test_compute_block_index_within_bounds() {
        let key = b"test_key";
        let num_blocks = 128;

        for seed in 0..100 {
            let block_idx = compute_block_index(key, num_blocks, seed as u128);
            assert!(block_idx < num_blocks);
        }
    }

    #[test]
    fn test_compute_blocked_positions() {
        let key = b"test_key";
        let num_blocks = 128;
        let bits_per_block = 16384;
        let num_hashes = 5;
        let seed = 42u128;

        let (block_idx, positions) =
            compute_blocked_positions(key, num_blocks, bits_per_block, num_hashes, seed);

        assert!(block_idx < num_blocks);
        assert_eq!(positions.len(), num_hashes as usize);
        for pos in positions {
            assert!(pos < bits_per_block);
        }
    }

    #[test]
    fn test_h2_distribution() {
        // Verify that h2 values have good distribution
        let num_samples = 10000;
        let seed = 9995u128;

        let mut h2_values = Vec::new();
        for i in 0..num_samples {
            let key = format!("test_key_{}", i).into_bytes();
            let (_, h2) = compute_hashes(&key, seed);
            h2_values.push(h2);
        }

        let min_h2 = h2_values.iter().min().unwrap();
        let max_h2 = h2_values.iter().max().unwrap();
        let range = max_h2.wrapping_sub(*min_h2);

        // Range should be large (good distribution across 64-bit space)
        assert!(
            range > u64::MAX / 2,
            "h2 range too small: {} (expected > {})",
            range,
            u64::MAX / 2
        );
    }
}

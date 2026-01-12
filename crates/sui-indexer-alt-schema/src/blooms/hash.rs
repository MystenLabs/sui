// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Double hashing for bloom filters.
//!
//! Bloom filters need k independent hash functions to set k bit positions per element.
//! Rather than computing k separate cryptographic hashes, double hashing
//! generates k positions from just two hash values without any loss to
//! asymptotic false positive rate. [Kirsch-Mitzenmacher 2006]
//!
//! This hasher takes a single SipHash-1-3 call to produce one 64-bit hash, which is split into h1 and h2
//! components. Subsequent positions use enhanced double hashing with rotation. [fastbloom]
//!
//! [Kirsch-Mitzenmacher 2006]: https://www.eecs.harvard.edu/~michaelm/postscripts/esa2006a.pdf
//! [fastbloom]: https://github.com/tomtomwombat/fastbloom
use std::hash::Hasher;

use siphasher::sip::SipHasher13;

/// Constant for deriving h2 from upper bits of hash.
///
/// Chosen as 2^64 / π for a large number with mixed bits for good distribution.
const H2_MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;

/// Double hasher for bloom filter position generation.
///
/// Produces an infinite sequence of hash values derived from a single
/// source hash using the double hashing technique.
#[derive(Clone, Copy)]
pub struct DoubleHasher {
    h1: u64,
    h2: u64,
}

impl DoubleHasher {
    /// Creates a DoubleHasher with two 64-bit hashes (h1, h2) from h1. Derives h2 by taking the upper 32 bits of h1 and multiplying it with
    ///  H2_MULTIPLIER, a large number with mixed bits, to distribute the bits across h2.
    #[inline]
    pub fn new(h1: u64) -> Self {
        let h2 = h1.wrapping_shr(32).wrapping_mul(H2_MULTIPLIER);
        Self { h1, h2 }
    }

    /// Creates a DoubleHasher from a byte value we want to insert or check in the bloom filter (e.g., an object ID).
    ///
    /// Entry point for bloom filter operations. Iterate through the DoubleHasher to produce the index of the block and the bit positions set in that block for a value and seed so we can
    /// - set bit positions in a bloom filter for a value
    /// - check if a value is in the bloom filter
    ///
    #[inline]
    pub fn with_value(value: &[u8], seed: u128) -> Self {
        let mut hasher = SipHasher13::new_with_keys(seed as u64, (seed >> 64) as u64);
        hasher.write(value);
        Self::new(hasher.finish())
    }

    /// Generate a new hash value from the current h1 and h2 with:
    /// - modulo addition of two independent hashes with well distributed bits to ensure inputs diverge into uncorrelated bits.
    /// - circular left shift by 5 (coprime to 64) to avoid evenly spaced distributions by mixing high and low bits after the addition.
    #[inline]
    pub fn next_hash(&mut self) -> u64 {
        self.h1 = self.h1.wrapping_add(self.h2).rotate_left(5);
        self.h1
    }
}

impl Iterator for DoubleHasher {
    type Item = u64;

    #[inline]
    fn next(&mut self) -> Option<u64> {
        Some(self.next_hash())
    }
}

#[inline]
pub fn set_bit(bits: &mut [u8], pos: usize) {
    bits[pos / 8] |= 1 << (pos % 8);
}

#[inline]
pub fn check_bit(bits: &[u8], pos: usize) -> bool {
    bits[pos / 8] & (1 << (pos % 8)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_hasher_deterministic() {
        let value = b"test_key";
        let seed = 42u128;

        let mut hasher_a = DoubleHasher::with_value(value, seed);
        let mut hasher_b = DoubleHasher::with_value(value, seed);

        for _ in 0..10 {
            assert_eq!(hasher_a.next_hash(), hasher_b.next_hash());
        }
    }

    #[test]
    fn test_different_seeds_produce_different_hashes() {
        let value = b"test_value";

        let mut hasher_a = DoubleHasher::with_value(value, 1);
        let mut hasher_b = DoubleHasher::with_value(value, 2);

        assert_ne!(hasher_a.next_hash(), hasher_b.next_hash());
    }

    #[test]
    fn test_double_hasher_iterator() {
        let value = b"test_value";
        let num_bits = 8192;
        let num_hashes = 5;

        let positions: Vec<usize> = DoubleHasher::with_value(value, 42)
            .take(num_hashes)
            .map(|h| (h as usize) % num_bits)
            .collect();

        assert_eq!(positions.len(), num_hashes);
        for pos in positions {
            assert!(pos < num_bits);
        }
    }

    #[test]
    fn test_first_hash_as_block_index() {
        let value = b"test_value";
        let num_blocks = 128;

        for seed in 0..100 {
            let mut hasher = DoubleHasher::with_value(value, seed as u128);
            let block_idx = (hasher.next_hash() as usize) % num_blocks;
            assert!(block_idx < num_blocks);
        }
    }

    #[test]
    fn test_hash_distribution() {
        let num_samples = 10000;
        let seed = 9995u128;

        let mut hash_values = Vec::new();
        for i in 0..num_samples {
            let value = format!("test_value_{}", i).into_bytes();
            let mut hasher = DoubleHasher::with_value(&value, seed);
            hash_values.push(hasher.next_hash());
        }

        let min_h = hash_values.iter().min().unwrap();
        let max_h = hash_values.iter().max().unwrap();
        let range = max_h.wrapping_sub(*min_h);

        // Range should be large (good distribution across 64-bit space)
        assert!(
            range > u64::MAX / 2,
            "hash range too small: {} (expected > {})",
            range,
            u64::MAX / 2
        );
    }

    #[test]
    fn test_set_and_check_bit() {
        let mut bits = vec![0u8; 16];

        // Set some bits
        set_bit(&mut bits, 0);
        set_bit(&mut bits, 7);
        set_bit(&mut bits, 8);
        set_bit(&mut bits, 127);

        // Check they're set
        assert!(check_bit(&bits, 0));
        assert!(check_bit(&bits, 7));
        assert!(check_bit(&bits, 8));
        assert!(check_bit(&bits, 127));

        // Check others are not set
        assert!(!check_bit(&bits, 1));
        assert!(!check_bit(&bits, 64));
        assert!(!check_bit(&bits, 126));

        // Verify byte values
        assert_eq!(bits[0], 0b1000_0001); // bits 0 and 7
        assert_eq!(bits[1], 0b0000_0001); // bit 8
        assert_eq!(bits[15], 0b1000_0000); // bit 127
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bloom Filter implementation with optional folding.
use super::hash;

/// Minimum size after folding: 8192 bits = 1024 bytes
///
/// This prevents over-folding which causes correlated bits (from common items like
/// popular packages) to concentrate and create hot spots with high false positive rates.
pub const MIN_FOLD_BITS: usize = 8192;

/// Stop folding when bit density exceeds this threshold.
pub const MAX_FOLD_DENSITY: f64 = 0.40;

/// A standard bloom filter with bits spread across the entire filter.
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<u8>,
    num_hashes: u32,
    seed: u128,
}

impl BloomFilter {
    /// Create a new bloom filter with the specified number of bits.
    pub fn new(num_bits: usize, num_hashes: u32, seed: u128) -> Self {
        Self {
            bits: vec![0u8; num_bits / 8],
            num_hashes,
            seed,
        }
    }

    /// Get the number of bits in this filter.
    pub fn num_bits(&self) -> usize {
        self.bits.len() * 8
    }

    /// Insert a key into the filter.
    pub fn insert(&mut self, key: &[u8]) {
        let positions = hash::compute_positions(key, self.num_bits(), self.num_hashes, self.seed);
        for bit_pos in positions {
            self.bits[bit_pos / 8] |= 1 << (bit_pos % 8);
        }
    }

    /// Serialize to bytes (for DB storage).
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.clone()
    }

    /// Get the seed used for hashing.
    pub fn seed(&self) -> u128 {
        self.seed
    }

    /// Get the number of hash functions.
    pub fn num_hashes(&self) -> u32 {
        self.num_hashes
    }

    /// Repeatedly halves the filter by ORing the upper half into the lower half
    /// until density exceeds MAX_FOLD_DENSITY or size reaches MIN_FOLD_BITS.
    ///
    /// To query the folded filter, use `compute_original_bit_positions()` with the original filter size,
    /// then apply `pos % folded_bits` to get positions in the folded filter.
    pub fn fold(self) -> Vec<u8> {
        let mut bits = self.bits;

        loop {
            let current_bits = bits.len() * 8;

            // Stop if we've reached minimum size
            if current_bits <= MIN_FOLD_BITS {
                break;
            }

            // Stop if density exceeds threshold
            let popcount: usize = bits.iter().map(|b| b.count_ones() as usize).sum();
            let density = popcount as f64 / current_bits as f64;
            if density > MAX_FOLD_DENSITY {
                break;
            }

            // Fold: OR upper half into lower half
            let half = bits.len() / 2;
            for i in 0..half {
                bits[i] |= bits[half + i];
            }
            bits.truncate(half);
        }

        bits
    }
}

/// Compute bit positions in the original (unfolded) filter for each hash function.
/// Used for SQL queries on folded filters where the folded position is computed
/// dynamically via `original_pos % (length(column) * 8)`.
pub fn compute_original_bit_positions(
    key: &[u8],
    original_num_bits: usize,
    num_hashes: u32,
    seed: u128,
) -> Vec<usize> {
    hash::compute_positions(key, original_num_bits, num_hashes, seed)
}

#[cfg(test)]
impl BloomFilter {
    /// Check if a key might be in the filter for testing, in production this is done using SQL.
    pub fn contains(&self, key: &[u8]) -> bool {
        let positions = hash::compute_positions(key, self.num_bits(), self.num_hashes, self.seed);
        positions
            .iter()
            .all(|&pos| self.bits[pos / 8] & (1 << (pos % 8)) != 0)
    }

    /// Calculate the current bit density (test-only).
    pub fn density(&self) -> f64 {
        let popcount: usize = self.bits.iter().map(|b| b.count_ones() as usize).sum();
        popcount as f64 / self.num_bits() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: Check if a key might be in a folded bloom filter.
    ///
    /// This function performs the same check as the SQL query would:
    /// compute positions using original size, then mod by folded size.
    pub fn folded_bloom_contains(
        folded_bytes: &[u8],
        key: &[u8],
        original_num_bits: usize,
        num_hashes: u32,
        seed: u128,
    ) -> bool {
        let folded_bits = folded_bytes.len() * 8;
        let positions = hash::compute_positions(key, original_num_bits, num_hashes, seed);
        positions.iter().all(|&pos| {
            let folded_pos = pos % folded_bits;
            folded_bytes[folded_pos / 8] & (1 << (folded_pos % 8)) != 0
        })
    }

    #[test]
    fn test_insert_and_contains() {
        let mut bloom = BloomFilter::new(8192, 5, 67);

        let key1 = b"hello";
        let key2 = b"world";

        bloom.insert(key1);
        bloom.insert(key2);

        assert!(bloom.contains(key1), "Should contain inserted key");
        assert!(bloom.contains(key2), "Should contain inserted key");
    }

    #[test]
    fn test_size() {
        let bloom = BloomFilter::new(8192, 5, 67);
        let bytes = bloom.to_bytes();

        assert_eq!(bytes.len(), 1024, "8192 bits = 1024 bytes = 1KB");
    }

    #[test]
    fn test_fold_preserves_membership() {
        let mut bloom = BloomFilter::new(8192, 5, 67);
        let original_num_bits = bloom.num_bits();
        let num_hashes = bloom.num_hashes();
        let seed = bloom.seed();

        let keys: Vec<Vec<u8>> = (0..50).map(|i| format!("key{}", i).into_bytes()).collect();
        for key in &keys {
            bloom.insert(key);
        }

        let folded_bytes = bloom.fold();

        // All original keys must still be found (no false negatives)
        for key in &keys {
            assert!(
                folded_bloom_contains(&folded_bytes, key, original_num_bits, num_hashes, seed),
                "Folded filter must contain all original keys"
            );
        }
    }

    #[test]
    fn test_fold_minimum_size() {
        let mut bloom = BloomFilter::new(16384, 5, 67);
        let original_num_bits = bloom.num_bits();
        let num_hashes = bloom.num_hashes();
        let seed = bloom.seed();

        // Insert just one key - should fold down to minimum
        bloom.insert(b"single");

        let folded_bytes = bloom.fold();

        // Should fold down to MIN_FOLD_BITS (8192 bits = 1024 bytes)
        assert!(
            folded_bytes.len() >= MIN_FOLD_BITS / 8,
            "Should not fold below minimum size"
        );
        assert_eq!(
            folded_bytes.len(),
            MIN_FOLD_BITS / 8,
            "Very sparse filter should fold to minimum"
        );
        assert!(
            folded_bloom_contains(
                &folded_bytes,
                b"single",
                original_num_bits,
                num_hashes,
                seed
            ),
            "Should still contain the key"
        );
    }

    #[test]
    fn test_fold_roundtrip() {
        let mut bloom = BloomFilter::new(8192, 5, 67);
        let original_num_bits = bloom.num_bits();
        let num_hashes = bloom.num_hashes();
        let seed = bloom.seed();

        bloom.insert(b"key1");
        bloom.insert(b"key2");

        let folded_bytes = bloom.fold();

        // Verify keys can be found in the folded bytes
        assert!(folded_bloom_contains(
            &folded_bytes,
            b"key1",
            original_num_bits,
            num_hashes,
            seed
        ));
        assert!(folded_bloom_contains(
            &folded_bytes,
            b"key2",
            original_num_bits,
            num_hashes,
            seed
        ));
    }

    #[test]
    fn test_density() {
        let mut bloom = BloomFilter::new(8192, 5, 67);
        assert_eq!(bloom.density(), 0.0, "Empty filter should have 0 density");

        bloom.insert(b"key1");
        let density = bloom.density();
        assert!(density > 0.0, "Should have some bits set");
        assert!(density < 0.01, "Few keys should have low density");
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::blooms::hash::DoubleHasher;
use crate::blooms::hash::set_bit;

/// Probe for a bloom filter: (byte_offsets, bit_masks).
pub type BloomProbe = (Vec<usize>, Vec<usize>);

/// A standard bloom filter with bits spread across the entire filter.
#[derive(Debug, Clone)]
pub struct BloomFilter<const BYTES: usize, const HASHES: u32, const SEED: u128> {
    bytes: Vec<u8>,
}

impl<const BYTES: usize, const HASHES: u32, const SEED: u128> BloomFilter<BYTES, HASHES, SEED> {
    /// Create a new bloom filter.
    pub fn new() -> Self {
        Self {
            bytes: vec![0u8; BYTES],
        }
    }

    pub fn insert(&mut self, key: &[u8]) {
        let num_bits = BYTES * 8;
        for h in DoubleHasher::with_value(key, SEED).take(HASHES as usize) {
            set_bit(&mut self.bytes, (h as usize) % num_bits);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn num_bits(&self) -> usize {
        BYTES * 8
    }

    pub fn popcount(&self) -> usize {
        self.bytes.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Repeatedly halves the filter by ORing the upper half into the lower half
    /// until density exceeds `max_fold_density` or size reaches `min_fold_bytes`.
    /// This allows us to allocate a larger bloom filter to handle checkpoints with more items
    /// while folding to handle sparse filters.
    ///
    /// To get the folded positions from the original bit positions,
    /// folded_idx = `idx % folded_num_bits` where idx is the original bit position.
    pub fn fold(self, min_fold_bytes: usize, max_fold_density: f64) -> Vec<u8> {
        let mut bytes = self.bytes;

        loop {
            // Stop if size is not a power of two (can't halve evenly)
            if !bytes.len().is_power_of_two() {
                break;
            }

            // Stop if we've reached minimum size
            if bytes.len() <= min_fold_bytes {
                break;
            }

            // Stop if density exceeds threshold
            let popcount: usize = bytes.iter().map(|b| b.count_ones() as usize).sum();
            let density = popcount as f64 / (bytes.len() * 8) as f64;
            if density > max_fold_density {
                break;
            }

            // Fold: OR upper half into lower half
            let half = bytes.len() / 2;
            for i in 0..half {
                bytes[i] |= bytes[half + i];
            }
            bytes.truncate(half);
        }

        bytes
    }
}

impl<const BYTES: usize, const HASHES: u32, const SEED: u128> Default
    for BloomFilter<BYTES, HASHES, SEED>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AsRef<[u8]>, const BYTES: usize, const HASHES: u32, const SEED: u128> Extend<T>
    for BloomFilter<BYTES, HASHES, SEED>
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for key in iter {
            self.insert(key.as_ref());
        }
    }
}

/// Hashes a value to the bit positions to set/check in a bloom filter.
pub fn hash<const BYTES: usize, const HASHES: u32, const SEED: u128>(
    value: &[u8],
) -> impl Iterator<Item = usize> {
    let num_bits = BYTES * 8;
    DoubleHasher::with_value(value, SEED)
        .take(HASHES as usize)
        .map(move |h| (h as usize) % num_bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blooms::hash;
    use crate::cp_blooms::MAX_FOLD_DENSITY;
    use crate::cp_blooms::MIN_FOLD_BYTES;

    // Test bloom filter with specific dimensions
    type TestBloomFilter = BloomFilter<1024, 5, 67>;
    type TestBloomFilter2048 = BloomFilter<2048, 5, 67>;

    impl<const BYTES: usize, const HASHES: u32, const SEED: u128> BloomFilter<BYTES, HASHES, SEED> {
        /// Check if a key might be in the filter for testing, in production this is done using SQL.
        pub fn contains(&self, key: &[u8]) -> bool {
            let num_bits = self.num_bits();
            DoubleHasher::with_value(key, SEED)
                .take(HASHES as usize)
                .map(|h| (h as usize) % num_bits)
                .all(|pos| hash::check_bit(&self.bytes, pos))
        }

        /// Calculate the current bit density.
        pub fn density(&self) -> f64 {
            self.popcount() as f64 / self.num_bits() as f64
        }
    }

    /// Check if a key might be in a folded bloom filter.
    fn folded_bloom_contains<const BYTES: usize, const HASHES: u32, const SEED: u128>(
        folded_bytes: &[u8],
        key: &[u8],
    ) -> bool {
        let original_num_bits = BYTES * 8;
        let folded_bits = folded_bytes.len() * 8;
        let mut hasher = DoubleHasher::with_value(key, SEED);
        (0..HASHES).all(|_| {
            let pos = (hasher.next_hash() as usize) % original_num_bits;
            let folded_pos = pos % folded_bits;
            hash::check_bit(folded_bytes, folded_pos)
        })
    }

    #[test]
    fn test_insert_and_contains() {
        let mut bloom = TestBloomFilter::new();

        let key1 = b"hello";
        let key2 = b"world";

        bloom.insert(key1);
        bloom.insert(key2);

        assert!(bloom.contains(key1), "Should contain inserted key");
        assert!(bloom.contains(key2), "Should contain inserted key");
    }

    #[test]
    fn test_size() {
        let bloom = TestBloomFilter::new();
        let bytes = bloom.to_bytes();

        assert_eq!(bytes.len(), 1024, "1024 bytes = 1KB");
    }

    #[test]
    fn test_fold_preserves_membership() {
        let mut bloom = TestBloomFilter::new();

        let keys: Vec<Vec<u8>> = (0..50).map(|i| format!("key{}", i).into_bytes()).collect();
        for key in &keys {
            bloom.insert(key);
        }

        let folded_bytes = bloom.fold(MIN_FOLD_BYTES, MAX_FOLD_DENSITY);

        // All original keys must still be found (no false negatives)
        for key in &keys {
            assert!(
                folded_bloom_contains::<1024, 5, 67>(&folded_bytes, key),
                "Folded filter must contain all original keys"
            );
        }
    }

    #[test]
    fn test_fold_minimum_size() {
        let mut bloom = TestBloomFilter2048::new();

        // Insert just one key - should fold down to minimum
        bloom.insert(b"single");

        let folded_bytes = bloom.fold(MIN_FOLD_BYTES, MAX_FOLD_DENSITY);

        // Should fold down to MIN_FOLD_BYTES (1024 bytes)
        assert!(
            folded_bytes.len() >= MIN_FOLD_BYTES,
            "Should not fold below minimum size"
        );
        assert_eq!(
            folded_bytes.len(),
            MIN_FOLD_BYTES,
            "Very sparse filter should fold to minimum"
        );
        assert!(
            folded_bloom_contains::<2048, 5, 67>(&folded_bytes, b"single"),
            "Should still contain the key"
        );
    }

    #[test]
    fn test_fold_roundtrip() {
        let mut bloom = TestBloomFilter::new();

        bloom.insert(b"key1");
        bloom.insert(b"key2");

        let folded_bytes = bloom.fold(MIN_FOLD_BYTES, MAX_FOLD_DENSITY);

        // Verify keys can be found in the folded bytes
        assert!(folded_bloom_contains::<1024, 5, 67>(&folded_bytes, b"key1"));
        assert!(folded_bloom_contains::<1024, 5, 67>(&folded_bytes, b"key2"));
    }

    #[test]
    fn test_density() {
        let mut bloom = TestBloomFilter::new();
        assert_eq!(bloom.density(), 0.0, "Empty filter should have 0 density");

        bloom.insert(b"key1");
        let density = bloom.density();
        assert!(density > 0.0, "Should have some bits set");
        assert!(density < 0.01, "Few keys should have low density");
    }
}

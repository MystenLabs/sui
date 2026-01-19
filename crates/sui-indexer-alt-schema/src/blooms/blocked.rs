// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::blooms::hash::DoubleHasher;
use crate::blooms::hash::set_bit;

/// Probe for a blocked bloom filter: (block_idx, byte_offsets, bit_masks).
pub type BlockedBloomProbe = (usize, Vec<usize>, Vec<usize>);

#[derive(Clone)]
pub struct BlockedBloomFilter<const BYTES: usize, const BLOCKS: usize, const HASHES: u32> {
    blocks: Vec<[u8; BYTES]>,
    seed: u128,
}

impl<const BYTES: usize, const BLOCKS: usize, const HASHES: u32>
    BlockedBloomFilter<BYTES, BLOCKS, HASHES>
{
    pub fn new(seed: u128) -> Self {
        Self {
            blocks: vec![[0u8; BYTES]; BLOCKS],
            seed,
        }
    }

    /// Insert a value into the bloom filter by setting the bits in the block that the
    /// hash function produces. The first hash sets the block index, and the remaining hashes
    /// set bits within that block. This allows us to query specific blocks for membership instead
    /// of scanning the entire bloom filter.
    pub fn insert(&mut self, value: &[u8]) {
        let bits_per_block = BYTES * 8;
        let block_idx = {
            let mut hasher = DoubleHasher::with_value(value, self.seed);
            (hasher.next_hash() as usize) % BLOCKS
        };
        let block = &mut self.blocks[block_idx];
        for h in DoubleHasher::with_value(value, self.seed.wrapping_add(1)).take(HASHES as usize) {
            set_bit(block, (h as usize) % bits_per_block);
        }
    }

    /// The block index and block's bloom filter for filters with bits set.
    pub fn into_sparse_blocks(self) -> impl Iterator<Item = (usize, Vec<u8>)> {
        self.blocks
            .into_iter()
            .enumerate()
            .filter(|(_, block)| block.iter().any(|&b| b != 0))
            .map(|(idx, block)| (idx, block.to_vec()))
    }
}

impl<T: AsRef<[u8]>, const BYTES: usize, const BLOCKS: usize, const HASHES: u32> Extend<T>
    for BlockedBloomFilter<BYTES, BLOCKS, HASHES>
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for key in iter {
            self.insert(key.as_ref());
        }
    }
}

/// Hashes a value to the block index and bit positions to set/check in a blocked bloom filter.
/// Uses separate hashers for block selection and bit indexes to prevent correlated patterns.
pub fn hash<const BYTES: usize, const BLOCKS: usize, const HASHES: u32>(
    value: &[u8],
    seed: u128,
) -> (usize, impl Iterator<Item = usize>) {
    let bits_per_block = BYTES * 8;
    let block_idx = {
        let mut hasher = DoubleHasher::with_value(value, seed);
        (hasher.next_hash() as usize) % BLOCKS
    };
    let bit_iter = DoubleHasher::with_value(value, seed.wrapping_add(1))
        .take(HASHES as usize)
        .map(move |h| (h as usize) % bits_per_block);
    (block_idx, bit_iter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cp_bloom_blocks::{BLOOM_BLOCK_BYTES, NUM_BLOOM_BLOCKS, NUM_HASHES};

    type TestBloomFilter =
        BlockedBloomFilter<{ BLOOM_BLOCK_BYTES }, { NUM_BLOOM_BLOCKS }, { NUM_HASHES }>;

    impl<const BYTES: usize, const BLOCKS: usize, const HASHES: u32>
        BlockedBloomFilter<BYTES, BLOCKS, HASHES>
    {
        /// Get a specific block by index.
        pub fn get_block(&self, block_idx: usize) -> Option<&[u8]> {
            self.blocks.get(block_idx).map(|b| b.as_slice())
        }

        /// Check if a block contains any set bits.
        pub fn is_block_nonzero(&self, block_idx: usize) -> bool {
            self.blocks
                .get(block_idx)
                .is_some_and(|b| b.iter().any(|&x| x != 0))
        }

        /// Check if a key might be in the bloom filter.
        pub fn contains(&self, key: &[u8]) -> bool {
            use crate::blooms::hash::check_bit;
            let (block_idx, mut bit_idxs) = hash::<BYTES, BLOCKS, HASHES>(key, self.seed);
            bit_idxs.all(|bit_idx| check_bit(&self.blocks[block_idx], bit_idx))
        }

        /// Number of blocks in the filter (for tests).
        pub fn num_blocks(&self) -> usize {
            BLOCKS
        }
    }

    fn new_test_filter(seed: u128) -> TestBloomFilter {
        TestBloomFilter::new(seed)
    }

    #[test]
    fn test_blocked_bloom_basic() {
        let mut bloom = new_test_filter(42);

        let key1 = b"test_key_1";
        let key2 = b"test_key_2";
        let key3 = b"test_key_3";

        // Initially empty
        assert!(!bloom.contains(key1));

        // Insert and check
        bloom.insert(key1);
        assert!(bloom.contains(key1));
        assert!(!bloom.contains(key2));

        // Insert more
        bloom.insert(key2);
        assert!(bloom.contains(key1));
        assert!(bloom.contains(key2));
        assert!(!bloom.contains(key3));
    }

    #[test]
    fn test_sparse_blocks() {
        let mut bloom = new_test_filter(42);

        // Insert a few items
        bloom.insert(b"key1");
        bloom.insert(b"key2");
        bloom.insert(b"key3");

        let bloomed = bloom.clone();

        let sparse: Vec<_> = bloom.into_sparse_blocks().collect();

        // Should have far fewer than 128 blocks
        assert!(sparse.len() <= 3);
        assert!(!sparse.is_empty());

        // All sparse blocks should be non-zero
        for (idx, block) in &sparse {
            assert!(bloomed.is_block_nonzero(*idx));
            assert!(block.iter().any(|&b| b != 0));
        }
    }

    #[test]
    fn test_no_oversaturation() {
        // Reproduce cp_block 9995 scenario: 1,292 items with seed 9995
        let seed: u128 = 9995;
        let num_items = 1292;
        let mut bloom = new_test_filter(seed);

        // Generate realistic items (32-byte addresses)
        for i in 0..num_items {
            let mut addr = [0u8; 32];
            addr[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            bloom.insert(&addr);
        }

        let bloomed = bloom.clone();

        // Analyze saturation across all non-zero blocks
        let sparse_blocks: Vec<_> = bloom.into_sparse_blocks().collect();

        let mut total_saturated_bytes = 0;
        let mut max_saturated_bytes = 0;

        for (block_idx, block_data) in &sparse_blocks {
            let mut saturated_bytes = 0;
            let mut nonzero_bytes = 0;

            for &byte in block_data {
                if byte != 0 {
                    nonzero_bytes += 1;
                }
                if byte == 255 {
                    saturated_bytes += 1;
                }
            }

            total_saturated_bytes += saturated_bytes;
            max_saturated_bytes = max_saturated_bytes.max(saturated_bytes);

            // With ~10 items per block (1292/128), we expect ~7-15 bytes with at least one bit
            // and 0-3 fully saturated bytes (healthy threshold)
            assert!(
                saturated_bytes <= 5,
                "Block {} has {} saturated bytes (should be ≤5). \
                 This indicates clustering! nonzero_bytes={}, block_data={:?}",
                block_idx,
                saturated_bytes,
                nonzero_bytes,
                &block_data[0..16] // Show first 16 bytes for debugging
            );
        }

        let avg_saturated_bytes = total_saturated_bytes as f64 / sparse_blocks.len() as f64;

        // Healthy thresholds for 1,292 items across 128 blocks (~10 items/block)
        assert!(
            avg_saturated_bytes < 3.0,
            "Average saturation too high: {:.2} (should be <3.0)",
            avg_saturated_bytes
        );
        assert!(
            max_saturated_bytes <= 5,
            "Max saturation too high: {} (should be ≤5)",
            max_saturated_bytes
        );

        // Verify all items can be found (no false negatives)
        for i in 0..num_items {
            let mut addr = [0u8; 32];
            addr[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            assert!(
                bloomed.contains(&addr),
                "False negative for item {} (bloom filter should never have false negatives)",
                i
            );
        }
    }

    #[test]
    fn test_single_item_sets_exactly_k_bits() {
        // Verify that inserting ONE item sets exactly k bits (one per hash function)
        let seed: u128 = 10001;
        let mut bloom = new_test_filter(seed);

        // Create a test key
        let key = b"test_single_item";

        // Count bits before insert
        let mut bits_before = 0;
        for block_idx in 0..bloom.num_blocks() {
            if let Some(block) = bloom.get_block(block_idx) {
                for &byte in block {
                    bits_before += byte.count_ones();
                }
            }
        }
        assert_eq!(bits_before, 0, "Bloom should start with 0 bits set");

        // Insert one item
        bloom.insert(key);

        // Count bits after insert
        let mut bits_after = 0;
        let mut bits_per_block = vec![0u32; bloom.num_blocks()];
        for (block_idx, bits) in bits_per_block.iter_mut().enumerate() {
            if let Some(block) = bloom.get_block(block_idx) {
                for &byte in block {
                    *bits += byte.count_ones();
                    bits_after += byte.count_ones();
                }
            }
        }

        // Find which block has bits set
        let non_zero_blocks: Vec<_> = bits_per_block
            .iter()
            .enumerate()
            .filter(|&(_, count)| *count > 0)
            .collect();

        // Should be exactly NUM_HASHES bits, all in ONE block
        assert_eq!(
            bits_after, NUM_HASHES,
            "Should set exactly {} bits (NUM_HASHES), got {}",
            NUM_HASHES, bits_after
        );
        assert_eq!(
            non_zero_blocks.len(),
            1,
            "Should affect only 1 block, got {}",
            non_zero_blocks.len()
        );
    }
}

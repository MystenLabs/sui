// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Blocked bloom filters are split into 128 blocks stored as separate database rows.
//!
//! 1. First hash selects which 2KB block (0-127)
//! 2. Remaining k-1 hashes select bit positions within that block
//! 3. Each block is stored as a separate database row
use super::hash;

/// Size of each bloom block in bytes.
pub const BLOOM_BLOCK_BYTES: usize = 2048;

/// Number of bits per bloom block (BLOOM_BLOCK_BYTES * 8).
pub const BLOOM_BLOCK_BITS: usize = 16384;

/// Number of blocks in the bloom filter (stored as separate database rows).
pub const NUM_BLOOM_BLOCKS: usize = 128;

/// Total bits in the bloom filter (128 × 16384 = 2M bits = 256KB).
pub const TOTAL_BLOOM_BITS: usize = 2097152;

/// Number of hash functions (k) used per key.
pub const NUM_HASHES: u32 = 5;

pub struct BlockedBloomFilter {
    blocks: Vec<Vec<u8>>,
    seed: u128,
}

impl BlockedBloomFilter {
    /// Create a new blocked bloom filter with the given seed.
    pub fn new(seed: u128) -> Self {
        Self {
            blocks: vec![vec![0u8; BLOOM_BLOCK_BYTES]; NUM_BLOOM_BLOCKS],
            seed,
        }
    }

    /// Insert a key into the bloom filter.
    pub fn insert(&mut self, key: &[u8]) {
        let (block_idx, positions) = hash::compute_blocked_positions(
            key,
            NUM_BLOOM_BLOCKS,
            BLOOM_BLOCK_BITS,
            NUM_HASHES,
            self.seed,
        );
        let block = &mut self.blocks[block_idx];

        for pos in positions {
            let byte_idx = pos / 8;
            let bit_idx = pos % 8;
            block[byte_idx] |= 1 << bit_idx;
        }
    }

    /// Get all non-zero blocks with their indices (for sparse storage).
    pub fn to_sparse_blocks(&self) -> Vec<(usize, Vec<u8>)> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.iter().any(|&b| b != 0))
            .map(|(idx, block)| (idx, block.clone()))
            .collect()
    }
}

/// Compute the block index and bit positions for a single key.
///
/// Returns (block_index, [5 bit positions within that block]).
/// Used for generating SQL bloom filter checks.
pub fn compute_key_hash_positions(key: &[u8], seed: u128) -> (usize, Vec<usize>) {
    hash::compute_blocked_positions(key, NUM_BLOOM_BLOCKS, BLOOM_BLOCK_BITS, NUM_HASHES, seed)
}

#[cfg(test)]
impl BlockedBloomFilter {
    /// Get a specific 2048-byte block by index.
    pub fn get_block(&self, block_idx: usize) -> Option<&[u8]> {
        self.blocks.get(block_idx).map(|b| b.as_slice())
    }

    /// Check if a block contains any set bits.
    pub fn is_block_nonzero(&self, block_idx: usize) -> bool {
        self.blocks
            .get(block_idx)
            .is_some_and(|b| b.iter().any(|&x| x != 0))
    }

    /// Check if a key might be in the bloom filter (test-only).
    /// In production this is done using SQL.
    pub fn contains(&self, key: &[u8]) -> bool {
        let (block_idx, positions) = hash::compute_blocked_positions(
            key,
            NUM_BLOOM_BLOCKS,
            BLOOM_BLOCK_BITS,
            NUM_HASHES,
            self.seed,
        );
        let block = &self.blocks[block_idx];
        positions
            .iter()
            .all(|&pos| (block[pos / 8] & (1 << (pos % 8))) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_bloom_basic() {
        let mut bloom = BlockedBloomFilter::new(42);

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
        let mut bloom = BlockedBloomFilter::new(42);

        // Insert a few items
        bloom.insert(b"key1");
        bloom.insert(b"key2");
        bloom.insert(b"key3");

        let sparse = bloom.to_sparse_blocks();

        // Should have far fewer than 128 blocks
        assert!(sparse.len() < NUM_BLOOM_BLOCKS);
        assert!(!sparse.is_empty());

        // All sparse blocks should be non-zero
        for (idx, block) in &sparse {
            assert!(bloom.is_block_nonzero(*idx));
            assert!(block.iter().any(|&b| b != 0));
        }
    }

    #[test]
    fn test_different_seeds_different_blocks() {
        let key = b"test_key";

        // Different seeds should hash to different block indices
        let block1 = hash::compute_block_index(key, NUM_BLOOM_BLOCKS, 1);
        let block2 = hash::compute_block_index(key, NUM_BLOOM_BLOCKS, 2);

        // Different seeds should produce different block assignments
        // (not guaranteed for every key but highly probable)
        assert!(block1 < NUM_BLOOM_BLOCKS);
        assert!(block2 < NUM_BLOOM_BLOCKS);
    }

    #[test]
    fn test_no_oversaturation_with_realistic_load() {
        // Reproduce cp_block 9995 scenario: 1,292 items with seed 9995
        let seed: u128 = 9995;
        let num_items = 1292;
        let mut bloom = BlockedBloomFilter::new(seed);

        // Generate realistic items (32-byte addresses)
        for i in 0..num_items {
            let mut addr = [0u8; 32];
            addr[0..8].copy_from_slice(&(i as u64).to_le_bytes());
            bloom.insert(&addr);
        }

        // Analyze saturation across all non-zero blocks
        let sparse_blocks = bloom.to_sparse_blocks();

        let mut total_saturated_bytes = 0;
        let mut total_nonzero_bytes = 0;
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
            total_nonzero_bytes += nonzero_bytes;
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
        let avg_nonzero_bytes = total_nonzero_bytes as f64 / sparse_blocks.len() as f64;

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
                bloom.contains(&addr),
                "False negative for item {} (bloom filter should never have false negatives)",
                i
            );
        }
    }

    #[test]
    fn test_single_item_sets_exactly_k_bits() {
        // Verify that inserting ONE item sets exactly k bits (one per hash function)
        let seed: u128 = 10001;
        let mut bloom = BlockedBloomFilter::new(seed);

        // Create a test key
        let key = b"test_single_item";

        // Count bits before insert
        let mut bits_before = 0;
        for block_idx in 0..NUM_BLOOM_BLOCKS {
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
        let mut bits_per_block = vec![0u32; NUM_BLOOM_BLOCKS];
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

    #[test]
    fn test_compute_key_hash_positions() {
        let key = b"test_key";
        let seed = 42u128;

        let (block_idx, positions) = compute_key_hash_positions(key, seed);

        assert!(block_idx < NUM_BLOOM_BLOCKS);
        assert_eq!(positions.len(), NUM_HASHES as usize);
        for pos in positions {
            assert!(pos < BLOOM_BLOCK_BITS);
        }
    }
}

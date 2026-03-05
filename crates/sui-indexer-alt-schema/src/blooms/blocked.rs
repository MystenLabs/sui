// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Blocked bloom filter implementation for efficient database queries.
//!
//! A blocked bloom filter partitions the filter into fixed-size blocks, where each element
//! hashes to exactly one block for bits to be set, essentially a hash-indexed array of smaller bloom filters.
//! Each block is stored as a separate row. This allows us to query specific blocks for membership instead of
//! scanning the entire filter for improved IO.
//!
//! 1. **Targeted queries**: When checking membership, only the specific block for that element
//!    needs to be loaded from the database, not the entire filter.
//!
//! 2. **Incremental updates**: Blocks can be OR'd together independently, allowing bloom filters
//!    from multiple checkpoints to be merged at the block level without loading entire filters.
//!    **Note**: Do not use folding with blocked bloom filters. Merging requires fixed-size blocks;
//!    variable-sized blocks from folding would cause the merge to fail.
//!
//! 3. **Sparse storage**: Only blocks with bits set need to be stored. In practice, with well-chosen parameters,
//!    values should be uniformly distributed across blocks.
//!
//! The filter uses double hashing: the first hash selects the block, and subsequent hashes
//! (with a different seed to prevent correlation) select bit positions within that block.
//!
//!
//! ```text
//!   Blocked Bloom Filter (128 blocks × 2 KB each)
//!   ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┐
//!   │  0  │  1  │  2  │  3  │ ... │ 125 │ 126 │ 127 │
//!   └─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┘
//!            ▲
//!            │
//!       hash(key, seed) selects block, then subsequent hash(key, seed1) sets bits within that block
//! ```
//!

use std::collections::BTreeMap;

use crate::blooms::bloom::BloomProbe;
use crate::blooms::hash::DoubleHasher;
use crate::blooms::hash::set_bit;

/// Probe for checking membership in a blocked bloom filter.
pub struct BlockedBloomProbe {
    pub block_idx: usize,
    pub probe: BloomProbe,
}

/// A generic blocked bloom filter.
///
/// # Type Parameters
///
/// - `BYTES`: Number of bytes per block.
/// - `BLOCKS`: Number of blocks in the filter.
/// - `HASHES`: Number of hash functions applied per element inserted. Each hash sets one bit within the selected block.
#[derive(Clone)]
pub struct BlockedBloomFilter<const BYTES: usize, const BLOCKS: usize, const HASHES: u32> {
    blocks: Box<[[u8; BYTES]]>,
    seed: u128,
}

impl<const BYTES: usize, const BLOCKS: usize, const HASHES: u32>
    BlockedBloomFilter<BYTES, BLOCKS, HASHES>
{
    pub fn new(seed: u128) -> Self {
        Self {
            blocks: vec![[0u8; BYTES]; BLOCKS].into_boxed_slice(),
            seed,
        }
    }

    /// Insert a value into the bloom filter by setting the bits in the block that the
    /// hash function produces. The first hash sets the block index, and the remaining hashes
    /// set bits within that block. This allows us to query specific blocks for membership instead
    /// of scanning the entire bloom filter.
    pub fn insert(&mut self, value: &[u8]) {
        let (block_idx, bit_positions) = Self::hash(value, self.seed);
        let block = &mut self.blocks[block_idx];
        for bit_idx in bit_positions {
            set_bit(block, bit_idx);
        }
    }

    /// Consumes the filter to return non-empty block indexes and corresponding data.
    pub fn into_sparse_blocks(self) -> impl Iterator<Item = (usize, [u8; BYTES])> {
        self.blocks
            .into_iter()
            .enumerate()
            .filter(|(_, block)| block.iter().any(|&b| b != 0))
    }

    /// Compute the block index and bit positions within the block for a value.
    /// Uses separate hashers for block selection and bit indexes to prevent correlated patterns.
    pub fn hash(value: &[u8], seed: u128) -> (usize, impl Iterator<Item = usize>) {
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

    /// Probes for use bloom filter membership checks, probes from different values that hash to the
    /// same block are merged together.
    pub fn probe(
        seed: u128,
        values: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Vec<BlockedBloomProbe> {
        let mut by_block: BTreeMap<usize, BTreeMap<usize, u8>> = BTreeMap::new();
        for value in values {
            let (block_idx, bits) = Self::hash(value.as_ref(), seed);
            let block_entry = by_block.entry(block_idx).or_default();
            for b in bits {
                *block_entry.entry(b / 8).or_default() |= 1u8 << (b % 8);
            }
        }
        by_block
            .into_iter()
            .map(|(block_idx, by_offset)| BlockedBloomProbe {
                block_idx,
                probe: BloomProbe {
                    bit_probes: by_offset.into_iter().collect(),
                },
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use crate::blooms::hash::check_bit;
    use crate::cp_bloom_blocks::BLOOM_BLOCK_BYTES;
    use crate::cp_bloom_blocks::NUM_BLOOM_BLOCKS;
    use crate::cp_bloom_blocks::NUM_HASHES;

    use super::*;

    type TestBloomFilter = BlockedBloomFilter<BLOOM_BLOCK_BYTES, NUM_BLOOM_BLOCKS, NUM_HASHES>;

    impl<const BYTES: usize, const BLOCKS: usize, const HASHES: u32>
        BlockedBloomFilter<BYTES, BLOCKS, HASHES>
    {
        /// Get a specific block by index.
        pub fn block(&self, block_idx: usize) -> Option<&[u8; BYTES]> {
            self.blocks.get(block_idx)
        }

        /// Check if a block contains any set bits.
        pub fn is_block_nonzero(&self, block_idx: usize) -> bool {
            self.blocks
                .get(block_idx)
                .is_some_and(|b| b.iter().any(|&x| x != 0))
        }

        /// Check if a key might be in the bloom filter.
        pub fn contains(&self, key: &[u8]) -> bool {
            let (block_idx, mut bit_idxs) = Self::hash(key, self.seed);
            bit_idxs.all(|bit_idx| check_bit(&self.blocks[block_idx], bit_idx))
        }
    }

    #[test]
    fn test_blocked_bloom_basic() {
        let mut bloom = TestBloomFilter::new(42);

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
        let mut bloom = TestBloomFilter::new(42);

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
        let mut bloom = TestBloomFilter::new(seed);

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
        let mut bloom = TestBloomFilter::new(seed);

        // Create a test key
        let key = b"test_single_item";

        // Count bits before insert
        let mut bits_before = 0;
        for block_idx in 0..NUM_BLOOM_BLOCKS {
            if let Some(block) = bloom.block(block_idx) {
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
            if let Some(block) = bloom.block(block_idx) {
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

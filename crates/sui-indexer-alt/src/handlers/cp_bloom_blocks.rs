// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::upsert::excluded;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_schema::cp_bloom_blocks::BLOOM_BLOCK_BYTES;
use sui_indexer_alt_schema::cp_bloom_blocks::CpBlockedBloomFilter;
use sui_indexer_alt_schema::cp_bloom_blocks::StoredCpBloomBlock;
use sui_indexer_alt_schema::cp_bloom_blocks::cp_block_index;
use sui_indexer_alt_schema::cp_blooms::bytea_or;
use sui_indexer_alt_schema::schema::cp_bloom_blocks;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::handlers::cp_blooms::insert_tx_addresses;

/// Blocked bloom filters that span multiple checkpoints for efficient range queries.
///
/// Checkpoints are assigned to 1000-checkpoint blocks:
/// - `cp_block_index(cp_num) = cp_num / 1000`
/// - Block 0: checkpoints 0-999
/// - Block 1: checkpoints 1000-1999
/// - etc.
pub(crate) struct CpBloomBlocks;

/// Bloom filter blocks from a single checkpoint.
#[derive(Debug, Clone)]
pub(crate) struct CheckpointBloom {
    /// The checkpoint sequence number.
    pub cp_sequence_number: i64,
    /// Sparse bloom blocks (bloom_block_index -> bloom bytes).
    /// Only non-zero blocks are included.
    pub blocks: BTreeMap<u16, [u8; BLOOM_BLOCK_BYTES]>,
}

#[async_trait]
impl Processor for CpBloomBlocks {
    const NAME: &'static str = "cp_bloom_blocks";

    type Value = CheckpointBloom;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;
        let block_index = cp_block_index(cp_num);
        let seed = block_index as u128;

        let mut bloom = CpBlockedBloomFilter::new(seed);
        for tx in &checkpoint.transactions {
            insert_tx_addresses(tx, &mut bloom);
        }

        let blocks = bloom
            .into_sparse_blocks()
            .map(|(idx, data)| (idx as u16, data))
            .collect();

        Ok(vec![CheckpointBloom {
            cp_sequence_number: cp_num as i64,
            blocks,
        }])
    }
}

/// Batch for a single cp_block_index, containing bloom blocks.
#[derive(Default)]
pub(crate) struct CpBlockBatch {
    /// Bloom blocks keyed by bloom_block_index.
    blocks: BTreeMap<u16, [u8; BLOOM_BLOCK_BYTES]>,
}

#[async_trait]
impl Handler for CpBloomBlocks {
    type Store = Db;
    type Batch = BTreeMap<i64, CpBlockBatch>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        for cp_bloom in values {
            let block_index = cp_block_index(cp_bloom.cp_sequence_number as u64);

            let cp_block = batch.entry(block_index).or_default();

            for (bloom_idx, bloom_bytes) in cp_bloom.blocks {
                cp_block
                    .blocks
                    .entry(bloom_idx)
                    .and_modify(|existing| {
                        for (i, byte) in bloom_bytes.iter().enumerate() {
                            existing[i] |= byte;
                        }
                    })
                    .or_insert(bloom_bytes);
            }
        }

        BatchStatus::Pending
    }

    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        let rows: Vec<StoredCpBloomBlock> = batch
            .iter()
            .flat_map(|(cp_block_index, cp_block)| {
                cp_block
                    .blocks
                    .iter()
                    .map(move |(bloom_block_index, bloom_bytes)| StoredCpBloomBlock {
                        cp_block_index: *cp_block_index,
                        bloom_block_index: *bloom_block_index as i16,
                        bloom_filter: bloom_bytes.to_vec(),
                    })
            })
            .collect();

        let count = diesel::insert_into(cp_bloom_blocks::table)
            .values(&rows)
            .on_conflict((
                cp_bloom_blocks::cp_block_index,
                cp_bloom_blocks::bloom_block_index,
            ))
            .do_update()
            .set(cp_bloom_blocks::bloom_filter.eq(bytea_or(
                cp_bloom_blocks::bloom_filter,
                excluded(cp_bloom_blocks::bloom_filter),
            )))
            .execute(conn)
            .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use diesel::QueryDsl;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;

    use crate::MIGRATIONS;

    /// Build a CheckpointBloom from a checkpoint number and list of keys.
    fn make_checkpoint_bloom(cp_num: i64, keys: &[&[u8]]) -> CheckpointBloom {
        let block_index = cp_block_index(cp_num as u64);
        let seed = block_index as u128;
        let mut bloom = CpBlockedBloomFilter::new(seed);
        for key in keys {
            bloom.insert(key);
        }
        CheckpointBloom {
            cp_sequence_number: cp_num,
            blocks: bloom
                .into_sparse_blocks()
                .map(|(idx, data)| (idx as u16, data))
                .collect(),
        }
    }

    /// Check if a key is present in a bloom filter block.
    fn block_contains_key(block_data: &[u8], key: &[u8], seed: u128) -> bool {
        CpBlockedBloomFilter::hash(key, seed)
            .1
            .all(|idx| (block_data[idx / 8] & (1 << (idx % 8))) != 0)
    }

    /// Load all bloom blocks for a given cp_block_index.
    async fn get_bloom_blocks(
        conn: &mut Connection<'_>,
        cp_block_index: i64,
    ) -> Vec<StoredCpBloomBlock> {
        cp_bloom_blocks::table
            .filter(cp_bloom_blocks::cp_block_index.eq(cp_block_index))
            .order_by(cp_bloom_blocks::bloom_block_index)
            .load(conn)
            .await
            .unwrap()
    }

    /// Helper to commit checkpoint blooms using the Handler trait methods.
    async fn commit_blooms(blooms: Vec<CheckpointBloom>, conn: &mut Connection<'_>) -> usize {
        let handler = CpBloomBlocks;
        let mut batch = BTreeMap::new();
        handler.batch(&mut batch, &mut blooms.into_iter());
        handler.commit(&batch, conn).await.unwrap()
    }

    #[tokio::test]
    async fn test_single_batch_insert() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let blooms = vec![
            make_checkpoint_bloom(0, &[b"key_a", b"key_b"]),
            make_checkpoint_bloom(1, &[b"key_c"]),
            make_checkpoint_bloom(2, &[b"key_d"]),
        ];

        let count = commit_blooms(blooms, &mut conn).await;
        assert!(count > 0, "Should insert at least one bloom block");

        let blocks = get_bloom_blocks(&mut conn, 0).await;
        assert!(
            !blocks.is_empty(),
            "Should have bloom blocks for cp_block 0"
        );
    }

    #[tokio::test]
    async fn test_merge_on_conflict_combines_bloom_filters() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let seed = 0u128;

        // Find two keys that hash to the same bloom_block_index to test the merge behavior.
        // (cp_block_index, bloom_block_index).
        let key1 = b"key_0";
        let (target_block_idx, _) = CpBlockedBloomFilter::hash(key1, seed);

        // key_104 hashes to the same bloom_block_index as key_0 with seed 0
        let key2 = b"key_104";

        // First batch: checkpoints 0-1 with key1
        let blooms1 = vec![
            make_checkpoint_bloom(0, &[key1]),
            make_checkpoint_bloom(1, &[]),
        ];

        commit_blooms(blooms1, &mut conn).await;

        let blocks_after_first = get_bloom_blocks(&mut conn, 0).await;
        let first_block = blocks_after_first
            .iter()
            .find(|b| b.bloom_block_index == target_block_idx as i16)
            .expect("Block should exist for key1");
        assert!(
            block_contains_key(&first_block.bloom_filter, key1, seed),
            "First batch should contain key1"
        );

        // Second batch: checkpoints 2-3 with key2 (same bloom_block_index as key1, triggers merge)
        let blooms2 = vec![
            make_checkpoint_bloom(2, &[key2]),
            make_checkpoint_bloom(3, &[]),
        ];

        commit_blooms(blooms2, &mut conn).await;

        let blocks_after_merge = get_bloom_blocks(&mut conn, 0).await;

        // Find the merged block (same bloom_block_index as both keys)
        let merged_block = blocks_after_merge
            .iter()
            .find(|b| b.bloom_block_index == target_block_idx as i16)
            .expect("Merged block should exist");

        // Verify both keys are present in the merged bloom filter (OR'd together)
        assert!(
            block_contains_key(&merged_block.bloom_filter, key1, seed),
            "Merged bloom should contain key1 from first batch"
        );
        assert!(
            block_contains_key(&merged_block.bloom_filter, key2, seed),
            "Merged bloom should contain key2 from second batch"
        );
    }

    #[tokio::test]
    async fn test_different_cp_blocks_are_separate() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let blooms0 = vec![make_checkpoint_bloom(500, &[b"block0_key"])];

        let blooms1 = vec![make_checkpoint_bloom(1500, &[b"block1_key"])];

        commit_blooms(blooms0, &mut conn).await;
        commit_blooms(blooms1, &mut conn).await;

        let blocks0 = get_bloom_blocks(&mut conn, 0).await;
        let blocks1 = get_bloom_blocks(&mut conn, 1).await;

        assert!(!blocks0.is_empty(), "cp_block 0 should have data");
        assert!(!blocks1.is_empty(), "cp_block 1 should have data");

        assert_eq!(blocks0[0].cp_block_index, 0);
        assert_eq!(blocks1[0].cp_block_index, 1);

        let seed0 = 0u128;
        let seed1 = 1u128;
        assert!(
            block_contains_key(&blocks0[0].bloom_filter, b"block0_key", seed0),
            "Block 0 should contain block0_key"
        );
        assert!(
            block_contains_key(&blocks1[0].bloom_filter, b"block1_key", seed1),
            "Block 1 should contain block1_key"
        );
    }

    /// Verify no false negatives: all inserted keys must be found in the bloom filter.
    #[tokio::test]
    async fn test_no_false_negatives() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let mut test_keys: Vec<Vec<u8>> = Vec::new();
        for i in 0u64..100 {
            let mut key = vec![0u8; 32];
            key[0..8].copy_from_slice(&i.to_le_bytes());
            test_keys.push(key);
        }

        // Create bloom for checkpoint 0 with all keys
        let key_refs: Vec<&[u8]> = test_keys.iter().map(|k| k.as_slice()).collect();
        let blooms = vec![make_checkpoint_bloom(0, &key_refs)];

        commit_blooms(blooms, &mut conn).await;

        // Load the committed bloom blocks
        let blocks = get_bloom_blocks(&mut conn, 0).await;
        assert!(!blocks.is_empty(), "Should have bloom blocks");

        let seed = 0u128;

        // Verify every key can be found (no false negatives)
        for (i, key) in test_keys.iter().enumerate() {
            let (block_idx, _) = CpBlockedBloomFilter::hash(key, seed);

            let block = blocks
                .iter()
                .find(|b| b.bloom_block_index == block_idx as i16)
                .unwrap_or_else(|| panic!("Block {} should exist for key {}", block_idx, i));

            assert!(
                block_contains_key(&block.bloom_filter, key, seed),
                "Key {} should be found in bloom filter (false negative detected!)",
                i
            );
        }
    }

    /// Verify no false negatives after SQL ON CONFLICT merge with bytea_or.
    ///
    /// This test specifically triggers the database-level merge by:
    /// 1. Committing batch 1 to DB (INSERT)
    /// 2. Committing batch 2 to DB (ON CONFLICT DO UPDATE with bytea_or)
    /// 3. Verifying keys from batch 1 survive the bytea_or merge
    #[tokio::test]
    async fn test_no_false_negatives_after_sql_merge() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let seed = 0u128;

        // Generate keys for batch 1 (checkpoints 0-1)
        let mut batch1_keys: Vec<Vec<u8>> = Vec::new();
        for i in 0u64..50 {
            let mut key = vec![0u8; 32];
            key[0..8].copy_from_slice(&i.to_le_bytes());
            batch1_keys.push(key);
        }

        // Generate keys for batch 2 (checkpoints 2-3) - different keys
        let mut batch2_keys: Vec<Vec<u8>> = Vec::new();
        for i in 50u64..100 {
            let mut key = vec![0u8; 32];
            key[0..8].copy_from_slice(&i.to_le_bytes());
            batch2_keys.push(key);
        }

        // Commit batch 1 - this does INSERT
        let refs1: Vec<&[u8]> = batch1_keys.iter().map(|k| k.as_slice()).collect();
        commit_blooms(vec![make_checkpoint_bloom(0, &refs1)], &mut conn).await;

        // Check batch 1 keys are in DB before merge
        let blocks_before = get_bloom_blocks(&mut conn, 0).await;
        for (i, key) in batch1_keys.iter().enumerate() {
            let (block_idx, _) = CpBlockedBloomFilter::hash(key, seed);
            let block = blocks_before
                .iter()
                .find(|b| b.bloom_block_index == block_idx as i16)
                .unwrap_or_else(|| panic!("Block should exist for batch1 key {} before merge", i));
            assert!(
                block_contains_key(&block.bloom_filter, key, seed),
                "Batch1 key {} should be found before merge",
                i
            );
        }

        // Commit batch 2 - this triggers ON CONFLICT DO UPDATE with bytea_or
        // because some bloom_block_indices will overlap with batch 1
        let refs2: Vec<&[u8]> = batch2_keys.iter().map(|k| k.as_slice()).collect();
        commit_blooms(vec![make_checkpoint_bloom(2, &refs2)], &mut conn).await;

        // Load merged bloom blocks
        let blocks_after = get_bloom_blocks(&mut conn, 0).await;

        // Check ALL keys from batch 1 survive the bytea_or merge
        for (i, key) in batch1_keys.iter().enumerate() {
            let (block_idx, _) = CpBlockedBloomFilter::hash(key, seed);

            let block = blocks_after
                .iter()
                .find(|b| b.bloom_block_index == block_idx as i16)
                .unwrap_or_else(|| panic!("Block should exist for batch1 key {} after merge", i));

            assert!(
                block_contains_key(&block.bloom_filter, key, seed),
                "Batch1 key {} should survive bytea_or merge (false negative!)",
                i
            );
        }

        // Check batch 2 keys are also present
        for (i, key) in batch2_keys.iter().enumerate() {
            let (block_idx, _) = CpBlockedBloomFilter::hash(key, seed);

            let block = blocks_after
                .iter()
                .find(|b| b.bloom_block_index == block_idx as i16)
                .unwrap_or_else(|| panic!("Block should exist for batch2 key {}", i + 50));

            assert!(
                block_contains_key(&block.bloom_filter, key, seed),
                "Batch2 key {} should be found after merge",
                i + 50
            );
        }
    }
}

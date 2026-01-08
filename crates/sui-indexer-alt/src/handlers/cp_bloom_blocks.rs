// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::{collections::HashSet, sync::Arc, time::Instant};

use anyhow::Result;
use async_trait::async_trait;
use diesel::upsert::excluded;
use diesel::{ExpressionMethods, define_sql_function, sql_types::Binary};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_schema::cp_bloom_blocks::{
    BLOOM_BLOCK_BITS, CP_BLOCK_SIZE, NUM_BLOOM_BLOCKS, NUM_HASHES, cp_block_id,
};
use tracing::debug;

use crate::handlers::cp_blooms::extract_filter_keys;
use sui_indexer_alt_framework::{
    pipeline::{Processor, sequential::Handler},
    postgres::{Connection, Db},
};
use sui_indexer_alt_schema::{
    blooms::blocked::BlockedBloomFilter,
    cp_bloom_blocks::{CheckpointItems, StoredCpBloomBlock, cp_block_seed},
    schema::cp_bloom_blocks,
};
use sui_types::full_checkpoint_content::Checkpoint;

// Define the bytea_or SQL function for merging bloom filters
define_sql_function! {
    /// Performs bitwise OR on two bytea values. Used for merging bloom filters.
    fn bytea_or(a: Binary, b: Binary) -> Binary;
}

/// Blocked bloom filters that span multiple checkpoints for efficient range queries.
///
/// Checkpoints are assigned to 1000-checkpoint blocks:
/// - `cp_block_id(cp_num) = cp_num / 1000`
/// - Block 0: checkpoints 0-999
/// - Block 1: checkpoints 1000-1999
/// - etc.
pub(crate) struct CpBloomBlocks;

#[async_trait]
impl Processor for CpBloomBlocks {
    const NAME: &'static str = "cp_bloom_blocks";

    type Value = CheckpointItems;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;

        let mut items = HashSet::new();
        for tx in checkpoint.transactions.iter() {
            items.extend(extract_filter_keys(tx));
        }

        Ok(vec![CheckpointItems {
            cp_sequence_number: cp_num as i64,
            items: items.into_iter().collect(),
        }])
    }
}

#[async_trait]
impl Handler for CpBloomBlocks {
    type Store = Db;
    type Batch = BTreeMap<i64, Vec<Self::Value>>;

    const MIN_EAGER_ROWS: usize = CP_BLOCK_SIZE as usize;
    const MAX_BATCH_CHECKPOINTS: usize = CP_BLOCK_SIZE as usize;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        for cp in values {
            let block_id = cp_block_id(cp.cp_sequence_number as u64);
            batch.entry(block_id).or_default().push(cp);
        }
    }

    /// Build bloom for the batch of checkpoints and merge with existing if present.
    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Build bloom rows for each cp_block
        let mut all_bloom_rows = Vec::new();
        let mut total_checkpoints = 0;
        for (block_id, checkpoints) in batch {
            total_checkpoints += checkpoints.len();
            let rows = Self::build_bloom_from_items(*block_id, checkpoints);
            all_bloom_rows.extend(rows);
        }

        let insert_start = Instant::now();

        // Upsert: merge bloom filters with OR, expand checkpoint range.
        let count = diesel::insert_into(cp_bloom_blocks::table)
            .values(&all_bloom_rows)
            .on_conflict((cp_bloom_blocks::cp_block_id, cp_bloom_blocks::bloom_block_index))
            .do_update()
            .set((
                // OR the bloom filters together using our custom SQL function
                cp_bloom_blocks::bloom_filter.eq(bytea_or(
                    cp_bloom_blocks::bloom_filter,
                    excluded(cp_bloom_blocks::bloom_filter),
                )),
                // Expand checkpoint range to cover both batches
                cp_bloom_blocks::cp_sequence_number_lo.eq(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "LEAST(cp_bloom_blocks.cp_sequence_number_lo, EXCLUDED.cp_sequence_number_lo)",
                )),
                cp_bloom_blocks::cp_sequence_number_hi.eq(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "GREATEST(cp_bloom_blocks.cp_sequence_number_hi, EXCLUDED.cp_sequence_number_hi)",
                )),
                // Sum the item counts (approximate since items may overlap)
                cp_bloom_blocks::num_items.eq(diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>>(
                    "COALESCE(cp_bloom_blocks.num_items, 0) + COALESCE(EXCLUDED.num_items, 0)",
                )),
            ))
            .execute(conn)
            .await?;

        let insert_elapsed = insert_start.elapsed().as_secs_f64() * 1000.0;

        debug!(
            cp_blocks = batch.len(),
            checkpoints = total_checkpoints,
            bloom_blocks = all_bloom_rows.len(),
            insert_ms = insert_elapsed,
            "built bloom from batch"
        );

        Ok(count)
    }
}

impl CpBloomBlocks {
    fn build_bloom_from_items(
        cp_block_id: i64,
        items: &[CheckpointItems],
    ) -> Vec<StoredCpBloomBlock> {
        let bloom_build_start = Instant::now();

        let mut all_items: HashSet<Vec<u8>> = HashSet::new();
        let mut cp_lo = i64::MAX;
        let mut cp_hi = i64::MIN;

        for cp in items {
            all_items.extend(cp.items.iter().cloned());
            cp_lo = cp_lo.min(cp.cp_sequence_number);
            cp_hi = cp_hi.max(cp.cp_sequence_number);
        }

        let seed = cp_block_seed(cp_block_id);
        let mut bloom =
            BlockedBloomFilter::new(seed, NUM_BLOOM_BLOCKS, BLOOM_BLOCK_BITS, NUM_HASHES);
        for item in &all_items {
            bloom.insert(item);
        }
        let bloom_insert_elapsed = bloom_build_start.elapsed().as_secs_f64() * 1000.0;

        let sparse_start = Instant::now();
        let blocks: Vec<_> = bloom
            .into_sparse_blocks()
            .into_iter()
            .map(|(idx, data)| StoredCpBloomBlock {
                cp_block_id,
                bloom_block_index: idx as i16,
                cp_sequence_number_lo: cp_lo,
                cp_sequence_number_hi: cp_hi,
                bloom_filter: data,
                num_items: Some(all_items.len() as i64),
            })
            .collect();
        let sparse_elapsed = sparse_start.elapsed().as_secs_f64() * 1000.0;

        debug!(
            cp_block_id,
            items = all_items.len(),
            cp_lo,
            cp_hi,
            bloom_insert_ms = bloom_insert_elapsed,
            sparse_ms = sparse_elapsed,
            "built bloom"
        );

        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use diesel::QueryDsl;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;

    use crate::MIGRATIONS;

    /// Check if a key is present in a bloom filter block.
    fn block_contains_key(block_data: &[u8], key: &[u8], seed: u128) -> bool {
        let (_, positions) =
            BlockedBloomFilter::hash(key, seed, NUM_BLOOM_BLOCKS, NUM_HASHES, BLOOM_BLOCK_BITS);
        positions
            .iter()
            .all(|&pos| (block_data[pos / 8] & (1 << (pos % 8))) != 0)
    }

    /// Load all bloom blocks for a given cp_block_id.
    async fn get_bloom_blocks(
        conn: &mut Connection<'_>,
        cp_block_id: i64,
    ) -> Vec<StoredCpBloomBlock> {
        cp_bloom_blocks::table
            .filter(cp_bloom_blocks::cp_block_id.eq(cp_block_id))
            .order_by(cp_bloom_blocks::bloom_block_index)
            .load(conn)
            .await
            .unwrap()
    }

    /// Helper to commit checkpoint items using the Handler trait methods.
    async fn commit_items(items: Vec<CheckpointItems>, conn: &mut Connection<'_>) -> usize {
        let handler = CpBloomBlocks;
        let mut batch = BTreeMap::new();
        handler.batch(&mut batch, items.into_iter());
        handler.commit(&batch, conn).await.unwrap()
    }

    #[tokio::test]
    async fn test_single_batch_insert() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // Create a batch with items for checkpoints 0-2
        let items = vec![
            CheckpointItems {
                cp_sequence_number: 0,
                items: vec![b"key_a".to_vec(), b"key_b".to_vec()],
            },
            CheckpointItems {
                cp_sequence_number: 1,
                items: vec![b"key_c".to_vec()],
            },
            CheckpointItems {
                cp_sequence_number: 2,
                items: vec![b"key_d".to_vec()],
            },
        ];

        let count = commit_items(items, &mut conn).await;
        assert!(count > 0, "Should insert at least one bloom block");

        let blocks = get_bloom_blocks(&mut conn, 0).await;
        assert!(
            !blocks.is_empty(),
            "Should have bloom blocks for cp_block 0"
        );

        // Check checkpoint range
        let first = &blocks[0];
        assert_eq!(first.cp_sequence_number_lo, 0);
        assert_eq!(first.cp_sequence_number_hi, 2);
        assert_eq!(first.num_items, Some(4)); // 4 unique items
    }

    #[tokio::test]
    async fn test_merge_on_conflict_combines_bloom_filters() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let seed = cp_block_seed(0);

        // Find two keys that hash to the same bloom_block_index to test the merge behavior.
        // This is important because merge only happens when there's a conflict on
        // (cp_block_id, bloom_block_index).
        let key1 = b"key_0".to_vec();
        let (target_block_idx, _) =
            BlockedBloomFilter::hash(&key1, seed, NUM_BLOOM_BLOCKS, NUM_HASHES, BLOOM_BLOCK_BITS);

        // key_104 hashes to the same bloom_block_index as key_0 with seed 0
        let key2 = b"key_104".to_vec();

        // First batch: checkpoints 0-1 with key1
        let items1 = vec![
            CheckpointItems {
                cp_sequence_number: 0,
                items: vec![key1.clone()],
            },
            CheckpointItems {
                cp_sequence_number: 1,
                items: vec![],
            },
        ];

        commit_items(items1, &mut conn).await;

        let blocks_after_first = get_bloom_blocks(&mut conn, 0).await;
        let first_block = blocks_after_first
            .iter()
            .find(|b| b.bloom_block_index == target_block_idx as i16)
            .expect("Block should exist for key1");
        assert_eq!(first_block.cp_sequence_number_lo, 0);
        assert_eq!(first_block.cp_sequence_number_hi, 1);
        assert_eq!(first_block.num_items, Some(1));

        // Second batch: checkpoints 2-3 with key2 (same bloom_block_index as key1, triggers merge)
        let items2 = vec![
            CheckpointItems {
                cp_sequence_number: 2,
                items: vec![key2.clone()],
            },
            CheckpointItems {
                cp_sequence_number: 3,
                items: vec![],
            },
        ];

        commit_items(items2, &mut conn).await;

        let blocks_after_merge = get_bloom_blocks(&mut conn, 0).await;

        // Find the merged block (same bloom_block_index as both keys)
        let merged_block = blocks_after_merge
            .iter()
            .find(|b| b.bloom_block_index == target_block_idx as i16)
            .expect("Merged block should exist");

        // The merge should have expanded the checkpoint range
        assert_eq!(
            merged_block.cp_sequence_number_lo, 0,
            "Lo should be min of both batches"
        );
        assert_eq!(
            merged_block.cp_sequence_number_hi, 3,
            "Hi should be max of both batches"
        );
        assert_eq!(
            merged_block.num_items,
            Some(2),
            "Item count should be sum of both batches"
        );

        // Verify both keys are present in the merged bloom filter (OR'd together)
        assert!(
            block_contains_key(&merged_block.bloom_filter, &key1, seed),
            "Merged bloom should contain key1 from first batch"
        );
        assert!(
            block_contains_key(&merged_block.bloom_filter, &key2, seed),
            "Merged bloom should contain key2 from second batch"
        );
    }

    #[tokio::test]
    async fn test_merge_preserves_original_keys() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let seed = cp_block_seed(0);

        // First batch with a specific key
        let items1 = vec![CheckpointItems {
            cp_sequence_number: 0,
            items: vec![b"original_key".to_vec()],
        }];

        commit_items(items1, &mut conn).await;

        // Second batch with different key
        let items2 = vec![CheckpointItems {
            cp_sequence_number: 1,
            items: vec![b"new_key".to_vec()],
        }];

        commit_items(items2, &mut conn).await;

        // Verify original key is still present
        let blocks = get_bloom_blocks(&mut conn, 0).await;
        let (original_block_idx, _) = BlockedBloomFilter::hash(
            b"original_key",
            seed,
            NUM_BLOOM_BLOCKS,
            NUM_HASHES,
            BLOOM_BLOCK_BITS,
        );
        let block = blocks
            .iter()
            .find(|b| b.bloom_block_index == original_block_idx as i16)
            .expect("Block for original key should exist");

        assert!(
            block_contains_key(&block.bloom_filter, b"original_key", seed),
            "Original key should still be present after merge"
        );
    }

    #[tokio::test]
    async fn test_idempotent_commit() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let items = vec![CheckpointItems {
            cp_sequence_number: 0,
            items: vec![b"test_key".to_vec()],
        }];

        // Commit twice with same data
        commit_items(items.clone(), &mut conn).await;
        commit_items(items, &mut conn).await;

        let blocks = get_bloom_blocks(&mut conn, 0).await;
        let block = &blocks[0];

        assert_eq!(block.cp_sequence_number_lo, 0);
        assert_eq!(block.cp_sequence_number_hi, 0);

        // Verify the key is still present
        let seed = cp_block_seed(0);
        assert!(
            block_contains_key(&block.bloom_filter, b"test_key", seed),
            "Key should be present after idempotent commit"
        );
    }

    #[tokio::test]
    async fn test_different_cp_blocks_are_separate() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // Batch for cp_block 0 (checkpoints 0-999)
        let items0 = vec![CheckpointItems {
            cp_sequence_number: 500,
            items: vec![b"block0_key".to_vec()],
        }];

        // Batch for cp_block 1 (checkpoints 1000-1999)
        let items1 = vec![CheckpointItems {
            cp_sequence_number: 1500,
            items: vec![b"block1_key".to_vec()],
        }];

        commit_items(items0, &mut conn).await;
        commit_items(items1, &mut conn).await;

        let blocks0 = get_bloom_blocks(&mut conn, 0).await;
        let blocks1 = get_bloom_blocks(&mut conn, 1).await;

        assert!(!blocks0.is_empty(), "cp_block 0 should have data");
        assert!(!blocks1.is_empty(), "cp_block 1 should have data");

        // Verify they have different checkpoint ranges
        assert_eq!(blocks0[0].cp_sequence_number_lo, 500);
        assert_eq!(blocks0[0].cp_sequence_number_hi, 500);
        assert_eq!(blocks1[0].cp_sequence_number_lo, 1500);
        assert_eq!(blocks1[0].cp_sequence_number_hi, 1500);
    }
}

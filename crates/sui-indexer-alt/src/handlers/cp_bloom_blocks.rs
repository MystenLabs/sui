// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tracing::{debug, warn};

use crate::handlers::cp_blooms::extract_filter_keys;
use sui_indexer_alt_framework::{
    pipeline::{
        Processor,
        concurrent::{BatchStatus, Handler},
    },
    postgres::{Connection, Db},
};
use sui_indexer_alt_schema::{
    blooms::BlockedBloomFilter,
    cp_bloom_blocks::{
        CP_BLOCK_SIZE, CheckpointItems, StoredCpBloomBlock, cp_block_id, cp_block_seed,
    },
    cp_bloom_items_pending::StoredCpBloomItemsPending,
    schema::{cp_bloom_blocks, cp_bloom_items_pending},
};
use sui_types::full_checkpoint_content::Checkpoint;

/// Indexes blocked bloom filters that span multiple checkpoints for efficient range queries.
///
/// This pipeline uses a two-phase commit process:
///
/// 1. **Pending**: Checkpoint items are written to partitioned `cp_bloom_items_pending` table
/// 2. **Bloom Building**: Once 1000 checkpoints accumulate for a cp_block, build the bloom
///
/// Checkpoints are assigned to 1000-checkpoint blocks:
/// - `cp_block_id(cp_num) = cp_num / 1000`
/// - Block 0: checkpoints 0-999
/// - Block 1: checkpoints 1000-1999
/// - etc.
///
/// Pending table is LIST partitioned by cp_block_id and cleaned up via DROP TABLE
pub(crate) struct CpBloomBlocks;

#[async_trait]
impl Processor for CpBloomBlocks {
    const NAME: &'static str = "cp_bloom_blocks";

    type Value = CheckpointItems;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;
        let cp_block_id = cp_block_id(cp_num);

        let mut items = HashSet::new();
        for tx in checkpoint.transactions.iter() {
            items.extend(extract_filter_keys(tx));
        }

        Ok(vec![CheckpointItems {
            cp_sequence_number: cp_num as i64,
            cp_block_id,
            items: items.into_iter().collect(),
        }])
    }
}

#[async_trait]
impl Handler for CpBloomBlocks {
    type Store = Db;
    type Batch = HashMap<i64, Vec<CheckpointItems>>;

    const MIN_EAGER_ROWS: usize = 1000;
    const MAX_PENDING_ROWS: usize = 10000;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        for value in values {
            batch.entry(value.cp_block_id).or_default().push(value);
        }
        BatchStatus::Pending
    }

    /// Two-phase commit: write to pending table, then opportunistically build blooms.
    ///
    /// Phase 1: Write current batch to pending partitions
    /// - Creates partition for each cp_block_id if needed
    /// - Uses `on_conflict_do_nothing` to handle concurrent writes
    ///
    /// Phase 2: Check pending table and build blooms for complete cp_blocks
    /// - Queries pending table to find cp_blocks with >= 1000 checkpoints
    /// - Builds blocked bloom filter from accumulated pending items
    /// - Drops pending partition after successful bloom insertion
    /// - Handles race conditions gracefully (concurrent bloom building)
    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        let mut inserted = 0;

        // Phase 1: Write checkpoint items to pending table
        for (&cp_block_id, checkpoint_items) in batch {
            // Create partition for this cp_block_id if it doesn't exist
            Self::create_partition_if_not_exists(conn, cp_block_id).await?;

            let pending_entries: Vec<_> = checkpoint_items
                .iter()
                .map(|cp| StoredCpBloomItemsPending {
                    cp_block_id,
                    cp_sequence_number: cp.cp_sequence_number,
                    items: cp.items.iter().cloned().map(Some).collect(),
                })
                .collect();

            diesel::insert_into(cp_bloom_items_pending::table)
                .values(&pending_entries)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?;
        }

        // Phase 2: Check pending table and opportunistically build blooms for complete cp_blocks
        // Find cp_blocks with >= 1000 checkpoints
        use diesel::dsl::count_star;
        let cp_block_ids: Vec<i64> = batch.keys().copied().collect();
        let ready_blocks: Vec<(i64, i64)> = cp_bloom_items_pending::table
            .filter(cp_bloom_items_pending::cp_block_id.eq_any(&cp_block_ids))
            .group_by(cp_bloom_items_pending::cp_block_id)
            .select((cp_bloom_items_pending::cp_block_id, count_star()))
            .having(count_star().ge(CP_BLOCK_SIZE as i64))
            .load(conn)
            .await?;

        if ready_blocks.is_empty() {
            return Ok(0);
        }

        // Load items for all ready blocks in a single batched query
        let pending_load_start = Instant::now();
        let ready_block_ids: Vec<i64> = ready_blocks.iter().map(|(id, _)| *id).collect();
        let all_pending_items: Vec<StoredCpBloomItemsPending> = cp_bloom_items_pending::table
            .filter(cp_bloom_items_pending::cp_block_id.eq_any(&ready_block_ids))
            .load(conn)
            .await?;
        let pending_load_elapsed = pending_load_start.elapsed().as_secs_f64() * 1000.0;

        // Group items by cp_block_id
        let mut items_by_block: HashMap<i64, Vec<StoredCpBloomItemsPending>> = HashMap::new();
        for item in all_pending_items {
            items_by_block
                .entry(item.cp_block_id)
                .or_default()
                .push(item);
        }

        // Build blooms for all ready blocks
        for (cp_block_id, checkpoint_count) in ready_blocks {
            let Some(pending_items) = items_by_block.get(&cp_block_id) else {
                debug!(
                    cp_block_id,
                    "pending items already cleaned up by concurrent committer"
                );
                continue;
            };
            debug!(
                cp_block_id,
                checkpoint_count,
                items_loaded = pending_items.len(),
                pending_load_ms = pending_load_elapsed,
                "building bloom for complete cp_block"
            );

            let mut items: HashSet<Vec<u8>> = HashSet::new();
            let mut cp_lo = i64::MAX;
            let mut cp_hi = i64::MIN;

            for pending in pending_items {
                items.extend(pending.items.iter().flatten().cloned());
                cp_lo = cp_lo.min(pending.cp_sequence_number);
                cp_hi = cp_hi.max(pending.cp_sequence_number);
            }

            if pending_items.len() < CP_BLOCK_SIZE as usize {
                warn!(
                    cp_block_id,
                    count = pending_items.len(),
                    "building bloom with incomplete data"
                );
            }

            let bloom_build_start = Instant::now();
            let seed = cp_block_seed(cp_block_id);
            let mut bloom = BlockedBloomFilter::new(seed);

            for item in &items {
                bloom.insert(item);
            }
            let bloom_insert_elapsed = bloom_build_start.elapsed().as_secs_f64() * 1000.0;

            let sparse_start = Instant::now();
            let blocks: Vec<_> = bloom
                .to_sparse_blocks()
                .into_iter()
                .map(|(idx, data)| StoredCpBloomBlock {
                    cp_block_id,
                    bloom_block_index: idx as i16,
                    cp_sequence_number_lo: cp_lo,
                    cp_sequence_number_hi: cp_hi,
                    bloom_filter: data,
                    num_items: Some(items.len() as i64),
                })
                .collect();
            let sparse_elapsed = sparse_start.elapsed().as_secs_f64() * 1000.0;

            debug!(
                cp_block_id,
                items = items.len(),
                cp_lo,
                cp_hi,
                bloom_insert_ms = bloom_insert_elapsed,
                sparse_ms = sparse_elapsed,
                "built bloom"
            );

            if !blocks.is_empty() {
                let insert_start = Instant::now();
                let count = diesel::insert_into(cp_bloom_blocks::table)
                    .values(&blocks)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                let insert_elapsed = insert_start.elapsed().as_secs_f64() * 1000.0;

                inserted += count;

                if count > 0 {
                    let drop_start = Instant::now();
                    Self::drop_partition(conn, cp_block_id).await?;
                    let drop_elapsed = drop_start.elapsed().as_secs_f64() * 1000.0;

                    debug!(
                        cp_block_id,
                        insert_ms = insert_elapsed,
                        drop_ms = drop_elapsed,
                        "dropped pending partition"
                    );
                } else if count == 0 {
                    debug!(cp_block_id, "bloom already built by concurrent thread");
                }
            }
        }

        Ok(inserted)
    }
}

impl CpBloomBlocks {
    async fn create_partition_if_not_exists<'a>(
        conn: &mut Connection<'a>,
        cp_block_id: i64,
    ) -> Result<()> {
        let query = format!(
            "CREATE TABLE IF NOT EXISTS cp_bloom_items_pending_{} \
             PARTITION OF cp_bloom_items_pending FOR VALUES IN ({})",
            cp_block_id, cp_block_id
        );

        diesel::sql_query(&query).execute(conn).await?;
        Ok(())
    }

    async fn drop_partition<'a>(conn: &mut Connection<'a>, cp_block_id: i64) -> Result<()> {
        let query = format!(
            "DROP TABLE IF EXISTS cp_bloom_items_pending_{}",
            cp_block_id
        );

        diesel::sql_query(&query).execute(conn).await?;
        Ok(())
    }
}

/// Clean up orphaned pending partitions at startup.
///
/// Pending partitions can be left behind when the blocked bloom was successfully built and inserted
/// but the partition was not dropped.
pub(crate) async fn cleanup_orphaned_pending_partitions<'a>(
    conn: &mut Connection<'a>,
) -> Result<()> {
    #[derive(diesel::QueryableByName)]
    struct PartitionName {
        #[diesel(sql_type = diesel::sql_types::Text)]
        tablename: String,
    }

    let partitions: Vec<PartitionName> = diesel::sql_query(
        "SELECT tablename FROM pg_tables \
         WHERE schemaname = 'public' \
         AND tablename LIKE 'cp_bloom_items_pending_%'",
    )
    .load(conn)
    .await?;

    for partition in partitions {
        if let Some(block_id_str) = partition.tablename.strip_prefix("cp_bloom_items_pending_")
            && let Ok(cp_block_id) = block_id_str.parse::<i64>()
        {
            // Check if bloom exists for this block
            let bloom_exists: bool = diesel::select(diesel::dsl::exists(
                cp_bloom_blocks::table.filter(cp_bloom_blocks::cp_block_id.eq(cp_block_id)),
            ))
            .get_result(conn)
            .await?;

            if bloom_exists {
                debug!(cp_block_id, "dropping leftover pending partition");
                CpBloomBlocks::drop_partition(conn, cp_block_id).await?;
            }
        }
    }

    Ok(())
}

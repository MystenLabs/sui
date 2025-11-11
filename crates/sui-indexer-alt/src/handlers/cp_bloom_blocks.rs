// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{Processor, sequential::Handler},
    postgres::{Connection, Db},
};
use sui_indexer_alt_schema::{
    blooms::BlockedBloomFilter,
    cp_bloom_blocks::{
        CP_BLOCK_SIZE, CheckpointItems, StoredCpBloomBlock, cp_block_id, cp_block_seed,
    },
    cp_bloom_items_wal::StoredCpBloomItemsWal,
    schema::{cp_bloom_blocks, cp_bloom_items_wal},
};
use sui_types::full_checkpoint_content::Checkpoint;
use tracing::{debug, warn};

use crate::handlers::cp_blooms::extract_filter_keys;

/// Indexes blocked bloom filters that span multiple checkpoints for efficient range queries.
///
/// Checkpoint items are first written to `cp_bloom_items_wal`, then
/// aggregated into `cp_bloom_blocks` once all checkpoints in a block are available.
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

        if items.is_empty() {
            return Ok(vec![]);
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

    const MAX_BATCH_CHECKPOINTS: usize = CP_BLOCK_SIZE as usize;
    const MIN_EAGER_ROWS: usize = CP_BLOCK_SIZE as usize;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        for value in values {
            batch.entry(value.cp_block_id).or_default().push(value);
        }
    }

    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        let mut inserted = 0;

        // Write checkpoint items to WAL
        for (&cp_block_id, checkpoint_items) in batch {
            let wal_entries: Vec<_> = checkpoint_items
                .iter()
                .map(|cp| StoredCpBloomItemsWal {
                    cp_block_id,
                    cp_sequence_number: cp.cp_sequence_number,
                    items: cp.items.iter().cloned().map(Some).collect(),
                })
                .collect();

            diesel::insert_into(cp_bloom_items_wal::table)
                .values(&wal_entries)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?;
        }

        // Check which cp_blocks have enough data to build blooms
        for &cp_block_id in batch.keys() {
            let bloom_exists: i64 = cp_bloom_blocks::table
                .filter(cp_bloom_blocks::cp_block_id.eq(cp_block_id))
                .count()
                .get_result(conn)
                .await?;

            if bloom_exists > 0 {
                debug!(cp_block_id, "bloom already exists");
                continue;
            }

            let checkpoint_count: i64 = cp_bloom_items_wal::table
                .filter(cp_bloom_items_wal::cp_block_id.eq(cp_block_id))
                .count()
                .get_result(conn)
                .await?;

            if checkpoint_count < CP_BLOCK_SIZE as i64 {
                debug!(cp_block_id, checkpoint_count, "waiting for full cp_block");
                continue;
            }

            // Build bloom from accumulated WAL items
            let wal_items: Vec<StoredCpBloomItemsWal> = cp_bloom_items_wal::table
                .filter(cp_bloom_items_wal::cp_block_id.eq(cp_block_id))
                .load(conn)
                .await?;

            let mut items = HashSet::new();
            let mut cp_lo = i64::MAX;
            let mut cp_hi = i64::MIN;

            for wal in &wal_items {
                items.extend(wal.items.iter().flatten().cloned());
                cp_lo = cp_lo.min(wal.cp_sequence_number);
                cp_hi = cp_hi.max(wal.cp_sequence_number);
            }

            if wal_items.len() < CP_BLOCK_SIZE as usize {
                warn!(
                    cp_block_id,
                    count = wal_items.len(),
                    "building bloom with incomplete data"
                );
            }

            debug!(
                cp_block_id,
                items = items.len(),
                cp_lo,
                cp_hi,
                "building bloom"
            );

            let seed = cp_block_seed(cp_block_id);
            let mut bloom = BlockedBloomFilter::new(seed);

            for item in &items {
                bloom.insert(item);
            }

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

            if !blocks.is_empty() {
                let count = diesel::insert_into(cp_bloom_blocks::table)
                    .values(&blocks)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                inserted += count;

                if count > 0 {
                    let deleted = diesel::delete(cp_bloom_items_wal::table)
                        .filter(cp_bloom_items_wal::cp_block_id.eq(cp_block_id))
                        .execute(conn)
                        .await?;

                    debug!(cp_block_id, deleted, "cleaned up WAL");
                }
            }
        }

        Ok(inserted)
    }
}

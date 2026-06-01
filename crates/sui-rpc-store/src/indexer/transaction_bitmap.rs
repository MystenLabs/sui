// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::transaction_bitmap`](crate::schema::transaction_bitmap)
//! CF.
//!
//! Mirrors the tx-space half of
//! `write_ledger_history_rows_for_checkpoint` in
//! `sui-core::rpc_index`. For every transaction in the checkpoint
//! the pipeline:
//!
//! 1. Visits every dimension candidate via
//!    [`sui_inverted_index::for_each_transaction_dimension`].
//! 2. Encodes each `(dimension, value)` into a `dimension_key` via
//!    [`sui_inverted_index::encode_dimension_key`] and dedupes
//!    per-tx, so a transaction matching the same dimension
//!    multiple times contributes a single bit per
//!    `(dim_key, bucket)`.
//! 3. Groups `tx_seq` bits by `(dim_key, tx_seq / TX_BUCKET_SIZE)`,
//!    folding any number of transactions in the checkpoint into
//!    one `RoaringBitmap` per group.
//!
//! Multiple checkpoints landing in the same commit batch are
//! folded into the same `RoaringBitmap` per group via the
//! handler's `batch` callback, so the commit path emits one
//! merge operand per `(dim_key, bucket)` regardless of how many
//! checkpoints contributed.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use roaring::RoaringBitmap;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::for_each_transaction_dimension;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::transaction_bitmap;
use crate::schema::transaction_bitmap::TX_BUCKET_SIZE;
use crate::schema::transaction_bitmap::bit_of;
use crate::schema::transaction_bitmap::bucket_of;

/// Pipeline marker for `transaction_bitmap`.
pub struct TransactionBitmap;

/// One pre-built bitmap for a single `(dimension_key, bucket)`
/// pair, ready to be staged as a merge operand against the CF.
pub struct Row {
    pub dimension_key: Vec<u8>,
    pub bucket: u64,
    pub bitmap: RoaringBitmap,
}

#[async_trait]
impl Processor for TransactionBitmap {
    const NAME: &'static str = "transaction_bitmap";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut groups: HashMap<(Vec<u8>, u64), RoaringBitmap> = HashMap::new();
        let mut dim_keys: HashSet<Vec<u8>> = HashSet::new();

        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_seq_at(checkpoint, i);
            let bucket = bucket_of(tx_seq);
            let bit = bit_of(tx_seq);

            // Dedupe `(dim, value)` pairs that occur multiple
            // times in the same tx (e.g. AffectedAddress when an
            // address shows up in several object changes).
            // Without this we'd add the same bit to the bitmap
            // repeatedly — not incorrect, but redundant work.
            dim_keys.clear();
            for_each_transaction_dimension(
                &tx.transaction,
                &tx.effects,
                tx.events.as_ref(),
                &checkpoint.object_set,
                |dim, value| {
                    dim_keys.insert(encode_dimension_key(dim, value));
                },
            );

            for dim_key in dim_keys.drain() {
                groups
                    .entry((dim_key, bucket))
                    .or_default()
                    .insert(bit);
            }
        }

        Ok(groups
            .into_iter()
            .map(|((dim_key, bucket), bitmap)| Row {
                dimension_key: dim_key,
                bucket,
                bitmap,
            })
            .collect())
    }
}

#[async_trait]
impl sequential::Handler for TransactionBitmap {
    type Store = Store;
    /// Fold operands from multiple checkpoints together so we
    /// emit at most one merge operand per `(dim_key, bucket)`
    /// per commit — even if many checkpoints land in the same
    /// batch.
    type Batch = HashMap<(Vec<u8>, u64), RoaringBitmap>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Row>) {
        for row in values {
            let entry = batch
                .entry((row.dimension_key, row.bucket))
                .or_default();
            *entry |= row.bitmap;
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().transaction_bitmap;
        for ((dim_key, bucket), bitmap) in batch {
            let (k, v) =
                transaction_bitmap::store_bitmap(dim_key.clone(), *bucket, bitmap.clone());
            conn.batch.merge(cf, &k, &v)?;
        }
        Ok(batch.len())
    }
}

// Re-export for documentation cross-referencing — silence the
// "unused import" lint without an `#[allow]`.
#[allow(dead_code)]
const _BUCKET_SIZE_DOC: u64 = TX_BUCKET_SIZE;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _ = TransactionBitmap.process(&checkpoint).await.unwrap();
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::event_bitmap`](crate::schema::event_bitmap) CF.
//!
//! For every event in every transaction the pipeline:
//!
//! 1. Visits dimension candidates via
//!    [`sui_inverted_index::for_each_event_dimension`].
//! 2. Encodes the dimension key via
//!    [`sui_inverted_index::encode_dimension_key`].
//! 3. Packs `(tx_seq, event_idx)` into the event-seq space the
//!    schema uses (`pack(tx_seq, event_idx)`), checks the packing
//!    doesn't overflow the per-tx event limit (`1 << EVENT_BITS`)
//!    or the `tx_seq` ceiling (`u64::MAX >> EVENT_BITS`), and
//!    groups the bit into `(dim_key, packed / EVENT_BUCKET_SIZE)`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use roaring::RoaringBitmap;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::for_each_event_dimension;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::event_bitmap;
use crate::schema::event_bitmap::EVENT_BITS;
use crate::schema::event_bitmap::bit_of;
use crate::schema::event_bitmap::bucket_of;
use crate::schema::event_bitmap::pack;

/// Maximum events a single transaction can contribute before the
/// packed event-seq space would collide with the next
/// transaction's range.
const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;

/// Maximum `tx_seq` value whose `<< EVENT_BITS` still fits in a
/// `u64`. Anything beyond this would lose its high bits during
/// packing.
const MAX_TX_SEQ: u64 = u64::MAX >> EVENT_BITS;

/// Pipeline marker for `event_bitmap`.
pub struct EventBitmap;

/// One pre-built bitmap for a single `(dimension_key, bucket)`
/// pair, ready to be staged as a merge operand against the CF.
pub struct Row {
    pub dimension_key: Vec<u8>,
    pub bucket: u64,
    pub bitmap: RoaringBitmap,
}

#[async_trait]
impl Processor for EventBitmap {
    const NAME: &'static str = "event_bitmap";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut groups: HashMap<(Vec<u8>, u64), RoaringBitmap> = HashMap::new();

        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_seq_at(checkpoint, i);
            if tx_seq > MAX_TX_SEQ {
                anyhow::bail!("tx_seq {tx_seq} exceeds packed event-seq limit {MAX_TX_SEQ}",);
            }
            let sender = tx.transaction.sender();

            // `for_each_event_dimension` can't propagate errors,
            // so a packing failure has to be captured and
            // surfaced afterwards.
            let mut packing_error: Option<anyhow::Error> = None;
            for_each_event_dimension(
                sender,
                &tx.effects,
                tx.events.as_ref(),
                |event_idx, dim, value| {
                    if packing_error.is_some() {
                        return;
                    }
                    if event_idx >= MAX_EVENTS_PER_TX {
                        packing_error = Some(anyhow::anyhow!(
                            "event_idx {event_idx} exceeds packed event-seq limit {}",
                            MAX_EVENTS_PER_TX - 1,
                        ));
                        return;
                    }
                    let packed = pack(tx_seq, event_idx);
                    let bucket = bucket_of(packed);
                    let bit = bit_of(packed);
                    groups
                        .entry((encode_dimension_key(dim, value), bucket))
                        .or_default()
                        .insert(bit);
                },
            );
            if let Some(e) = packing_error {
                return Err(e);
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
impl sequential::Handler for EventBitmap {
    type Store = Store;
    /// Fold operands from multiple checkpoints together so the
    /// commit path stages at most one merge operand per
    /// `(dim_key, bucket)` per commit.
    type Batch = HashMap<(Vec<u8>, u64), RoaringBitmap>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Row>) {
        for row in values {
            let entry = batch.entry((row.dimension_key, row.bucket)).or_default();
            *entry |= row.bitmap;
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().event_bitmap;
        for ((dim_key, bucket), bitmap) in batch {
            let (k, v) = event_bitmap::store_bitmap(dim_key.clone(), *bucket, bitmap.clone());
            conn.batch.merge(cf, &k, &v)?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _ = EventBitmap.process(&checkpoint).await.unwrap();
    }
}

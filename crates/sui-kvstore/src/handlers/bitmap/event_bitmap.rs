// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Event-keyed Roaring bitmap inverted index processor.
//!
//! Parallel to [`super::transaction_processor`], but bit positions
//! correspond to packed `event_seq`s (see [`crate::tables::event_bitmap_index`])
//! rather than `tx_sequence_number`s. Enables `list_events` to resolve matches
//! directly in event-space with no over-fetch.

use std::sync::Arc;

use bytes::Bytes;
use roaring::RoaringBitmap;
use rustc_hash::FxHashMap;
use sui_index_dimensions::for_each_event_dimension;
use sui_index_dimensions::write_dimension_key;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::event_bitmap_index;

use crate::handlers::bitmap::BitmapIndexProcessor;
use crate::handlers::bitmap::BitmapIndexValue;

// Compile-time check that BUCKET_SIZE fits in u32 (required for RoaringBitmap bit positions).
const _: () = assert!(event_bitmap_index::BUCKET_SIZE <= u32::MAX as u64);

/// Event-keyed bitmap index: one bit per (dimension, packed event_seq).
pub struct EventBitmapProcessor;

#[async_trait::async_trait]
impl Processor for EventBitmapProcessor {
    const NAME: &'static str = "kvstore_event_bitmap_index";
    type Value = BitmapIndexValue;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp = checkpoint.summary.data();
        let max_cp = cp.sequence_number;
        let max_ts_ms = cp.timestamp_ms;
        // network_total_transactions is cumulative *including* this checkpoint,
        // so tx_lo is the first tx_seq in this checkpoint.
        let tx_lo = cp.network_total_transactions - checkpoint.transactions.len() as u64;

        // See [`crate::handlers::bitmap::transaction_bitmap`] for the
        // rationale behind the `Vec<u8>`-keyed map + on-miss-only
        // allocation pattern.
        let mut rows: FxHashMap<Vec<u8>, (u64, RoaringBitmap)> = FxHashMap::default();
        let mut dimension_key = Vec::new();
        let mut row_key = Vec::new();
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_lo + i as u64;
            for_each_event_dimension(tx, |event_idx, dim, value| {
                let event_seq = event_bitmap_index::encode_event_seq(tx_seq, event_idx);
                let bucket_id = event_seq / event_bitmap_index::BUCKET_SIZE;
                let bit_position = (event_seq % event_bitmap_index::BUCKET_SIZE) as u32;
                write_dimension_key(&mut dimension_key, dim, value);
                event_bitmap_index::encode_row_key_into(
                    &mut row_key,
                    event_bitmap_index::SCHEMA_VERSION,
                    &dimension_key,
                    bucket_id,
                );
                if let Some((_, bm)) = rows.get_mut(row_key.as_slice()) {
                    bm.insert(bit_position);
                } else {
                    let mut bm = RoaringBitmap::new();
                    bm.insert(bit_position);
                    rows.insert(row_key.clone(), (bucket_id, bm));
                }
            });
        }

        Ok(rows
            .into_iter()
            .map(|(row_key, (bucket_id, bitmap))| BitmapIndexValue {
                row_key: Bytes::from(row_key),
                bucket_id,
                bitmap,
                max_cp,
                max_ts_ms,
            })
            .collect())
    }
}

impl BitmapIndexProcessor for EventBitmapProcessor {
    const TABLE: &'static str = event_bitmap_index::NAME;
    const COLUMN: &'static str = event_bitmap_index::col::BITMAP;

    fn seal_tx_hi_exclusive(bucket_id: u64) -> u64 {
        // Bucket B is sealed once every future tx's smallest event_seq
        // (`event_seq_lo(tx) = tx * MAX_EVENTS_PER_TX`) is past bucket B's
        // upper end. Solve for the smallest tx satisfying that.
        ((bucket_id + 1) * event_bitmap_index::BUCKET_SIZE)
            .div_ceil(event_bitmap_index::MAX_EVENTS_PER_TX as u64)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use move_core_types::ident_str;
    use sui_index_dimensions::IndexDimension;
    use sui_index_dimensions::encode_dimension_key;
    use sui_types::base_types::ObjectID;
    use sui_types::event::Event;
    use sui_types::gas_coin::GAS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn preserves_duplicate_dimensions_across_distinct_events() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .with_events(vec![
                Event::new(
                    &ObjectID::ZERO,
                    ident_str!("dup_mod"),
                    TestCheckpointBuilder::derive_address(1),
                    GAS::type_(),
                    vec![],
                ),
                Event::new(
                    &ObjectID::ZERO,
                    ident_str!("dup_mod"),
                    TestCheckpointBuilder::derive_address(1),
                    GAS::type_(),
                    vec![],
                ),
            ])
            .finish_transaction()
            .build_checkpoint();

        let values = EventBitmapProcessor
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let sender_dim_key = encode_dimension_key(
            IndexDimension::Sender,
            TestCheckpointBuilder::derive_address(1).as_ref(),
        );
        let sender_row_key = event_bitmap_index::encode_row_key(
            event_bitmap_index::SCHEMA_VERSION,
            &sender_dim_key,
            0,
        );

        let sender_values: Vec<_> = values
            .iter()
            .filter(|v| v.row_key.as_ref() == sender_row_key.as_slice())
            .collect();

        assert!(!values.is_empty());
        assert_eq!(
            sender_values.len(),
            1,
            "processor groups both events' bits for the same sender row into one value"
        );
        assert_eq!(
            sender_values[0].bitmap.len(),
            2,
            "two events contribute two distinct event_seq bits"
        );
    }
}

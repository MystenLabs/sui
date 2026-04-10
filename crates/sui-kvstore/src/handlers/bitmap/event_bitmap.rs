// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Event-keyed Roaring bitmap inverted index processor.
//!
//! Parallel to [`super::transaction_bitmap`], but bit positions
//! correspond to packed `event_seq`s (see [`crate::tables::event_bitmap_index`])
//! rather than `tx_sequence_number`s. Enables `list_events` to resolve matches
//! directly in event-space with no over-fetch.

use sui_index_dimensions::IndexDimension;
use sui_index_dimensions::for_each_event_dimension;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::event_bitmap_index;

use crate::handlers::bitmap::BitmapIndexProcessor;

// Compile-time check that BUCKET_SIZE fits in u32 (required for RoaringBitmap bit positions).
const _: () = assert!(event_bitmap_index::BUCKET_SIZE <= u32::MAX as u64);

/// Event-keyed bitmap index: one bit per (dimension, packed event_seq).
pub struct EventBitmapProcessor;

impl BitmapIndexProcessor for EventBitmapProcessor {
    const NAME: &'static str = "kvstore_event_dimensions";
    const TABLE: &'static str = "event_bitmap_index";
    const COLUMN: &'static str = event_bitmap_index::col::BITMAP;
    const SCHEMA_VERSION: u32 = event_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = event_bitmap_index::BUCKET_ID_WIDTH;

    fn for_each_indexed_bit<F>(&self, checkpoint: &Checkpoint, mut emit: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]),
    {
        let cp = checkpoint.summary.data();
        // network_total_transactions is cumulative *including* this checkpoint,
        // so tx_lo is the first tx_seq in this checkpoint.
        let tx_lo = cp.network_total_transactions - checkpoint.transactions.len() as u64;

        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_lo + i as u64;
            for_each_event_dimension(tx, |event_idx, dim, value| {
                let event_seq = event_bitmap_index::encode_event_seq(tx_seq, event_idx);
                let bucket_id = event_seq / event_bitmap_index::BUCKET_SIZE;
                let bit_position = (event_seq % event_bitmap_index::BUCKET_SIZE) as u32;
                emit(bucket_id, bit_position, dim, value);
            });
        }
    }

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
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_types::base_types::ObjectID;
    use sui_types::event::Event;
    use sui_types::gas_coin::GAS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use crate::handlers::bitmap::BitmapIndexHandler;
    use crate::handlers::bitmap::BitmapIndexValue;

    use super::*;

    fn event(sender_idx: u8) -> Event {
        Event::new(
            &ObjectID::ZERO,
            ident_str!("dup_mod"),
            TestCheckpointBuilder::derive_address(sender_idx),
            GAS::type_(),
            vec![],
        )
    }

    fn sender_row_key(sender_idx: u8, bucket_id: u64) -> Vec<u8> {
        let sender = TestCheckpointBuilder::derive_address(sender_idx);
        let dimension_key = encode_dimension_key(IndexDimension::Sender, sender.as_ref());
        event_bitmap_index::encode_row_key(
            event_bitmap_index::SCHEMA_VERSION,
            &dimension_key,
            bucket_id,
        )
    }

    fn row<'a>(values: &'a [BitmapIndexValue], row_key: &[u8]) -> &'a BitmapIndexValue {
        values
            .iter()
            .find(|v| v.row_key.as_ref() == row_key)
            .expect("row must be emitted")
    }

    #[tokio::test]
    async fn preserves_duplicate_dimensions_across_distinct_events() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .with_events(vec![event(1), event(1)])
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(EventBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = sender_row_key(1, 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bucket_id, 0);
        assert_eq!(value.bitmap.len(), 2);
        assert!(value.bitmap.contains(0));
        assert!(value.bitmap.contains(1));
    }

    #[tokio::test]
    async fn same_dimension_across_transactions_sets_packed_event_bits() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .with_network_total_transactions(100)
            .start_transaction(1)
            .with_events(vec![event(1)])
            .finish_transaction()
            .start_transaction(1)
            .with_events(vec![event(1)])
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(EventBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = sender_row_key(1, 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bucket_id, 0);
        assert_eq!(value.bitmap.len(), 2);
        assert!(
            value
                .bitmap
                .contains(event_bitmap_index::event_seq_lo(100) as u32)
        );
        assert!(
            value
                .bitmap
                .contains(event_bitmap_index::event_seq_lo(101) as u32)
        );
    }

    #[tokio::test]
    async fn transactions_crossing_event_bucket_boundary_emit_distinct_bucket_rows() {
        let txs_per_bucket =
            event_bitmap_index::BUCKET_SIZE / event_bitmap_index::MAX_EVENTS_PER_TX as u64;
        let checkpoint = TestCheckpointBuilder::new(0)
            .with_network_total_transactions(txs_per_bucket - 1)
            .start_transaction(1)
            .with_events(vec![event(1)])
            .finish_transaction()
            .start_transaction(1)
            .with_events(vec![event(1)])
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(EventBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let bucket_0_key = sender_row_key(1, 0);
        let bucket_0 = row(&values, &bucket_0_key);
        assert_eq!(bucket_0.bucket_id, 0);
        assert_eq!(bucket_0.bitmap.len(), 1);
        assert!(
            bucket_0
                .bitmap
                .contains(event_bitmap_index::event_seq_lo(txs_per_bucket - 1) as u32)
        );

        let bucket_1_key = sender_row_key(1, 1);
        let bucket_1 = row(&values, &bucket_1_key);
        assert_eq!(bucket_1.bucket_id, 1);
        assert_eq!(bucket_1.bitmap.len(), 1);
        assert!(bucket_1.bitmap.contains(0));
    }
}

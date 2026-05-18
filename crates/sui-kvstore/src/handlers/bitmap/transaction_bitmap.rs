// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction-keyed Roaring bitmap inverted index processor.
//!
//! Emits one bit per `(dimension, tx_seq)` pair. Bits within a bucket row
//! correspond to `tx_sequence_number`s; see [`crate::tables::transaction_bitmap_index`].

use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::for_each_transaction_dimension;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::transaction_bitmap_index;

use crate::handlers::bitmap::BitmapIndexProcessor;

// Compile-time check that BUCKET_SIZE fits in u32 (required for RoaringBitmap bit positions).
const _: () = assert!(transaction_bitmap_index::BUCKET_SIZE <= u32::MAX as u64);

pub struct TransactionBitmapProcessor;

impl BitmapIndexProcessor for TransactionBitmapProcessor {
    const NAME: &'static str = "kvstore_transaction_dimensions";
    const TABLE: &'static str = "transaction_bitmap_index";
    const COLUMN: &'static str = transaction_bitmap_index::col::BITMAP;
    const SCHEMA_VERSION: u32 = transaction_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = transaction_bitmap_index::BUCKET_ID_WIDTH;

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
            let bucket_id = tx_seq / transaction_bitmap_index::BUCKET_SIZE;
            let bit_position = (tx_seq % transaction_bitmap_index::BUCKET_SIZE) as u32;

            for_each_transaction_dimension(tx, &checkpoint.object_set, |dim, value| {
                emit(bucket_id, bit_position, dim, value);
            });
        }
    }

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        watermark.tx_hi >= (bucket_id + 1) * transaction_bitmap_index::BUCKET_SIZE
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use move_core_types::ident_str;
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_inverted_index::encode_dimension_key;
    use sui_inverted_index::move_call_value;
    use sui_types::base_types::ObjectID;
    use sui_types::event::Event;
    use sui_types::gas_coin::GAS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::handlers::bitmap::BitmapIndexHandler;
    use crate::handlers::bitmap::BitmapIndexValue;

    fn sender_row_key(sender_idx: u8, bucket_id: u64) -> Vec<u8> {
        let sender = TestCheckpointBuilder::derive_address(sender_idx);
        let dimension_key = encode_dimension_key(IndexDimension::Sender, sender.as_ref());
        transaction_bitmap_index::encode_row_key(
            transaction_bitmap_index::SCHEMA_VERSION,
            &dimension_key,
            bucket_id,
        )
    }

    fn move_call_row_key(
        package: ObjectID,
        module: &str,
        function: &str,
        bucket_id: u64,
    ) -> Vec<u8> {
        let value = move_call_value(package.as_ref(), Some(module), Some(function));
        let dimension_key = encode_dimension_key(IndexDimension::MoveCall, &value);
        transaction_bitmap_index::encode_row_key(
            transaction_bitmap_index::SCHEMA_VERSION,
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
    async fn deduplicates_repeated_tx_dimensions_within_a_transaction() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .create_coin_object(10, 2, 10, GAS::type_tag())
            .create_coin_object(11, 2, 20, GAS::type_tag())
            .add_move_call(ObjectID::ZERO, "dup", "call")
            .add_move_call(ObjectID::ZERO, "dup", "call")
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

        let values = BitmapIndexHandler::new(TransactionBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = move_call_row_key(ObjectID::ZERO, "dup", "call", 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bucket_id, 0);
        assert_eq!(value.bitmap.len(), 1);
        assert!(value.bitmap.contains(0));
    }

    #[tokio::test]
    async fn same_dimension_across_transactions_sets_multiple_bits() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .with_network_total_transactions(100)
            .start_transaction(1)
            .finish_transaction()
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(TransactionBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = sender_row_key(1, 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bucket_id, 0);
        assert_eq!(value.bitmap.len(), 2);
        assert!(value.bitmap.contains(100));
        assert!(value.bitmap.contains(101));
    }

    #[tokio::test]
    async fn transactions_crossing_bucket_boundary_emit_distinct_bucket_rows() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .with_network_total_transactions(transaction_bitmap_index::BUCKET_SIZE - 1)
            .start_transaction(1)
            .finish_transaction()
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(TransactionBitmapProcessor)
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
                .contains((transaction_bitmap_index::BUCKET_SIZE - 1) as u32)
        );

        let bucket_1_key = sender_row_key(1, 1);
        let bucket_1 = row(&values, &bucket_1_key);
        assert_eq!(bucket_1.bucket_id, 1);
        assert_eq!(bucket_1.bitmap.len(), 1);
        assert!(bucket_1.bitmap.contains(0));
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint-keyed Roaring bitmap inverted index processor.
//!
//! Parallel to [`super::transaction_bitmap`] and [`super::event_bitmap`], but
//! bit positions correspond to `checkpoint_sequence_number`s within the
//! bucket. Emits the union of every dimension that any tx or event in the
//! checkpoint contributes — a single set bit per dimension at the
//! checkpoint's seq, regardless of how many txs/events emit that dimension.
//! Lets queries that operate at checkpoint granularity match without
//! per-tx expansion.

use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::for_each_transaction_dimension;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::checkpoint_bitmap_index;

use crate::handlers::bitmap::BitmapIndexProcessor;

// Compile-time check that BUCKET_SIZE fits in u32 (required for RoaringBitmap bit positions).
const _: () = assert!(checkpoint_bitmap_index::BUCKET_SIZE <= u32::MAX as u64);

pub struct CheckpointBitmapProcessor;

impl BitmapIndexProcessor for CheckpointBitmapProcessor {
    const NAME: &'static str = "kvstore_checkpoint_dimensions";
    const TABLE: &'static str = "checkpoint_bitmap_index";
    const COLUMN: &'static str = checkpoint_bitmap_index::col::BITMAP;
    const SCHEMA_VERSION: u32 = checkpoint_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = checkpoint_bitmap_index::BUCKET_ID_WIDTH;

    fn for_each_indexed_bit<F>(&self, checkpoint: &Checkpoint, mut emit: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]),
    {
        let cp_seq = checkpoint.summary.data().sequence_number;
        let bucket_id = cp_seq / checkpoint_bitmap_index::BUCKET_SIZE;
        let bit_position = (cp_seq % checkpoint_bitmap_index::BUCKET_SIZE) as u32;

        for tx in &checkpoint.transactions {
            for_each_transaction_dimension(tx, &checkpoint.object_set, |dim, value| {
                emit(bucket_id, bit_position, dim, value);
            });
        }
    }

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        // Bucket b covers cp seqs [b*BUCKET_SIZE, (b+1)*BUCKET_SIZE). It is
        // sealed once cp_hi_inclusive has reached the last cp in the bucket;
        // any cp > (b+1)*BUCKET_SIZE - 1 lives in a later bucket and cannot
        // contribute new bits.
        watermark.checkpoint_hi_inclusive + 1
            >= (bucket_id + 1) * checkpoint_bitmap_index::BUCKET_SIZE
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_inverted_index::encode_dimension_key;
    use sui_inverted_index::move_call_value;
    use sui_types::base_types::ObjectID;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::handlers::bitmap::BitmapIndexHandler;
    use crate::handlers::bitmap::BitmapIndexValue;

    fn sender_row_key(sender_idx: u8, bucket_id: u64) -> Vec<u8> {
        let sender = TestCheckpointBuilder::derive_address(sender_idx);
        let dimension_key = encode_dimension_key(IndexDimension::Sender, sender.as_ref());
        checkpoint_bitmap_index::encode_row_key(
            checkpoint_bitmap_index::SCHEMA_VERSION,
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
        checkpoint_bitmap_index::encode_row_key(
            checkpoint_bitmap_index::SCHEMA_VERSION,
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
    async fn checkpoint_bit_position_is_checkpoint_seq() {
        let checkpoint = TestCheckpointBuilder::new(123)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(CheckpointBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = sender_row_key(1, 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bucket_id, 0);
        assert_eq!(value.bitmap.len(), 1);
        assert!(value.bitmap.contains(123));
    }

    #[tokio::test]
    async fn deduplicates_dimensions_across_transactions_in_same_checkpoint() {
        let checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "dup", "call")
            .finish_transaction()
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "dup", "call")
            .finish_transaction()
            .build_checkpoint();

        let values = BitmapIndexHandler::new(CheckpointBitmapProcessor)
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();

        let row_key = move_call_row_key(ObjectID::ZERO, "dup", "call", 0);
        let value = row(&values, &row_key);
        assert_eq!(value.bitmap.len(), 1);
        assert!(value.bitmap.contains(0));

        let sender_key = sender_row_key(1, 0);
        let sender_value = row(&values, &sender_key);
        assert_eq!(sender_value.bitmap.len(), 1);
    }

    #[tokio::test]
    async fn checkpoints_crossing_bucket_boundary_emit_distinct_buckets() {
        let last_in_bucket_0 = checkpoint_bitmap_index::BUCKET_SIZE - 1;

        let cp_a = TestCheckpointBuilder::new(last_in_bucket_0)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();
        let cp_b = TestCheckpointBuilder::new(last_in_bucket_0 + 1)
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();

        let handler = BitmapIndexHandler::new(CheckpointBitmapProcessor);
        let values_a = handler.process(&Arc::new(cp_a)).await.unwrap();
        let values_b = handler.process(&Arc::new(cp_b)).await.unwrap();

        let bucket_0 = row(&values_a, &sender_row_key(1, 0));
        assert_eq!(bucket_0.bucket_id, 0);
        assert!(bucket_0.bitmap.contains(last_in_bucket_0 as u32));

        let bucket_1 = row(&values_b, &sender_row_key(1, 1));
        assert_eq!(bucket_1.bucket_id, 1);
        assert!(bucket_1.bitmap.contains(0));
    }
}

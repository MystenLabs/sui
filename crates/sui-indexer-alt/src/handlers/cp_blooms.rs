// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_framework::types::full_checkpoint_content::ExecutedTransaction;
use sui_indexer_alt_framework::types::transaction::TransactionDataAPI;
use sui_indexer_alt_schema::blooms::should_skip_for_bloom;
use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
use sui_indexer_alt_schema::cp_blooms::MAX_FOLD_DENSITY;
use sui_indexer_alt_schema::cp_blooms::MIN_FOLD_BYTES;
use sui_indexer_alt_schema::cp_blooms::StoredCpBlooms;
use sui_indexer_alt_schema::schema::cp_blooms;

use crate::handlers::affected_addresses;

/// Indexes bloom filters per checkpoint for transaction scanning.
pub(crate) struct CpBlooms;

#[async_trait]
impl Processor for CpBlooms {
    const NAME: &'static str = "cp_blooms";

    type Value = StoredCpBlooms;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;

        let mut bloom = CpBloomFilter::new();
        for tx in &checkpoint.transactions {
            insert_tx_addresses(tx, &mut bloom);
        }

        if bloom.popcount() == 0 {
            return Ok(vec![]);
        }

        Ok(vec![StoredCpBlooms {
            cp_sequence_number: cp_num as i64,
            bloom_filter: bloom.fold(MIN_FOLD_BYTES, MAX_FOLD_DENSITY),
        }])
    }
}

#[async_trait]
impl Handler for CpBlooms {
    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        if values.is_empty() {
            return Ok(0);
        }

        // Each row stores a single checkpoint's bloom filter. On conflict (reprocessing),
        // the filter would be identical, so we can safely ignore duplicates.
        let inserted = diesel::insert_into(cp_blooms::table)
            .values(values)
            .on_conflict(cp_blooms::cp_sequence_number)
            .do_nothing()
            .execute(conn)
            .await?;

        Ok(inserted)
    }
}

/// Inserts values from a transaction into bloom filter.
///
/// Values include:
/// - Transaction sender
/// - Recipient addresses of changed objects
/// - Object IDs of all changed objects
/// - Package IDs from Move calls
/// - Addresses from emitted events (package, type address, type params)
///
/// Common addresses (e.g., 0x0, clock) are filtered out as they appear in most
/// checkpoints and would defeat the bloom filter's purpose.
pub(crate) fn insert_tx_addresses(tx: &ExecutedTransaction, bloom: &mut impl Extend<Vec<u8>>) {
    let sender = std::iter::once(tx.transaction.sender().to_vec());

    let recipients = affected_addresses(&tx.effects).map(|a| a.to_vec());

    let object_ids = tx
        .effects
        .object_changes()
        .into_iter()
        .map(|change| change.id.to_vec());

    let package_ids = tx
        .transaction
        .move_calls()
        .into_iter()
        .map(|(_, package_id, _, _)| package_id.to_vec());

    let event_addresses = tx
        .events
        .iter()
        .flat_map(|evs| evs.data.iter())
        .flat_map(|ev| {
            std::iter::once(ev.type_.address.to_vec())
                .chain(std::iter::once(ev.package_id.to_vec()))
                .chain(
                    ev.type_
                        .type_params
                        .iter()
                        .flat_map(|tp| tp.all_addresses())
                        .map(|addr| addr.to_vec()),
                )
        });

    bloom.extend(
        sender
            .chain(recipients)
            .chain(object_ids)
            .chain(package_ids)
            .chain(event_addresses)
            .filter(|bytes| !should_skip_for_bloom(bytes)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use diesel::QueryDsl;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::cp_blooms::CpBloomFilter;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::transaction::TransactionDataAPI;

    use crate::MIGRATIONS;

    async fn get_all_bloom_filters(conn: &mut Connection<'_>) -> Vec<StoredCpBlooms> {
        cp_blooms::table
            .order_by(cp_blooms::cp_sequence_number)
            .load(conn)
            .await
            .unwrap()
    }

    /// Check if a key might be in a folded bloom filter.
    fn folded_bloom_contains(folded_bytes: &[u8], key: &[u8]) -> bool {
        let folded_bits = folded_bytes.len() * 8;
        CpBloomFilter::hash(key).all(|bit_idx| {
            let folded_idx = bit_idx % folded_bits;
            folded_bytes[folded_idx / 8] & (1 << (folded_idx % 8)) != 0
        })
    }

    #[tokio::test]
    async fn test_cp_blooms_empty_checkpoint() {
        let mut builder = TestCheckpointBuilder::new(0);
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();

        assert!(
            values.is_empty(),
            "Should produce no bloom filter for empty checkpoint"
        );
    }

    #[tokio::test]
    async fn test_cp_blooms_with_function_calls() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder
            .start_transaction(1) // Use sender_idx=1 to avoid SuiAddress::ZERO which is filtered out
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();

        assert_eq!(values.len(), 1, "Should produce one bloom filter");
        assert_eq!(values[0].cp_sequence_number, 0);
        assert!(!values[0].bloom_filter.is_empty());

        CpBlooms::commit(&values, &mut conn).await.unwrap();

        let stored = get_all_bloom_filters(&mut conn).await;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].cp_sequence_number, 0);
    }

    #[tokio::test]
    async fn test_cp_blooms_multiple_checkpoints() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        for cp_num in 0..3 {
            let mut builder = TestCheckpointBuilder::new(cp_num);
            builder = builder
                .start_transaction(1) // Use sender_idx=1 to avoid SuiAddress::ZERO which is filtered out
                .add_move_call(ObjectID::ZERO, "module", "function")
                .finish_transaction();
            let checkpoint = Arc::new(builder.build_checkpoint());

            let values = CpBlooms.process(&checkpoint).await.unwrap();
            CpBlooms::commit(&values, &mut conn).await.unwrap();
        }

        let stored = get_all_bloom_filters(&mut conn).await;
        assert_eq!(stored.len(), 3);

        for (i, bloom) in stored.iter().enumerate() {
            assert_eq!(bloom.cp_sequence_number, i as i64);
        }
    }

    #[tokio::test]
    async fn test_cp_blooms_filter_accuracy() {
        let package_id = ObjectID::from_single_byte(0x42);
        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder
            .start_transaction(1)
            .add_move_call(package_id, "module", "function")
            .finish_transaction()
            .start_transaction(2)
            .add_move_call(package_id, "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();
        assert_eq!(values.len(), 1);

        let bloom_bytes = &values[0].bloom_filter;

        assert!(
            folded_bloom_contains(bloom_bytes, &package_id.to_vec()),
            "Should contain package ID from move call"
        );

        // Common addresses should be filtered out
        assert!(
            !folded_bloom_contains(bloom_bytes, &ObjectID::ZERO.to_vec()),
            "Should NOT contain common address ObjectID::ZERO"
        );

        let sender_0 = checkpoint.transactions[0].transaction.sender();
        let sender_1 = checkpoint.transactions[1].transaction.sender();
        assert!(
            folded_bloom_contains(bloom_bytes, &sender_0.to_vec()),
            "Should contain sender address from tx 0"
        );
        assert!(
            folded_bloom_contains(bloom_bytes, &sender_1.to_vec()),
            "Should contain sender address from tx 1"
        );

        for tx in &checkpoint.transactions {
            for ((obj_id, _, _), _, _) in tx.effects.all_changed_objects() {
                assert!(
                    folded_bloom_contains(bloom_bytes, &obj_id.to_vec()),
                    "Should contain object ID {}",
                    obj_id
                );
            }
        }

        let random_addr = SuiAddress::random_for_testing_only();
        assert!(
            !folded_bloom_contains(bloom_bytes, &random_addr.to_vec()),
            "Should not contain random address"
        );
    }

    /// Test that committing the same checkpoint twice uses do_nothing behavior.
    /// This verifies that reprocessing a checkpoint keeps the original data
    /// since the filter would be identical when reprocessing.
    #[tokio::test]
    async fn test_cp_blooms_merge_on_conflict() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // First commit: checkpoint 0 with package call
        let package1 = ObjectID::from_single_byte(0x42);
        let mut builder1 = TestCheckpointBuilder::new(0);
        builder1 = builder1
            .start_transaction(1)
            .add_move_call(package1, "module", "function")
            .finish_transaction();
        let checkpoint1 = Arc::new(builder1.build_checkpoint());
        let values1 = CpBlooms.process(&checkpoint1).await.unwrap();
        let sender1 = checkpoint1.transactions[0].transaction.sender();

        CpBlooms::commit(&values1, &mut conn).await.unwrap();

        // Verify first key is present
        let stored1 = get_all_bloom_filters(&mut conn).await;
        assert_eq!(stored1.len(), 1);
        assert!(
            folded_bloom_contains(&stored1[0].bloom_filter, &package1.to_vec()),
            "First commit should contain package1"
        );
        assert!(
            folded_bloom_contains(&stored1[0].bloom_filter, &sender1.to_vec()),
            "First commit should contain sender1"
        );

        // Second commit: same checkpoint 0 (simulating reprocessing)
        // Since it's the same checkpoint, the bloom filter would be identical
        // and do_nothing keeps the original row
        CpBlooms::commit(&values1, &mut conn).await.unwrap();

        // Verify data is unchanged (do_nothing preserves original)
        let stored2 = get_all_bloom_filters(&mut conn).await;
        assert_eq!(
            stored2.len(),
            1,
            "Should still have only one row for checkpoint 0"
        );

        // Keys from first commit should still be present
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &package1.to_vec()),
            "package1 should still be present after do_nothing"
        );
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &sender1.to_vec()),
            "sender1 should still be present after do_nothing"
        );
    }
}

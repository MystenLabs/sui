// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::define_sql_function;
use diesel::sql_types::Binary;
use diesel::upsert::excluded;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::SUI_CLOCK_ADDRESS;
use sui_indexer_alt_framework::types::SUI_SYSTEM_ADDRESS;
use sui_indexer_alt_framework::types::base_types::SuiAddress;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_framework::types::full_checkpoint_content::ExecutedTransaction;
use sui_indexer_alt_framework::types::object::Owner;
use sui_indexer_alt_framework::types::transaction::TransactionDataAPI;
use sui_indexer_alt_schema::blooms::bloom::BloomFilter;
use sui_indexer_alt_schema::cp_blooms::BLOOM_FILTER_SEED;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_BYTES;
use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_HASHES;
use sui_indexer_alt_schema::cp_blooms::MAX_FOLD_DENSITY;
use sui_indexer_alt_schema::cp_blooms::MIN_FOLD_BYTES;
use sui_indexer_alt_schema::cp_blooms::StoredCpBlooms;
use sui_indexer_alt_schema::schema::cp_blooms;

// Define the bytea_or SQL function for merging bloom filters
define_sql_function! {
    /// Performs bitwise OR on two bytea values. Used for merging bloom filters.
    fn bytea_or(a: Binary, b: Binary) -> Binary;
}

/// Indexes bloom filters per checkpoint for transaction scanning.
pub(crate) struct CpBlooms;

#[async_trait]
impl Processor for CpBlooms {
    const NAME: &'static str = "cp_blooms";

    type Value = StoredCpBlooms;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;

        let mut bloom =
            BloomFilter::new(CP_BLOOM_NUM_BYTES, CP_BLOOM_NUM_HASHES, BLOOM_FILTER_SEED);
        for tx in checkpoint.transactions.iter() {
            insert_tx_values(tx, &mut bloom);
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

        // Upsert with bytea_or to merge bloom filters on conflict.
        // This ensures bits are accumulated, never lost - if a checkpoint is reprocessed,
        // the new bits are OR'd with existing bits rather than replacing them.
        let inserted = diesel::insert_into(cp_blooms::table)
            .values(values)
            .on_conflict(cp_blooms::cp_sequence_number)
            .do_update()
            .set(cp_blooms::bloom_filter.eq(bytea_or(
                cp_blooms::bloom_filter,
                excluded(cp_blooms::bloom_filter),
            )))
            .execute(conn)
            .await?;

        Ok(inserted)
    }
}

/// Inserts values from a transaction into bloom filter.
///
/// Values include:
/// - Transaction sender (excluding system addresses)
/// - Recipient addresses of changed objects
/// - Object IDs of all changed objects (excluding clock)
/// - Package IDs from Move calls
/// - Addresses from emitted events (package, type address, type params)
pub(crate) fn insert_tx_values(tx: &ExecutedTransaction, bloom: &mut impl Extend<Vec<u8>>) {
    if tx.transaction.sender() != SUI_SYSTEM_ADDRESS.into()
        && tx.transaction.sender() != SuiAddress::ZERO
    {
        bloom.extend([tx.transaction.sender().to_vec()]);
    }

    bloom.extend(tx.effects.all_changed_objects().into_iter().filter_map(
        |(_, owner, _)| match owner {
            Owner::AddressOwner(address) => Some(address.to_vec()),
            _ => None,
        },
    ));

    bloom.extend(
        tx.effects
            .object_changes()
            .into_iter()
            .filter(|change| change.id != SUI_CLOCK_ADDRESS.into())
            .map(|change| change.id.to_vec()),
    );

    bloom.extend(
        tx.transaction
            .move_calls()
            .into_iter()
            .map(|(_, package_id, _, _)| package_id.to_vec()),
    );

    for ev in tx.events.iter().flat_map(|evs| evs.data.iter()) {
        bloom.extend([ev.type_.address.to_vec(), ev.package_id.to_vec()]);
        bloom.extend(
            ev.type_
                .type_params
                .iter()
                .flat_map(|tp| tp.all_addresses())
                .map(|addr| addr.to_vec()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use diesel::QueryDsl;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::blooms::hash;
    use sui_indexer_alt_schema::cp_blooms::CP_BLOOM_NUM_BITS;
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
        let mut hasher = hash::DoubleHasher::with_value(key, BLOOM_FILTER_SEED);
        (0..CP_BLOOM_NUM_HASHES).all(|_| {
            let pos = (hasher.next_hash() as usize) % CP_BLOOM_NUM_BITS;
            let folded_pos = pos % folded_bits;
            folded_bytes[folded_pos / 8] & (1 << (folded_pos % 8)) != 0
        })
    }

    #[tokio::test]
    async fn test_cp_blooms_empty_checkpoint() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let _conn = indexer.store().connect().await.unwrap();

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
    async fn test_cp_blooms_with_affected_addresses() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let _conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder
            .start_transaction(1) // Use sender_idx=1 to avoid SuiAddress::ZERO which is filtered out
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();

        assert_eq!(values.len(), 1);
        assert!(!values[0].bloom_filter.is_empty());
    }

    #[tokio::test]
    async fn test_cp_blooms_with_affected_objects() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let _conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder
            .start_transaction(1) // Use sender_idx=1 to avoid SuiAddress::ZERO which is filtered out
            .create_shared_object(0)
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();

        assert_eq!(values.len(), 1);
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
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let _conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        // Use sender_idx=1 and 2 to avoid SuiAddress::ZERO which is filtered out
        builder = builder
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction()
            .start_transaction(2)
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();
        assert_eq!(values.len(), 1);

        let bloom_bytes = &values[0].bloom_filter;

        assert!(
            folded_bloom_contains(bloom_bytes, &ObjectID::ZERO.to_vec()),
            "Should contain package ID from move call"
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

    #[tokio::test]
    async fn test_cp_blooms_mixed_transactions() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let _conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .finish_transaction()
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();
        assert_eq!(values.len(), 1);

        let bloom_bytes = &values[0].bloom_filter;

        let sender_0 = checkpoint.transactions[0].transaction.sender();
        let sender_1 = checkpoint.transactions[1].transaction.sender();
        assert!(
            folded_bloom_contains(bloom_bytes, &sender_0.to_vec()),
            "Should contain sender from tx 0"
        );
        assert!(
            folded_bloom_contains(bloom_bytes, &sender_1.to_vec()),
            "Should contain sender from tx 1"
        );

        // Verify bloom filter contains package ID from move call in tx 1
        assert!(
            folded_bloom_contains(bloom_bytes, &ObjectID::ZERO.to_vec()),
            "Should contain package ID from move call in tx 1"
        );
    }

    /// Test that committing the same checkpoint twice merges bits (bytea_or behavior).
    /// This verifies that reprocessing a checkpoint accumulates bits rather than
    /// replacing them, preventing data loss from partial/incomplete processing.
    #[tokio::test]
    async fn test_cp_blooms_merge_on_conflict() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // First commit: checkpoint 0 with package call to ObjectID::ZERO
        let mut builder1 = TestCheckpointBuilder::new(0);
        builder1 = builder1
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction();
        let checkpoint1 = Arc::new(builder1.build_checkpoint());
        let values1 = CpBlooms.process(&checkpoint1).await.unwrap();
        let sender1 = checkpoint1.transactions[0].transaction.sender();

        CpBlooms::commit(&values1, &mut conn).await.unwrap();

        // Verify first key is present
        let stored1 = get_all_bloom_filters(&mut conn).await;
        assert_eq!(stored1.len(), 1);
        assert!(
            folded_bloom_contains(&stored1[0].bloom_filter, &ObjectID::ZERO.to_vec()),
            "First commit should contain ObjectID::ZERO"
        );
        assert!(
            folded_bloom_contains(&stored1[0].bloom_filter, &sender1.to_vec()),
            "First commit should contain sender1"
        );

        // Second commit: same checkpoint 0 but with different package (ObjectID::from_single_byte(1))
        // In real scenarios this might happen if indexer restarts or retries
        let package2 = ObjectID::from_single_byte(1);
        let mut builder2 = TestCheckpointBuilder::new(0);
        builder2 = builder2
            .start_transaction(2) // Different sender
            .add_move_call(package2, "other_module", "other_function")
            .finish_transaction();
        let checkpoint2 = Arc::new(builder2.build_checkpoint());
        let values2 = CpBlooms.process(&checkpoint2).await.unwrap();
        let sender2 = checkpoint2.transactions[0].transaction.sender();

        CpBlooms::commit(&values2, &mut conn).await.unwrap();

        // Verify BOTH sets of keys are present after merge
        let stored2 = get_all_bloom_filters(&mut conn).await;
        assert_eq!(
            stored2.len(),
            1,
            "Should still have only one row for checkpoint 0"
        );

        // Keys from first commit should survive
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &ObjectID::ZERO.to_vec()),
            "ObjectID::ZERO from first commit should survive bytea_or merge"
        );
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &sender1.to_vec()),
            "sender1 from first commit should survive bytea_or merge"
        );

        // Keys from second commit should also be present
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &package2.to_vec()),
            "package2 from second commit should be present after merge"
        );
        assert!(
            folded_bloom_contains(&stored2[0].bloom_filter, &sender2.to_vec()),
            "sender2 from second commit should be present after merge"
        );
    }
}

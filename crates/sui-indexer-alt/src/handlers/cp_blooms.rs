// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    types::{
        SUI_CLOCK_ADDRESS, SUI_SYSTEM_ADDRESS,
        base_types::SuiAddress,
        effects::TransactionEffectsAPI,
        full_checkpoint_content::{Checkpoint, ExecutedTransaction},
        object::Owner,
        transaction::TransactionDataAPI,
    },
};
use sui_indexer_alt_schema::{
    blooms::BloomFilter,
    cp_blooms::{BLOOM_FILTER_SEED, CP_BLOOM_NUM_BITS, CP_BLOOM_NUM_HASHES, StoredCpBlooms},
    schema::cp_blooms,
};

/// Indexes bloom filters per checkpoint for transaction scanning.
pub(crate) struct CpBlooms;

#[async_trait]
impl Processor for CpBlooms {
    const NAME: &'static str = "cp_blooms";

    type Value = StoredCpBlooms;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_num = checkpoint.summary.sequence_number;

        let mut items = HashSet::new();
        for tx in checkpoint.transactions.iter() {
            items.extend(extract_filter_keys(tx));
        }

        if items.is_empty() {
            return Ok(vec![]);
        }

        let mut bloom = BloomFilter::new(CP_BLOOM_NUM_BITS, CP_BLOOM_NUM_HASHES, BLOOM_FILTER_SEED);
        for item in &items {
            bloom.insert(item);
        }

        Ok(vec![StoredCpBlooms {
            cp_sequence_number: cp_num as i64,
            bloom_filter: bloom.fold(),
            num_items: Some(items.len() as i64),
        }])
    }
}

#[async_trait]
impl Handler for CpBlooms {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 500;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        if values.is_empty() {
            return Ok(0);
        }

        // Single batched insert
        let inserted = diesel::insert_into(cp_blooms::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?;

        Ok(inserted)
    }
}

/// Extract keys from a transaction for bloom filter indexing.
///
/// Keys include:
/// - Transaction sender (excluding system addresses)
/// - Recipient addresses of changed objects
/// - Object IDs of all changed objects (excluding clock)
/// - Package IDs from Move calls
/// - Addresses from emitted events (package, type address, type params)
pub(crate) fn extract_filter_keys(tx: &ExecutedTransaction) -> HashSet<Vec<u8>> {
    let mut keys = HashSet::new();

    if tx.transaction.sender() != SUI_SYSTEM_ADDRESS.into()
        && tx.transaction.sender() != SuiAddress::ZERO
    {
        keys.insert(tx.transaction.sender().to_vec());
    }

    for ((_obj_id, _version, _digest), owner, _write_kind) in tx.effects.all_changed_objects() {
        if let Owner::AddressOwner(address) = owner {
            keys.insert(address.to_vec());
        }
    }

    for object_change in tx.effects.object_changes() {
        if object_change.id != SUI_CLOCK_ADDRESS.into() {
            keys.insert(object_change.id.to_vec());
        }
    }

    for (_, package_id, _, _) in tx.transaction.move_calls() {
        keys.insert(package_id.to_vec());
    }

    for ev in tx.events.iter().flat_map(|evs| evs.data.iter()) {
        keys.insert(ev.type_.address.to_vec());
        keys.insert(ev.package_id.to_vec());
        for type_param in ev.type_.type_params.as_slice() {
            for addr in type_param.all_addresses() {
                keys.insert(addr.to_vec());
            }
        }
    }

    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MIGRATIONS;
    use diesel::QueryDsl;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::blooms::hash;
    use sui_types::{
        base_types::{ObjectID, SuiAddress},
        test_checkpoint_data_builder::TestCheckpointBuilder,
        transaction::TransactionDataAPI,
    };

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
        let positions = hash::compute_positions(
            key,
            CP_BLOOM_NUM_BITS,
            CP_BLOOM_NUM_HASHES,
            BLOOM_FILTER_SEED,
        );
        positions.iter().all(|&pos| {
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

        // Verify bloom filter contains package ID from the move call
        assert!(
            folded_bloom_contains(bloom_bytes, &ObjectID::ZERO.to_vec()),
            "Should contain package ID from move call"
        );

        // Verify bloom filter contains sender addresses
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

        // Verify bloom filter contains affected object IDs
        for tx in &checkpoint.transactions {
            for ((obj_id, _, _), _, _) in tx.effects.all_changed_objects() {
                assert!(
                    folded_bloom_contains(bloom_bytes, &obj_id.to_vec()),
                    "Should contain object ID {}",
                    obj_id
                );
            }
        }

        // Verify bloom filter does NOT contain random values
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
            // Empty transaction with no operations - still affects gas object
            .finish_transaction()
            .start_transaction(1)
            .add_move_call(ObjectID::ZERO, "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let values = CpBlooms.process(&checkpoint).await.unwrap();
        assert_eq!(values.len(), 1);

        let bloom_bytes = &values[0].bloom_filter;

        // Verify bloom filter contains sender addresses from both transactions
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
}

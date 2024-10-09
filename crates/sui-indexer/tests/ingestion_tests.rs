// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;

use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use simulacrum::Simulacrum;
use sui_indexer::errors::IndexerError;
use sui_indexer::models::{
    objects::StoredObject, objects::StoredObjectSnapshot, transactions::StoredTransaction,
};
use sui_indexer::schema::{objects, objects_snapshot, transactions};
use sui_indexer::store::indexer_store::IndexerStore;
use sui_indexer::test_utils::{set_up, wait_for_checkpoint, wait_for_objects_snapshot};
use sui_indexer::types::EventIndex;
use sui_indexer::types::TxIndex;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GasCoin;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use tempfile::tempdir;

#[tokio::test]
pub async fn test_transaction_table() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Execute a simple transaction.
    let transfer_recipient = SuiAddress::random_for_testing_only();
    let (transaction, _) = sim.transfer_txn(transfer_recipient);
    let (effects, err) = sim.execute_transaction(transaction.clone()).unwrap();
    assert!(err.is_none());

    // Create a checkpoint which should include the transaction we executed.
    let checkpoint = sim.create_checkpoint();

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    // Wait for the indexer to catch up to the checkpoint.
    wait_for_checkpoint(&pg_store, 1).await?;

    let digest = effects.transaction_digest();

    // Read the transaction from the database directly.
    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    let db_txn: StoredTransaction = transactions::table
        .filter(transactions::transaction_digest.eq(digest.inner().to_vec()))
        .first::<StoredTransaction>(&mut connection)
        .await
        .expect("Failed reading transaction from PostgresDB");

    // Check that the transaction was stored correctly.
    assert_eq!(db_txn.tx_sequence_number, 1);
    assert_eq!(db_txn.transaction_digest, digest.inner().to_vec());
    assert_eq!(
        db_txn.raw_transaction,
        bcs::to_bytes(&transaction.data()).unwrap()
    );
    assert_eq!(db_txn.raw_effects, bcs::to_bytes(&effects).unwrap());
    assert_eq!(db_txn.timestamp_ms, checkpoint.timestamp_ms as i64);
    assert_eq!(db_txn.checkpoint_sequence_number, 1);
    assert_eq!(db_txn.transaction_kind, 1);
    assert_eq!(db_txn.success_command_count, 2); // split coin + transfer
    Ok(())
}

#[tokio::test]
pub async fn test_object_type() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Execute a simple transaction.
    let transfer_recipient = SuiAddress::random_for_testing_only();
    let (transaction, _) = sim.transfer_txn(transfer_recipient);
    let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
    assert!(err.is_none());

    // Create a checkpoint which should include the transaction we executed.
    let _ = sim.create_checkpoint();

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    // Wait for the indexer to catch up to the checkpoint.
    wait_for_checkpoint(&pg_store, 1).await?;

    let obj_id = transaction.gas()[0].0;

    // Read the transaction from the database directly.
    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    let db_object: StoredObject = objects::table
        .filter(objects::object_id.eq(obj_id.to_vec()))
        .first::<StoredObject>(&mut connection)
        .await
        .expect("Failed reading object from PostgresDB");

    let obj_type_tag = GasCoin::type_();

    // Check that the different components of the event type were stored correctly.
    assert_eq!(
        db_object.object_type,
        Some(obj_type_tag.to_canonical_string(true))
    );
    assert_eq!(
        db_object.object_type_package,
        Some(SUI_FRAMEWORK_PACKAGE_ID.to_vec())
    );
    assert_eq!(db_object.object_type_module, Some("coin".to_string()));
    assert_eq!(db_object.object_type_name, Some("Coin".to_string()));
    Ok(())
}

#[tokio::test]
pub async fn test_objects_snapshot() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Run 10 transfer transactions and create 10 checkpoints
    let mut last_transaction = None;
    let total_checkpoint_sequence_number = 7usize;
    for _ in 0..total_checkpoint_sequence_number {
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());
        last_transaction = Some(transaction);
        let _ = sim.create_checkpoint();
    }

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    // Wait for objects snapshot at checkpoint max_expected_checkpoint_sequence_number
    let max_expected_checkpoint_sequence_number = total_checkpoint_sequence_number - 5;
    wait_for_objects_snapshot(&pg_store, max_expected_checkpoint_sequence_number as u64).await?;

    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    // Get max checkpoint_sequence_number from objects_snapshot table and assert it's expected
    let max_checkpoint_sequence_number = objects_snapshot::table
        .select(objects_snapshot::checkpoint_sequence_number)
        .order(objects_snapshot::checkpoint_sequence_number.desc())
        .limit(1)
        .first::<i64>(&mut connection)
        .await
        .expect("Failed to read max checkpoint_sequence_number from objects_snapshot");
    assert_eq!(
        max_checkpoint_sequence_number,
        max_expected_checkpoint_sequence_number as i64
    );

    // Get the object state at max_expected_checkpoint_sequence_number and assert.
    let last_tx = last_transaction.unwrap();
    let obj_id = last_tx.gas()[0].0;
    let gas_owner_id = last_tx.sender_address();

    let snapshot_object = objects_snapshot::table
        .filter(objects_snapshot::object_id.eq(obj_id.to_vec()))
        .filter(
            objects_snapshot::checkpoint_sequence_number
                .eq(max_expected_checkpoint_sequence_number as i64),
        )
        .first::<StoredObjectSnapshot>(&mut connection)
        .await
        .expect("Failed reading object from objects_snapshot");
    // Assert that the object state is as expected at checkpoint max_expected_checkpoint_sequence_number
    assert_eq!(snapshot_object.object_id, obj_id.to_vec());
    assert_eq!(
        snapshot_object.checkpoint_sequence_number,
        max_expected_checkpoint_sequence_number as i64
    );
    assert_eq!(snapshot_object.owner_type, Some(1));
    assert_eq!(snapshot_object.owner_id, Some(gas_owner_id.to_vec()));
    Ok(())
}

// test insert large batch of tx_indices
#[tokio::test]
pub async fn test_insert_large_batch_tx_indices() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    let mut v = Vec::new();
    for _ in 0..1000 {
        v.push(TxIndex::random());
    }
    pg_store.persist_tx_indices(v).await?;
    Ok(())
}

// test insert large batch of event_indices
#[tokio::test]
pub async fn test_insert_large_batch_event_indices() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    let mut v = Vec::new();
    for _ in 0..1000 {
        v.push(EventIndex::random());
    }
    pg_store.persist_event_indices(v).await?;
    Ok(())
}

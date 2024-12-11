// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;
use std::time::Duration;

use diesel::dsl::count_star;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use simulacrum::Simulacrum;
use sui_indexer::errors::IndexerError;
use sui_indexer::handlers::TransactionObjectChangesToCommit;
use sui_indexer::models::{
    checkpoints::StoredCheckpoint, objects::StoredObject, objects::StoredObjectSnapshot,
    transactions::StoredTransaction,
};
use sui_indexer::schema::epochs;
use sui_indexer::schema::events;
use sui_indexer::schema::full_objects_history;
use sui_indexer::schema::objects_history;
use sui_indexer::schema::{checkpoints, objects, objects_snapshot, transactions};
use sui_indexer::store::indexer_store::IndexerStore;
use sui_indexer::test_utils::set_up_on_mvr_mode;
use sui_indexer::test_utils::{
    set_up, set_up_with_start_and_end_checkpoints, wait_for_checkpoint, wait_for_objects_snapshot,
};
use sui_indexer::types::EventIndex;
use sui_indexer::types::IndexedDeletedObject;
use sui_indexer::types::IndexedObject;
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
pub async fn test_checkpoint_range_ingestion() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Create multiple checkpoints
    for _ in 0..10 {
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (_, err) = sim.execute_transaction(transaction).unwrap();
        assert!(err.is_none());
        sim.create_checkpoint();
    }

    // Set up indexer with specific start and end checkpoints
    let start_checkpoint = 2;
    let end_checkpoint = 4;
    let (_, pg_store, _, _database) = set_up_with_start_and_end_checkpoints(
        Arc::new(sim),
        data_ingestion_path,
        start_checkpoint,
        end_checkpoint,
    )
    .await;

    // Wait for the indexer to catch up to the end checkpoint
    wait_for_checkpoint(&pg_store, end_checkpoint).await?;

    // Verify that only checkpoints within the specified range were ingested
    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    let checkpoint_count: i64 = checkpoints::table
        .count()
        .get_result(&mut connection)
        .await
        .expect("Failed to count checkpoints");
    assert_eq!(checkpoint_count, 3, "Expected 3 checkpoints to be ingested");

    // Verify the range of ingested checkpoints
    let min_checkpoint = checkpoints::table
        .select(diesel::dsl::min(checkpoints::sequence_number))
        .first::<Option<i64>>(&mut connection)
        .await
        .expect("Failed to get min checkpoint")
        .expect("Min checkpoint should be Some");
    let max_checkpoint = checkpoints::table
        .select(diesel::dsl::max(checkpoints::sequence_number))
        .first::<Option<i64>>(&mut connection)
        .await
        .expect("Failed to get max checkpoint")
        .expect("Max checkpoint should be Some");
    assert_eq!(
        min_checkpoint, start_checkpoint as i64,
        "Minimum ingested checkpoint should be {}",
        start_checkpoint
    );
    assert_eq!(
        max_checkpoint, end_checkpoint as i64,
        "Maximum ingested checkpoint should be {}",
        end_checkpoint
    );

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

#[tokio::test]
pub async fn test_objects_ingestion() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;

    let mut objects = Vec::new();
    for _ in 0..1000 {
        objects.push(TransactionObjectChangesToCommit {
            changed_objects: vec![IndexedObject::random()],
            deleted_objects: vec![IndexedDeletedObject::random()],
        });
    }
    pg_store.persist_objects(objects).await?;
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

#[tokio::test]
pub async fn test_epoch_boundary() -> Result<(), IndexerError> {
    println!("test_epoch_boundary");
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    let transfer_recipient = SuiAddress::random_for_testing_only();
    let (transaction, _) = sim.transfer_txn(transfer_recipient);
    let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
    assert!(err.is_none());

    sim.create_checkpoint(); // checkpoint 1
    sim.advance_epoch(true); // checkpoint 2 and epoch 1

    let (transaction, _) = sim.transfer_txn(transfer_recipient);
    let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
    sim.create_checkpoint(); // checkpoint 3
    assert!(err.is_none());

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;
    wait_for_checkpoint(&pg_store, 3).await?;
    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    let db_checkpoint: StoredCheckpoint = checkpoints::table
        .order(checkpoints::sequence_number.desc())
        .first::<StoredCheckpoint>(&mut connection)
        .await
        .expect("Failed reading checkpoint from PostgresDB");
    assert_eq!(db_checkpoint.sequence_number, 3);
    assert_eq!(db_checkpoint.epoch, 1);
    Ok(())
}

#[tokio::test]
pub async fn test_mvr_mode() -> Result<(), IndexerError> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Create 3 checkpoints and epochs of sequence number 0 through 2 inclusive
    for _ in 0..=2 {
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());

        // creates checkpoint and advances epoch
        sim.advance_epoch(true);
    }

    sim.create_checkpoint(); // advance to checkpoint 4 to stabilize indexer

    let (_, pg_store, _, _database) =
        set_up_on_mvr_mode(Arc::new(sim), data_ingestion_path, true).await;
    wait_for_checkpoint(&pg_store, 4).await?;
    let mut connection = pg_store.pool().dedicated_connection().await.unwrap();
    let db_checkpoint: StoredCheckpoint = checkpoints::table
        .order(checkpoints::sequence_number.desc())
        .first::<StoredCheckpoint>(&mut connection)
        .await
        .expect("Failed reading checkpoint from PostgresDB");
    let db_epoch = epochs::table
        .order(epochs::epoch.desc())
        .select(epochs::epoch)
        .first::<i64>(&mut connection)
        .await
        .expect("Failed reading epoch from PostgresDB");

    assert_eq!(db_checkpoint.sequence_number, 4);
    assert_eq!(db_checkpoint.epoch, db_epoch);

    // Check that other tables have not been written to
    assert_eq!(
        0_i64,
        transactions::table
            .select(count_star())
            .first::<i64>(&mut connection)
            .await
            .expect("Failed to count * transactions")
    );
    assert_eq!(
        0_i64,
        events::table
            .select(count_star())
            .first::<i64>(&mut connection)
            .await
            .expect("Failed to count * transactions")
    );
    assert_eq!(
        0_i64,
        full_objects_history::table
            .select(count_star())
            .first::<i64>(&mut connection)
            .await
            .expect("Failed to count * transactions")
    );

    // Check that objects_history is being correctly pruned. At epoch 3, we should only have data
    // between 2 and 3 inclusive.
    loop {
        let history_objects = objects_history::table
            .select(objects_history::checkpoint_sequence_number)
            .load::<i64>(&mut connection)
            .await?;

        let has_invalid_entries = history_objects.iter().any(|&elem| elem < 2);

        if !has_invalid_entries {
            // No more invalid entries found, exit the loop
            break;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // After the loop, verify all entries are within expected range
    let final_check = objects_history::table
        .select(objects_history::checkpoint_sequence_number)
        .order_by(objects_history::checkpoint_sequence_number.asc())
        .load::<i64>(&mut connection)
        .await?;

    for elem in final_check {
        assert!(elem >= 2);
    }

    Ok(())
}

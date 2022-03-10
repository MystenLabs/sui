// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::*;

use std::env;
use std::fs;

#[tokio::test]
async fn test_open_manager() {
    // let (_, authority_key) = get_key_pair();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Make a test key pair
    let (_, key_pair) = get_key_pair();
    let key_pair = Arc::pin(key_pair);
    let address = *key_pair.public_key_bytes();

    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        // TEST 1: init from an empty database should return to a zero block
        let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
        let last_block = manager
            .init_from_database(address, key_pair.clone())
            .await
            .expect("No error expected.");

        assert_eq!(0, last_block.next_sequence_number);

        // TEST 2: init from a db with a transaction not in the sequence makes a new block
        //         when we re-open the database.

        store
            .executed_sequence
            .insert(&0, &TransactionDigest::new([0; 32]))
            .expect("no error on write");
    }
    // drop all
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
        let last_block = manager
            .init_from_database(address, key_pair.clone())
            .await
            .expect("No error expected.");

        assert_eq!(1, last_block.next_sequence_number);

        // TEST 3: If the database contains out of order transactions we just make a block with gaps
        store
            .executed_sequence
            .insert(&2, &TransactionDigest::new([0; 32]))
            .expect("no error on write");
    }
    // drop all
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
        let last_block = manager
            .init_from_database(address, key_pair.clone())
            .await
            .unwrap();

        assert_eq!(last_block.next_sequence_number, 3);
        assert_eq!(last_block.initial_sequence_number, 2);
        assert_eq!(last_block.size, 1);
    }
}

#[tokio::test]
async fn test_batch_manager_happy_path() {
    // let (_, authority_key) = get_key_pair();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

    // Make a test key pair
    let (_, key_pair) = get_key_pair();
    let key_pair = Arc::pin(key_pair);
    let address = *key_pair.public_key_bytes();

    // TEST 1: init from an empty database should return to a zero block
    let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
    let _join = manager
        .start_service(address, key_pair, 1000, Duration::from_millis(500))
        .await
        .expect("No errors starting manager.");

    // Send a transaction.
    let tx_zero = TransactionDigest::new([0; 32]);
    _send
        .send_item(0, tx_zero)
        .await
        .expect("Send to the channel.");

    // First we get a transaction update
    let (_tx, mut rx) = _pair;
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((0, _))
    ));

    // Then we (eventually) get a batch
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    _send
        .send_item(1, tx_zero)
        .await
        .expect("Send to the channel.");

    // When we close the sending channel we also also end the service task
    drop(_send);
    drop(_tx);

    _join.await.expect("No errors in task");

    // But the block is made, and sent as a notification.
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((1, _))
    ));
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));
    assert!(matches!(rx.recv().await, Err(_)));
}

#[tokio::test]
async fn test_batch_manager_out_of_order() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

    // Make a test key pair
    let (_, key_pair) = get_key_pair();
    let key_pair = Arc::pin(key_pair);
    let address = *key_pair.public_key_bytes();

    // TEST 1: init from an empty database should return to a zero block
    let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
    let _join = manager
        .start_service(address, key_pair, 4, Duration::from_millis(5000))
        .await
        .expect("Start service with no issues.");

    // Send transactions out of order
    let tx_zero = TransactionDigest::new([0; 32]);
    _send
        .send_item(1, tx_zero)
        .await
        .expect("Send to the channel.");

    _send
        .send_item(3, tx_zero)
        .await
        .expect("Send to the channel.");

    _send
        .send_item(2, tx_zero)
        .await
        .expect("Send to the channel.");

    _send
        .send_item(0, tx_zero)
        .await
        .expect("Send to the channel.");

    // Get transactions in order then batch.
    let (_tx, mut rx) = _pair;
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((0, _))
    ));

    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((1, _))
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((2, _))
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((3, _))
    ));

    // Then we (eventually) get a batch
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    // When we close the sending channel we also also end the service task
    drop(_send);
    drop(_tx);

    _join.await.expect("No errors in task");

    assert!(matches!(rx.recv().await, Err(_)));
}

use sui_types::{crypto::get_key_pair, object::Object};

#[tokio::test]
async fn test_handle_move_order_with_batch() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let mut authority_state = init_state_with_objects(vec![gas_payment_object]).await;

    // Create a listening infrastrucure.
    let (_send, manager, _pair) = BatchManager::new(authority_state.db(), 100);
    let _join = manager
        .start_service(
            authority_state.name,
            authority_state.secret.clone(),
            4,
            Duration::from_millis(500),
        )
        .await
        .expect("No issues starting service.");

    // Check we can subscribe
    let mut rx = _pair.0.subscribe();

    authority_state
        .set_batch_sender(_send, _pair.0)
        .expect("No problem registering");
    tokio::task::yield_now().await;

    let effects = create_move_object(
        &authority_state,
        &gas_payment_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    // Second and after is the one
    let y = rx.recv().await.unwrap();
    println!("{:?}", y);
    assert!(matches!(
        y,
        UpdateItem::Transaction((0, x)) if x == effects.transaction_digest
    ));

    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    drop(authority_state);

    _join.await.expect("No issues ending task.");
}

#[tokio::test]
async fn test_batch_store_retrieval() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

    // Make a test key pair
    let (_, key_pair) = get_key_pair();
    let key_pair = Arc::pin(key_pair);
    let address = *key_pair.public_key_bytes();

    // TEST 1: init from an empty database should return to a zero block
    let (_send, manager, _pair) = BatchManager::new(store.clone(), 100);
    let _join = manager
        .start_service(address, key_pair, 10, Duration::from_secs(60))
        .await
        .expect("Start service with no issues.");

    // Send transactions out of order
    let tx_zero = TransactionDigest::new([0; 32]);

    let inner_store = store.clone();
    for i in 0u64..105 {
        inner_store
            .executed_sequence
            .insert(&i, &tx_zero)
            .expect("Failed to write.");

        _send
            .send_item(i, tx_zero)
            .await
            .expect("Send to the channel.");
    }

    // Add a few out of order transactions that should be ignored
    // NOTE: gap between 104 and 110
    for i in 110u64..120 {
        inner_store
            .executed_sequence
            .insert(&i, &tx_zero)
            .expect("Failed to write.");

        _send
            .send_item(i, tx_zero)
            .await
            .expect("Send to the channel.");
    }

    // TEST 1: Get batches across boundaries

    let (batches, transactions) = store
        .batches_and_transactions(12, 34)
        .expect("Retrieval failed!");

    assert_eq!(4, batches.len());
    assert_eq!(10, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(40, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(30, transactions.len());

    // TEST 2: Get with range wihin batch
    let (batches, transactions) = store
        .batches_and_transactions(54, 56)
        .expect("Retrieval failed!");

    assert_eq!(2, batches.len());
    assert_eq!(50, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(60, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(10, transactions.len());

    // TEST 3: Get on boundary
    let (batches, transactions) = store
        .batches_and_transactions(30, 50)
        .expect("Retrieval failed!");

    println!("{:?}", batches);

    assert_eq!(3, batches.len());
    assert_eq!(30, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(50, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(20, transactions.len());

    // TEST 4: Get past the end
    let (batches, transactions) = store
        .batches_and_transactions(94, 120)
        .expect("Retrieval failed!");

    println!("{:?}", batches);

    assert_eq!(2, batches.len());
    assert_eq!(90, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(100, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(15, transactions.len());

    // TEST 5: Both past the end
    let (batches, transactions) = store
        .batches_and_transactions(123, 222)
        .expect("Retrieval failed!");

    println!("{:?}", batches);

    assert_eq!(1, batches.len());
    assert_eq!(100, batches.first().unwrap().batch.next_sequence_number);

    assert_eq!(5, transactions.len());

    // When we close the sending channel we also also end the service task
    drop(_send);
    drop(_pair);

    _join.await.expect("No errors in task");
}

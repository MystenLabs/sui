// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_adapter::genesis;
use sui_types::committee::Committee;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::KeyPair;

use super::*;
use crate::authority::authority_tests::*;
use crate::authority::*;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::sync::Arc;

fn init_state_parameters() -> (Committee, SuiAddress, KeyPair) {
    let (authority_address, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ *authority_key.public_key_bytes(),
        /* voting right */ 1,
    );
    let committee = Committee::new(authorities);

    (committee, authority_address, authority_key)
}

async fn init_state(
    committee: Committee,
    authority_key: KeyPair,
    store: Arc<AuthorityStore>,
) -> AuthorityState {
    AuthorityState::new(
        committee,
        *authority_key.public_key_bytes(),
        Arc::pin(authority_key),
        store,
        genesis::clone_genesis_compiled_modules(),
        &mut genesis::get_genesis_context(),
    )
    .await
}

#[tokio::test]
async fn test_open_manager() {
    // let (_, authority_key) = get_key_pair();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    let (committee_source, _, authority_key_source) = init_state_parameters();
    let (committee, authority_key) = (committee_source.clone(), authority_key_source.copy());
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
        let mut authority_state = init_state(committee, authority_key, store.clone()).await;

        // TEST 1: init from an empty database should return to a zero block
        let last_block = authority_state
            .init_batches_from_database()
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
    let (committee, authority_key) = (committee_source.clone(), authority_key_source.copy());
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
        let mut authority_state = init_state(committee, authority_key, store.clone()).await;

        let last_block = authority_state
            .init_batches_from_database()
            .expect("No error expected.");

        assert_eq!(1, last_block.next_sequence_number);

        // TEST 3: If the database contains out of order transactions we just make a block with gaps
        store
            .executed_sequence
            .insert(&2, &TransactionDigest::new([0; 32]))
            .expect("no error on write");
    }
    // drop all
    let (committee, authority_key) = (committee_source.clone(), authority_key_source.copy());
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
        let mut authority_state = init_state(committee, authority_key, store.clone()).await;

        let last_block = authority_state.init_batches_from_database().unwrap();

        assert_eq!(last_block.next_sequence_number, 3);
        assert_eq!(last_block.initial_sequence_number, 2);
        assert_eq!(last_block.size, 1);
    }
}

#[tokio::test]
async fn test_batch_manager_happy_path() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

    // Make a test key pair
    let (committee, _, authority_key) = init_state_parameters();
    let authority_state = Arc::new(init_state(committee, authority_key, store.clone()).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            .run_batch_service(1000, Duration::from_millis(500))
            .await
    });

    // TEST 1: init from an empty database should return to a zero block

    // Send a transaction.
    {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        store.side_sequence(t0.seq(), &TransactionDigest::random());
    }

    // First we get a transaction update
    let mut rx = authority_state.subscribe_batch();
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((0, _, _))
    ));

    // Then we (eventually) get a batch
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        store.side_sequence(t0.seq(), &TransactionDigest::random());
    }

    // When we close the sending channel we also also end the service task
    authority_state.batch_notifier.close();

    // But the block is made, and sent as a notification.
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((1, _, _))
    ));
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    _join.await.expect("No errors in task").expect("ok");
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
    let (committee, _, authority_key) = init_state_parameters();
    let authority_state = Arc::new(init_state(committee, authority_key, store.clone()).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            .run_batch_service(1000, Duration::from_millis(500))
            .await
    });
    // Send transactions out of order
    let mut rx = authority_state.subscribe_batch();

    {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        let t1 = &authority_state.batch_notifier.ticket().expect("ok");
        let t2 = &authority_state.batch_notifier.ticket().expect("ok");
        let t3 = &authority_state.batch_notifier.ticket().expect("ok");

        store.side_sequence(t1.seq(), &TransactionDigest::random());
        store.side_sequence(t3.seq(), &TransactionDigest::random());
        store.side_sequence(t2.seq(), &TransactionDigest::random());
        store.side_sequence(t0.seq(), &TransactionDigest::random());
    }

    // Get transactions in order then batch.
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((0, _, _))
    ));

    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((1, _, _))
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((2, _, _))
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((3, _, _))
    ));

    // Then we (eventually) get a batch
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    // When we close the sending channel we also also end the service task
    authority_state.batch_notifier.close();

    _join.await.expect("No errors in task").expect("ok");
}

use sui_types::object::Object;

#[tokio::test]
async fn test_handle_move_order_with_batch() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let authority_state = Arc::new(init_state_with_objects(vec![gas_payment_object]).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            .run_batch_service(1000, Duration::from_millis(500))
            .await
    });
    // Send transactions out of order
    let mut rx = authority_state.subscribe_batch();

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
        UpdateItem::Transaction((0, x, _)) if x == effects.transaction_digest
    ));

    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    authority_state.batch_notifier.close();
    _join.await.expect("No issues ending task.").expect("ok");
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
    let (committee, _, authority_key) = init_state_parameters();
    let authority_state = Arc::new(init_state(committee, authority_key, store.clone()).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            .run_batch_service(10, Duration::from_secs(6000))
            .await
    });
    // Send transactions out of order
    let tx_zero = TransactionDigest::new([0; 32]);

    let inner_store = store.clone();
    for _i in 0u64..105 {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        inner_store
            .executed_sequence
            .insert(&t0.seq(), &tx_zero)
            .expect("Failed to write.");
    }

    // Add a few out of order transactions that should be ignored
    // NOTE: gap between 105 and 110
    (105u64..110).into_iter().for_each(|_| {
        let _ = &authority_state.batch_notifier.ticket().expect("ok");
    });

    for _i in 110u64..120 {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        inner_store
            .executed_sequence
            .insert(&t0.seq(), &tx_zero)
            .expect("Failed to write.");
    }

    // Give a change to the channels to send.
    tokio::task::yield_now().await;

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

    assert_eq!(3, batches.len());
    assert_eq!(30, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(50, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(20, transactions.len());

    // TEST 4: Get past the end
    let (batches, transactions) = store
        .batches_and_transactions(94, 120)
        .expect("Retrieval failed!");

    println!("{:?}", batches);
    assert_eq!(3, batches.len());
    assert_eq!(90, batches.first().unwrap().batch.next_sequence_number);
    assert_eq!(115, batches.last().unwrap().batch.next_sequence_number);

    assert_eq!(25, transactions.len());

    // TEST 5: Both past the end
    let (batches, transactions) = store
        .batches_and_transactions(123, 222)
        .expect("Retrieval failed!");

    assert_eq!(1, batches.len());
    assert_eq!(115, batches.first().unwrap().batch.next_sequence_number);

    assert_eq!(5, transactions.len());

    // When we close the sending channel we also also end the service task
    authority_state.batch_notifier.close();
    _join.await.expect("No errors in task").expect("ok");
}

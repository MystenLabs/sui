// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{prelude::StdRng, SeedableRng};
use sui_types::committee::Committee;
use sui_types::crypto::KeyPair;
use sui_types::messages_checkpoint::CheckpointRequest;
use sui_types::messages_checkpoint::CheckpointResponse;

use super::*;
use crate::authority::authority_tests::*;
use crate::authority::*;
use crate::safe_client::SafeClient;

use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use async_trait::async_trait;
use futures::lock::Mutex;
use futures::stream;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::sync::Arc;
use sui_types::messages::{
    AccountInfoRequest, AccountInfoResponse, BatchInfoRequest, BatchInfoResponseItem,
    ConfirmationTransaction, ConsensusTransaction, ObjectInfoRequest, ObjectInfoResponse,
    Transaction, TransactionInfoRequest, TransactionInfoResponse,
};
use sui_types::object::Object;

pub(crate) fn init_state_parameters_from_rng<R>(rng: &mut R) -> (Committee, SuiAddress, KeyPair)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let (authority_address, authority_key) = KeyPair::get_key_pair_from_rng(rng);
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ *authority_key.public_key_bytes(),
        /* voting right */ 1,
    );
    let committee = Committee::new(0, authorities).unwrap();

    (committee, authority_address, authority_key)
}

pub(crate) async fn init_state(
    committee: Committee,
    authority_key: KeyPair,
    store: Arc<AuthorityStore>,
) -> AuthorityState {
    AuthorityState::new(
        committee,
        *authority_key.public_key_bytes(),
        Arc::pin(authority_key),
        store,
        None,
        None,
        None,
        &sui_config::genesis::Genesis::get_default_genesis(),
        &prometheus::Registry::new(),
    )
    .await
}

#[tokio::test]
async fn test_open_manager() {
    // let (_, authority_key) = KeyPair::get_key_pair();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    let seed = [1u8; 32];

    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    {
        // Create an authority
        let store = Arc::new(AuthorityStore::open(&path, None));
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
            .insert(&0, &ExecutionDigests::random())
            .expect("no error on write");
        drop(store);
        drop(authority_state);
    }
    // drop all
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    {
        // Create an authority
        let store = Arc::new(AuthorityStore::open(&path, None));
        let mut authority_state = init_state(committee, authority_key, store.clone()).await;

        let last_block = authority_state
            .init_batches_from_database()
            .expect("No error expected.");

        assert_eq!(1, last_block.next_sequence_number);

        // TEST 3: If the database contains out of order transactions we just make a block with gaps
        store
            .executed_sequence
            .insert(&2, &ExecutionDigests::random())
            .expect("no error on write");
        drop(store);
        drop(authority_state);
    }
    // drop all
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    {
        // Create an authority
        let store = Arc::new(AuthorityStore::open(&path, None));
        let mut authority_state = init_state(committee, authority_key, store.clone()).await;

        let last_block = authority_state.init_batches_from_database().unwrap();

        assert_eq!(last_block.next_sequence_number, 3);
        assert_eq!(last_block.initial_sequence_number, 2);
        assert_eq!(last_block.size, 1);
        drop(store);
        drop(authority_state);
    }
}

#[tokio::test]
async fn test_batch_manager_happy_path() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let store = Arc::new(AuthorityStore::open(&path, None));

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
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
        store.side_sequence(t0.seq(), &ExecutionDigests::random());
    }

    // First we get a transaction update
    let mut rx = authority_state.subscribe_batch();
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((0, _))
    ));

    // Then we (eventually) get a batch
    assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

    {
        let t0 = &authority_state.batch_notifier.ticket().expect("ok");
        store.side_sequence(t0.seq(), &ExecutionDigests::random());
    }

    // When we close the sending channel we also also end the service task
    authority_state.batch_notifier.close();

    // But the block is made, and sent as a notification.
    assert!(matches!(
        rx.recv().await.unwrap(),
        UpdateItem::Transaction((1, _))
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
    let store = Arc::new(AuthorityStore::open(&path, None));

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
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

        store.side_sequence(t1.seq(), &ExecutionDigests::random());
        store.side_sequence(t3.seq(), &ExecutionDigests::random());
        store.side_sequence(t2.seq(), &ExecutionDigests::random());
        store.side_sequence(t0.seq(), &ExecutionDigests::random());
    }

    // Get transactions in order then batch.
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
    authority_state.batch_notifier.close();

    _join.await.expect("No errors in task").expect("ok");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_batch_manager_drop_out_of_order() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let store = Arc::new(AuthorityStore::open(&path, None));

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let authority_state = Arc::new(init_state(committee, authority_key, store.clone()).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            // Make sure that a batch will not be formed due to time, but will be formed
            // when there are 4 transactions.
            .run_batch_service(4, Duration::from_millis(10000))
            .await
    });
    // Send transactions out of order
    let mut rx = authority_state.subscribe_batch();

    let t0 = authority_state.batch_notifier.ticket().expect("ok");
    let t1 = authority_state.batch_notifier.ticket().expect("ok");
    let t2 = authority_state.batch_notifier.ticket().expect("ok");
    let t3 = authority_state.batch_notifier.ticket().expect("ok");

    store.side_sequence(t1.seq(), &ExecutionDigests::random());
    drop(t1);
    store.side_sequence(t3.seq(), &ExecutionDigests::random());
    drop(t3);
    store.side_sequence(t2.seq(), &ExecutionDigests::random());
    drop(t2);

    // Give a chance to send signals
    tokio::task::yield_now().await;
    // Still nothing has arrived out of order
    assert_eq!(rx.len(), 0);

    store.side_sequence(t0.seq(), &ExecutionDigests::random());
    drop(t0);

    // Get transactions in order then batch.
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
    authority_state.batch_notifier.close();

    _join.await.expect("No errors in task").expect("ok");
}

#[tokio::test]
async fn test_handle_move_order_with_batch() {
    let (sender, sender_key) = KeyPair::get_key_pair();
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
        UpdateItem::Transaction((0, x)) if x.transaction == effects.transaction_digest
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
    let store = Arc::new(AuthorityStore::open(&path, None));

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let authority_state = Arc::new(init_state(committee, authority_key, store.clone()).await);

    let inner_state = authority_state.clone();
    let _join = tokio::task::spawn(async move {
        inner_state
            .run_batch_service(10, Duration::from_secs(6000))
            .await
    });
    // Send transactions out of order
    let tx_zero = ExecutionDigests::random();

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

#[derive(Clone)]
struct TrustworthyAuthorityClient(Arc<Mutex<AuthorityState>>);

#[async_trait]
impl AuthorityAPI for TrustworthyAuthorityClient {
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_confirmation_transaction(
        &self,
        _transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_consensus_transaction(
        &self,
        _transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_account_info_request(
        &self,
        _request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        Ok(AccountInfoResponse {
            object_ids: vec![],
            owner: Default::default(),
        })
    }

    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        Ok(ObjectInfoResponse {
            parent_certificate: None,
            requested_object_reference: None,
            object_and_lock: None,
        })
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        _request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!();
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let secret = self.0.lock().await.secret.clone();
        let name = self.0.lock().await.name;
        let batch_size = 3;

        let mut items = Vec::new();
        let mut last_batch = AuthorityBatch::initial();
        items.push({
            let item = SignedBatch::new(last_batch.clone(), &*secret, name);
            BatchInfoResponseItem(UpdateItem::Batch(item))
        });
        let mut seq = 0;
        while last_batch.next_sequence_number < request.length {
            let mut transactions = Vec::new();
            for _i in 0..batch_size {
                let rnd = ExecutionDigests::random();
                transactions.push((seq, rnd));
                items.push(BatchInfoResponseItem(UpdateItem::Transaction((seq, rnd))));
                seq += 1;
            }

            let new_batch = AuthorityBatch::make_next(&last_batch, &transactions).unwrap();
            last_batch = new_batch;
            items.push({
                let item = SignedBatch::new(last_batch.clone(), &*secret, name);
                BatchInfoResponseItem(UpdateItem::Batch(item))
            });
        }

        items.reverse();

        let stream = stream::unfold(items, |mut items| async move {
            items.pop().map(|item| (Ok(item), items))
        });
        Ok(Box::pin(stream))
    }
}

impl TrustworthyAuthorityClient {
    fn new(state: AuthorityState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
}

#[derive(Clone)]
struct ByzantineAuthorityClient(Arc<Mutex<AuthorityState>>);

#[async_trait]
impl AuthorityAPI for ByzantineAuthorityClient {
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_confirmation_transaction(
        &self,
        _transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_consensus_transaction(
        &self,
        _transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_account_info_request(
        &self,
        _request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        Ok(AccountInfoResponse {
            object_ids: vec![],
            owner: Default::default(),
        })
    }

    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        Ok(ObjectInfoResponse {
            parent_certificate: None,
            requested_object_reference: None,
            object_and_lock: None,
        })
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        _request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!();
    }

    /// Handle Batch information requests for this authority.
    /// This function comes from a byzantine authority that has incorrect behavior.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let secret = self.0.lock().await.secret.clone();
        let name = self.0.lock().await.name;
        let batch_size = 3;

        let mut items = Vec::new();
        let mut last_batch = AuthorityBatch::initial();
        items.push({
            let item = SignedBatch::new(last_batch.clone(), &*secret, name);
            BatchInfoResponseItem(UpdateItem::Batch(item))
        });
        let mut seq = 0;
        while last_batch.next_sequence_number < request.length {
            let mut transactions = Vec::new();
            for _i in 0..batch_size {
                let rnd = ExecutionDigests::random();
                transactions.push((seq, rnd));
                items.push(BatchInfoResponseItem(UpdateItem::Transaction((seq, rnd))));
                seq += 1;
            }

            // Introduce byzantine behaviour:
            // Pop last transaction
            let (seq, _) = transactions.pop().unwrap();
            // Insert a different one
            transactions.push((seq, ExecutionDigests::random()));

            let new_batch = AuthorityBatch::make_next(&last_batch, &transactions).unwrap();
            last_batch = new_batch;
            items.push({
                let item = SignedBatch::new(last_batch.clone(), &*secret, name);
                BatchInfoResponseItem(UpdateItem::Batch(item))
            });
        }

        items.reverse();

        let stream = stream::unfold(items, |mut items| async move {
            items.pop().map(|item| (Ok(item), items))
        });
        Ok(Box::pin(stream))
    }
}

impl ByzantineAuthorityClient {
    fn new(state: AuthorityState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
}

#[tokio::test]
async fn test_safe_batch_stream() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    let (_, authority_key) = KeyPair::get_key_pair();
    let mut authorities = BTreeMap::new();
    let public_key_bytes = *authority_key.public_key_bytes();
    println!("init public key {:?}", public_key_bytes);

    authorities.insert(public_key_bytes, 1);
    let committee = Committee::new(0, authorities).unwrap();
    // Create an authority
    let store = Arc::new(AuthorityStore::open(&path, None));
    let state = AuthorityState::new(
        committee.clone(),
        public_key_bytes,
        Arc::pin(authority_key),
        store.clone(),
        None,
        None,
        None,
        &sui_config::genesis::Genesis::get_default_genesis(),
        &prometheus::Registry::new(),
    )
    .await;

    // Happy path:
    let auth_client = TrustworthyAuthorityClient::new(state);
    let safe_client = SafeClient::new(auth_client, committee.clone(), public_key_bytes);

    let request = BatchInfoRequest {
        start: Some(0),
        length: 15,
    };
    let batch_stream = safe_client.handle_batch_stream(request.clone()).await;

    // No errors expected
    assert!(batch_stream.is_ok());
    let items = batch_stream
        .unwrap()
        .collect::<Vec<Result<BatchInfoResponseItem, SuiError>>>()
        .await;

    // Check length
    assert!(!items.is_empty());
    assert_eq!(items.len(), 15 + 6); // 15 items, and 6 batches (enclosing them)

    let mut error_found = false;
    for item in items {
        if item.is_err() {
            error_found = true;
            println!("error found: {:?}", item);
        }
    }

    assert!(!error_found);

    // Byzantine cases:
    let (_, authority_key) = KeyPair::get_key_pair();
    let public_key_bytes_b = *authority_key.public_key_bytes();
    let state_b = AuthorityState::new(
        committee.clone(),
        public_key_bytes_b,
        Arc::pin(authority_key),
        store,
        None,
        None,
        None,
        &sui_config::genesis::Genesis::get_default_genesis(),
        &prometheus::Registry::new(),
    )
    .await;
    let auth_client_from_byzantine = ByzantineAuthorityClient::new(state_b);
    let safe_client_from_byzantine = SafeClient::new(
        auth_client_from_byzantine,
        committee.clone(),
        public_key_bytes_b,
    );

    let mut batch_stream = safe_client_from_byzantine
        .handle_batch_stream(request.clone())
        .await;

    // We still expect an ok result
    assert!(batch_stream.is_ok());

    let items = batch_stream
        .unwrap()
        .collect::<Vec<Result<BatchInfoResponseItem, SuiError>>>()
        .await;

    // Check length
    assert!(!items.is_empty());
    assert_eq!(items.len(), 15 + 6); // 15 items, and 6 batches (enclosing them)

    let request_b = BatchInfoRequest {
        start: Some(0),
        length: 10,
    };
    batch_stream = safe_client_from_byzantine
        .handle_batch_stream(request_b.clone())
        .await;

    // We still expect an ok result
    assert!(batch_stream.is_ok());

    let items = batch_stream
        .unwrap()
        .collect::<Vec<Result<BatchInfoResponseItem, SuiError>>>()
        .await;

    let mut error_found = false;
    for item in items {
        if item.is_err() {
            error_found = true;
        }
    }
    assert!(error_found);
}

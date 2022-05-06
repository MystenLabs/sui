// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::{authority_tests::max_files_authority_tests, AuthorityState, AuthorityStore},
    authority_batch::batch_tests::init_state_parameters_from_rng,
};
use rand::prelude::StdRng;
use rand::SeedableRng;
use std::{collections::HashSet, env, fs, path::PathBuf, sync::Arc, time::Duration};
use sui_adapter::genesis;
use sui_types::{
    base_types::{AuthorityName, ObjectID},
    batch::UpdateItem,
    utils::make_committee_key,
    waypoint::GlobalCheckpoint,
};

fn random_ckpoint_store() -> Vec<(PathBuf, CheckpointStore)> {
    let (keys, committee) = make_committee_key();

    keys.iter()
        .map(|k| {
            let dir = env::temp_dir();
            let path = dir.join(format!("SC_{:?}", ObjectID::random()));
            fs::create_dir(&path).unwrap();

            // Create an authority
            let mut opts = rocksdb::Options::default();
            opts.set_max_open_files(max_files_authority_tests());

            let cps = CheckpointStore::open(
                path.clone(),
                Some(opts),
                *k.public_key_bytes(),
                committee.clone(),
                Arc::pin(k.copy()),
            )
            .unwrap();
            (path, cps)
        })
        .collect()
}

#[test]
fn crash_recovery() {
    let (keys, committee) = make_committee_key();
    let k = keys[0].copy();

    // Setup

    let dir = env::temp_dir();
    let path = dir.join(format!("SC_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());

    // Open store first time

    let cps = CheckpointStore::open(
        path.clone(),
        Some(opts.clone()),
        *k.public_key_bytes(),
        committee.clone(),
        Arc::pin(k.copy()),
    )
    .unwrap();

    // --- TEST 0 ---

    // Check init from empty works.

    let locals = cps.get_locals();
    assert!(locals.current_proposal.is_none());
    assert!(locals.proposal_next_transaction.is_none());

    // Do stuff

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    let t6 = TransactionDigest::random();

    cps.handle_internal_batch(4, &[(1, t1), (2, t2), (3, t3)])
        .unwrap();

    // --- TEST 1 ---
    // Check the recording of transactions works

    let locals = cps.get_locals();
    assert_eq!(locals.next_transaction_sequence, 4);

    let proposal = cps.set_proposal().unwrap();
    assert_eq!(*proposal.sequence_number(), 0);

    cps.handle_internal_batch(7, &[(4, t4), (5, t5), (6, t6)])
        .unwrap();

    // Delete and re-open DB
    drop(cps);

    let cps_new = CheckpointStore::open(
        path,
        Some(opts),
        *k.public_key_bytes(),
        committee,
        Arc::pin(k.copy()),
    )
    .unwrap();

    // TEST 3 -- the current proposal is correctly recreated.

    let locals = cps_new.get_locals();
    assert!(locals.current_proposal.is_some());
    assert!(locals.proposal_next_transaction.is_some());
    assert_eq!(locals.next_transaction_sequence, 7);

    assert_eq!(
        &proposal.proposal.0.checkpoint,
        &locals
            .current_proposal
            .as_ref()
            .unwrap()
            .proposal
            .0
            .checkpoint
    );
}

#[test]
fn make_checkpoint_db() {
    let (_, cps) = random_ckpoint_store().pop().unwrap();

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    let t6 = TransactionDigest::random();

    cps.update_processed_transactions(&[(1, t1), (2, t2), (3, t3)])
        .unwrap();
    assert!(cps.checkpoint_contents.iter().count() == 0);
    assert!(cps.extra_transactions.iter().count() == 3);
    assert!(cps.unprocessed_transactions.iter().count() == 0);

    assert!(cps.next_checkpoint() == 0);

    cps.update_new_checkpoint(0, &[t1, t2, t4, t5]).unwrap();
    assert!(cps.checkpoint_contents.iter().count() == 4);
    assert_eq!(cps.extra_transactions.iter().count(), 1);
    assert!(cps.unprocessed_transactions.iter().count() == 2);

    assert_eq!(cps.lowest_unprocessed_checkpoint(), 0);

    let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
    assert!(tx_seq >= u64::MAX / 2);

    assert!(cps.next_checkpoint() == 1);

    cps.update_processed_transactions(&[(4, t4), (5, t5), (6, t6)])
        .unwrap();
    assert!(cps.checkpoint_contents.iter().count() == 4);
    assert_eq!(cps.extra_transactions.iter().count(), 2); // t3 & t6
    assert!(cps.unprocessed_transactions.iter().count() == 0);

    assert_eq!(cps.lowest_unprocessed_checkpoint(), 1);

    let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
    assert_eq!(tx_seq, 4);
}

#[test]
fn make_proposals() {
    let mut test_objects = random_ckpoint_store();
    let (_, cps1) = test_objects.pop().unwrap();
    let (_, cps2) = test_objects.pop().unwrap();
    let (_, cps3) = test_objects.pop().unwrap();
    let (_, cps4) = test_objects.pop().unwrap();

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal().unwrap();
    let p2 = cps2.set_proposal().unwrap();
    let p3 = cps3.set_proposal().unwrap();

    let ckp_items: Vec<_> = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned()
        .collect();

    cps1.update_new_checkpoint(0, &ckp_items[..]).unwrap();
    cps2.update_new_checkpoint(0, &ckp_items[..]).unwrap();
    cps3.update_new_checkpoint(0, &ckp_items[..]).unwrap();
    cps4.update_new_checkpoint(0, &ckp_items[..]).unwrap();

    assert!(
        cps4.unprocessed_transactions.keys().collect::<HashSet<_>>()
            == [t1, t2, t3].into_iter().collect::<HashSet<_>>()
    );

    assert!(
        cps4.extra_transactions.keys().collect::<HashSet<_>>()
            == [t5].into_iter().collect::<HashSet<_>>()
    );
}

#[test]
fn make_diffs() {
    let mut test_objects = random_ckpoint_store();
    let (_, cps1) = test_objects.pop().unwrap();
    let (_, cps2) = test_objects.pop().unwrap();
    let (_, cps3) = test_objects.pop().unwrap();
    let (_, cps4) = test_objects.pop().unwrap();

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal().unwrap();
    let p2 = cps2.set_proposal().unwrap();
    let p3 = cps3.set_proposal().unwrap();
    let p4 = cps4.set_proposal().unwrap();

    let diff12 = p1.diff_with(&p2);
    let diff23 = p2.diff_with(&p3);

    let mut global = GlobalCheckpoint::<AuthorityName, TransactionDigest>::new(0);
    global.insert(diff12.diff.clone()).unwrap();
    global.insert(diff23.diff).unwrap();

    // P4 proposal not selected
    let diff41 = p4.diff_with(&p1);
    let all_items4 = global
        .checkpoint_items(&diff41.diff, p4.transactions().cloned().collect())
        .unwrap();

    // P1 proposal selected
    let all_items1 = global
        .checkpoint_items(&diff12.diff, p1.transactions().cloned().collect())
        .unwrap();

    // All get the same set for the proposal
    assert_eq!(all_items1, all_items4);
}

#[test]
fn latest_proposal() {
    let mut test_objects = random_ckpoint_store();
    let (_, cps1) = test_objects.pop().unwrap();
    let (_, cps2) = test_objects.pop().unwrap();
    let (_, cps3) = test_objects.pop().unwrap();
    let (_, cps4) = test_objects.pop().unwrap();

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    // --- TEST 0 ---

    // No checkpoint no proposal

    let request = CheckpointRequest::latest(false);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(response.detail.is_none());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_none());
        assert!(matches!(previous, AuthenticatedCheckpoint::None));
    }

    // ---

    let p1 = cps1.set_proposal().unwrap();
    let p2 = cps2.set_proposal().unwrap();
    let p3 = cps3.set_proposal().unwrap();

    // --- TEST 1 ---

    // First checkpoint condition

    // Check the latest checkpoint with no detail
    let request = CheckpointRequest::latest(false);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(response.detail.is_none());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_some());
        assert!(matches!(previous, AuthenticatedCheckpoint::None));

        let current_proposal = current.unwrap();
        current_proposal
            .0
            .check_digest()
            .expect("no signature error");
        assert_eq!(*current_proposal.0.checkpoint.sequence_number(), 0);
    }

    // --- TEST 2 ---

    // Check the latest checkpoint with detail
    let request = CheckpointRequest::latest(true);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(response.detail.is_some());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_some());
        assert!(matches!(previous, AuthenticatedCheckpoint::None));

        let current_proposal = current.unwrap();
        current_proposal
            .0
            .check_transactions(response.detail.as_ref().unwrap())
            .expect("no signature error");
        assert_eq!(*current_proposal.0.checkpoint.sequence_number(), 0);
    }

    // ---

    let ckp_items: Vec<_> = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned()
        .collect();

    let transactions = CheckpointContents::new(ckp_items.clone().into_iter());
    let summary = CheckpointSummary::new(0, &transactions);

    cps1.handle_internal_set_checkpoint(summary.clone(), &transactions)
        .unwrap();
    cps2.handle_internal_set_checkpoint(summary.clone(), &transactions)
        .unwrap();
    cps3.handle_internal_set_checkpoint(summary.clone(), &transactions)
        .unwrap();
    cps4.handle_internal_set_checkpoint(summary, &transactions)
        .unwrap();

    // --- TEST3 ---

    // No valid checkpoint proposal condition...
    assert!(cps1.get_locals().current_proposal.is_none());
    // ... because a valid checkpoint cannot be generated.
    assert!(cps1.set_proposal().is_err());

    let request = CheckpointRequest::latest(false);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(response.detail.is_none());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_none());
        assert!(matches!(previous, AuthenticatedCheckpoint::Signed { .. }));
    }

    // --- TEST 4 ---

    // When details are needed, then return unexecuted trasnactions if there is no proposal
    let request = CheckpointRequest::latest(true);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(response.detail.is_some());
    use typed_store::traits::Map;
    let txs = response.detail.unwrap();
    let unprocessed = CheckpointContents::new(cps1.unprocessed_transactions.keys());
    assert_eq!(txs.transactions, unprocessed.transactions);

    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_none());
        assert!(matches!(previous, AuthenticatedCheckpoint::Signed { .. }));
    }

    // ---
    use std::iter;
    let batch: Vec<_> = ckp_items
        .into_iter()
        .chain(iter::once(t6))
        .enumerate()
        .map(|(seq, item)| (seq as u64 + 2, item))
        .collect();
    cps1.update_processed_transactions(&batch[..]).unwrap();

    let _p1 = cps1.set_proposal().unwrap();

    // --- TEST 5 ---

    // Get the full proposal with previous proposal
    let request = CheckpointRequest::latest(true);
    let response = cps1.handle_latest_proposal(&request).expect("no errors");
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Proposal { .. }
    ));
    if let AuthorityCheckpointInfo::Proposal { current, previous } = response.info {
        assert!(current.is_some());
        assert!(matches!(previous, AuthenticatedCheckpoint::Signed { .. }));

        let current_proposal = current.unwrap();
        current_proposal
            .0
            .check_digest()
            .expect("no signature error");
        assert_eq!(*current_proposal.0.checkpoint.sequence_number(), 1);
    }
}

#[test]
fn set_get_checkpoint() {
    let mut test_objects = random_ckpoint_store();
    let (_, cps1) = test_objects.pop().unwrap();
    let (_, cps2) = test_objects.pop().unwrap();
    let (_, cps3) = test_objects.pop().unwrap();
    let (_, cps4) = test_objects.pop().unwrap();

    let t1 = TransactionDigest::random();
    let t2 = TransactionDigest::random();
    let t3 = TransactionDigest::random();
    let t4 = TransactionDigest::random();
    let t5 = TransactionDigest::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal().unwrap();
    let p2 = cps2.set_proposal().unwrap();
    let p3 = cps3.set_proposal().unwrap();

    // --- TEST 0 ---

    // There is no previous checkpoint
    let request = CheckpointRequest::past(0, true);
    let response = cps1.handle_checkpoint_request(&request).unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::None)
    ));
    assert!(response.detail.is_none());

    // There is no previous checkpoint
    let request = CheckpointRequest::past(0, true);
    let response = cps1.handle_checkpoint_request(&request).unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::None)
    ));
    assert!(response.detail.is_none());

    // ---

    let ckp_items = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned();

    let transactions = CheckpointContents::new(ckp_items);
    let summary = CheckpointSummary::new(0, &transactions);

    cps1.handle_internal_set_checkpoint(summary.clone(), &transactions)
        .unwrap();
    cps2.handle_internal_set_checkpoint(summary.clone(), &transactions)
        .unwrap();
    cps3.handle_internal_set_checkpoint(summary, &transactions)
        .unwrap();
    // cps4.handle_internal_set_checkpoint(summary, &transactions)
    //     .unwrap();

    // --- TEST 1 ---

    // Now we have a signed checkpoint
    let request = CheckpointRequest::past(0, true);
    let response = cps1.handle_checkpoint_request(&request).unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Signed(..))
    ));
    if let AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Signed(signed)) = response.info {
        signed
            .check_transactions(&response.detail.unwrap())
            .unwrap();
    }

    // Make a certificate
    let mut signed_checkpoint: Vec<SignedCheckpoint> = Vec::new();
    for x in [&cps1, &cps2, &cps3] {
        match x.handle_checkpoint_request(&request).unwrap().info {
            AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Signed(signed)) => {
                signed_checkpoint.push(signed)
            }
            _ => unreachable!(),
        };
    }

    // --- TEST 2 ---

    // We can set the checkpoint cert to those that have it

    let checkpoint_cert =
        CertifiedCheckpoint::aggregate(signed_checkpoint, &cps1.committee).unwrap();

    // Send the certificate to a party that has the data
    let request_ckp = CheckpointRequest::set_checkpoint(checkpoint_cert.clone(), None);
    let response_ckp = cps1.handle_checkpoint_request(&request_ckp).unwrap();
    assert!(matches!(
        response_ckp.info,
        AuthorityCheckpointInfo::Success
    ));

    // Now we have a certified checkpoint
    let request = CheckpointRequest::past(0, true);
    let response = cps1.handle_checkpoint_request(&request).unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Certified(..))
    ));

    // --- TEST 3 ---

    // Setting just cert to a node that does not have the checkpoint fails
    let request_ckp = CheckpointRequest::set_checkpoint(checkpoint_cert.clone(), None);
    let response_ckp = cps4.handle_checkpoint_request(&request_ckp);
    assert!(response_ckp.is_err());

    // Setting with contents suceeds
    let request_ckp = CheckpointRequest::set_checkpoint(checkpoint_cert, Some(transactions));
    let response_ckp = cps4.handle_checkpoint_request(&request_ckp).unwrap();
    assert!(matches!(
        response_ckp.info,
        AuthorityCheckpointInfo::Success
    ));

    // Now we have a certified checkpoint
    let request = CheckpointRequest::past(0, true);
    let response = cps4.handle_checkpoint_request(&request).unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Certified(..))
    ));
}

#[test]
fn checkpoint_integration() {
    let (keys, committee) = make_committee_key();
    let k = keys[0].copy();

    // Setup

    let dir = env::temp_dir();
    let path = dir.join(format!("SC_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());

    // Make a checkpoint store:

    let cps = CheckpointStore::open(
        path,
        Some(opts.clone()),
        *k.public_key_bytes(),
        committee,
        Arc::pin(k.copy()),
    )
    .unwrap();

    let mut next_tx_num: TxSequenceNumber = 0;
    let mut unprocessed = Vec::new();
    while cps.get_locals().next_checkpoint < 10 {
        let old_checkpoint = cps.get_locals().next_checkpoint;

        let some_fresh_transactions: Vec<_> = (0..7)
            .map(|_| TransactionDigest::random())
            .chain(unprocessed.clone().into_iter())
            .enumerate()
            .map(|(i, d)| (i as u64 + next_tx_num, d))
            .collect();
        next_tx_num = some_fresh_transactions
            .iter()
            .map(|(s, _)| s)
            .max()
            .unwrap()
            + 1;

        // Step 0. Add transactions to checkpoint
        cps.handle_internal_batch(next_tx_num, &some_fresh_transactions[..])
            .unwrap();

        // Step 1. Make a proposal
        let _proposal = cps.set_proposal().unwrap();

        // Step 2. Continue to process transactions while a proposal is out.
        let some_fresh_transactions: Vec<_> = (0..7)
            .map(|_| TransactionDigest::random())
            .enumerate()
            .map(|(i, d)| (i as u64 + next_tx_num, d))
            .collect();
        next_tx_num = some_fresh_transactions
            .iter()
            .map(|(s, _)| s)
            .max()
            .unwrap()
            + 1;

        // Step 3. Receive a Checkpoint
        unprocessed = (0..5)
            .map(|_| TransactionDigest::random())
            .into_iter()
            .chain(some_fresh_transactions.iter().cloned().map(|(_, d)| d))
            .collect();
        let transactions = CheckpointContents::new(unprocessed.clone().into_iter());
        let summary = CheckpointSummary::new(cps.get_locals().next_checkpoint, &transactions);

        cps.handle_internal_set_checkpoint(summary.clone(), &transactions)
            .unwrap();

        // Cannot make a checkpoint proposal before adding the unprocessed transactions
        assert!(cps.set_proposal().is_err());
        // Loop invariant to ensure termination or error
        assert_eq!(cps.get_locals().next_checkpoint, old_checkpoint + 1);
    }
}

// Now check the connection between state / bacth and checkpoint mechanism

#[tokio::test]
async fn test_batch_to_checkpointing() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));

    let mut store_path = path.clone();
    store_path.push("store");
    let store = Arc::new(AuthorityStore::open(&store_path, Some(opts)));

    let mut checkpoints_path = path.clone();
    checkpoints_path.push("checkpoints");

    let secret = Arc::pin(authority_key);
    let checkpoints = Arc::new(
        CheckpointStore::open(
            &checkpoints_path,
            None,
            *secret.public_key_bytes(),
            committee.clone(),
            secret.clone(),
        )
        .unwrap(),
    );

    let state = AuthorityState::new(
        committee,
        *secret.public_key_bytes(),
        secret,
        store.clone(),
        Some(checkpoints.clone()),
        genesis::clone_genesis_compiled_modules(),
        &mut genesis::get_genesis_context(),
    )
    .await;
    let authority_state = Arc::new(state);

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

    // Now once we have a batch we should also have stuff in the checkpoint
    assert_eq!(checkpoints.next_transaction_sequence_expected(), 4);

    // When we close the sending channel we also also end the service task
    authority_state.batch_notifier.close();

    _join.await.expect("No errors in task").expect("ok");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_batch_to_checkpointing_init_crash() {
    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));

    let mut store_path = path.clone();
    store_path.push("store");

    let mut checkpoints_path = path.clone();
    checkpoints_path.push("checkpoints");

    let secret = Arc::pin(authority_key);

    // Scope to ensure all variables are dropped
    {
        let store = Arc::new(AuthorityStore::open(&store_path, Some(opts.clone())));

        let state = AuthorityState::new(
            committee.clone(),
            *secret.public_key_bytes(),
            secret.clone(),
            store.clone(),
            None,
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await;
        let authority_state = Arc::new(state);

        let inner_state = authority_state.clone();
        let _join = tokio::task::spawn(async move {
            inner_state
                .run_batch_service(1000, Duration::from_millis(500))
                .await
        });

        tokio::time::advance(Duration::from_millis(10)).await;
        tokio::task::yield_now().await;

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
        let v = rx.recv().await;
        assert!(matches!(v.unwrap(), UpdateItem::Transaction((3, _))));

        // Then we (eventually) get a batch
        assert!(matches!(rx.recv().await.unwrap(), UpdateItem::Batch(_)));

        // When we close the sending channel we also also end the service task
        authority_state.batch_notifier.close();

        _join.await.expect("No errors in task").expect("ok");
    }

    // Scope to ensure all variables are dropped
    {
        let store = Arc::new(AuthorityStore::open(&store_path, Some(opts)));

        let checkpoints = Arc::new(
            CheckpointStore::open(
                &checkpoints_path,
                None,
                *secret.public_key_bytes(),
                committee.clone(),
                secret.clone(),
            )
            .unwrap(),
        );

        // Start with no transactions
        assert_eq!(checkpoints.next_transaction_sequence_expected(), 0);

        let state = AuthorityState::new(
            committee,
            *secret.public_key_bytes(),
            secret,
            store.clone(),
            Some(checkpoints.clone()),
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await;
        let authority_state = Arc::new(state);

        // But init feeds the transactions in
        assert_eq!(checkpoints.next_transaction_sequence_expected(), 4);

        // When we close the sending channel we also also end the service task
        authority_state.batch_notifier.close();
    }
}

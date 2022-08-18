// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::execution_driver::PendCertificateForExecutionNoop,
    authority_aggregator::{
        authority_aggregator_tests::transfer_coin_transaction, AuthorityAggregator,
    },
    authority_batch::batch_tests::init_state_parameters_from_rng,
    authority_client::LocalAuthorityClient,
    safe_client::SafeClientMetrics,
};
use rand::prelude::StdRng;
use rand::SeedableRng;
use std::{collections::HashSet, env, fs, path::PathBuf, sync::Arc, time::Duration};
use sui_types::{
    base_types::{AuthorityName, ObjectID, TransactionDigest},
    batch::UpdateItem,
    crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, KeypairTraits},
    messages::{CertifiedTransaction, ExecutionStatus},
    messages_checkpoint::CheckpointRequest,
    object::Object,
    utils::{make_committee_key, make_committee_key_num},
    waypoint::GlobalCheckpoint,
};

use crate::authority_aggregator::AuthAggMetrics;
use crate::epoch::epoch_store::EpochStore;
use parking_lot::Mutex;

pub struct TestCausalOrderPendCertNoop;

impl CausalOrder for TestCausalOrderPendCertNoop {
    fn get_complete_causal_order<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests>,
        _ckpt_store: &mut CheckpointStore,
    ) -> SuiResult<Vec<ExecutionDigests>> {
        Ok(transactions.cloned().collect())
    }
}

impl PendCertificateForExecution for TestCausalOrderPendCertNoop {
    fn add_pending_certificates(
        &self,
        _certs: Vec<(TransactionDigest, Option<CertifiedTransaction>)>,
    ) -> SuiResult<()> {
        Ok(())
    }
}

fn random_ckpoint_store() -> (
    Committee,
    Vec<AuthorityKeyPair>,
    Vec<(PathBuf, CheckpointStore)>,
) {
    random_ckpoint_store_num(4)
}

fn random_ckpoint_store_num(
    num: usize,
) -> (
    Committee,
    Vec<AuthorityKeyPair>,
    Vec<(PathBuf, CheckpointStore)>,
) {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key_num(num, &mut rng);
    let stores = keys
        .iter()
        .map(|k| {
            let dir = env::temp_dir();
            let path = dir.join(format!("SC_{:?}", ObjectID::random()));
            fs::create_dir(&path).unwrap();

            // Create an authority
            let cps = CheckpointStore::open(
                &path,
                None,
                committee.epoch,
                k.public().into(),
                Arc::pin(k.copy()),
            )
            .unwrap();
            (path, cps)
        })
        .collect();
    (committee, keys, stores)
}

#[test]
fn crash_recovery() {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key(&mut rng);
    let k = keys[0].copy();

    // Setup

    let dir = env::temp_dir();
    let path = dir.join(format!("SC_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    // Open store first time

    let mut cps = CheckpointStore::open(
        &path,
        None,
        committee.epoch,
        k.public().into(),
        Arc::pin(k.copy()),
    )
    .unwrap();

    // --- TEST 0 ---

    // Check init from empty works.

    let locals = cps.get_locals();
    assert!(locals.current_proposal.is_none());
    assert!(locals.proposal_next_transaction.is_none());

    // Do stuff

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    let t6 = ExecutionDigests::random();

    cps.handle_internal_batch(4, &[(1, t1), (2, t2), (3, t3)])
        .unwrap();

    // --- TEST 1 ---
    // Check the recording of transactions works

    let locals = cps.get_locals();
    assert_eq!(locals.next_transaction_sequence, 4);

    let proposal = cps.set_proposal(committee.epoch).unwrap();
    assert_eq!(*proposal.sequence_number(), 0);

    cps.handle_internal_batch(7, &[(4, t4), (5, t5), (6, t6)])
        .unwrap();

    // Delete and re-open DB
    drop(cps);

    let mut cps_new = CheckpointStore::open(
        &path,
        None,
        committee.epoch,
        k.public().into(),
        Arc::pin(k.copy()),
    )
    .unwrap();

    // TEST 3 -- the current proposal is correctly recreated.

    let locals = cps_new.get_locals();
    assert!(locals.current_proposal.is_some());
    assert!(locals.proposal_next_transaction.is_some());
    assert_eq!(locals.next_transaction_sequence, 7);

    assert_eq!(
        &proposal.signed_summary.summary,
        &locals
            .current_proposal
            .as_ref()
            .unwrap()
            .signed_summary
            .summary
    );
}

#[test]
fn make_checkpoint_db() {
    let (_committee, _keys, mut stores) = random_ckpoint_store();
    let (_, mut cps) = stores.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    let t6 = ExecutionDigests::random();

    cps.update_processed_transactions(&[(1, t1), (2, t2), (3, t3)])
        .unwrap();
    assert_eq!(cps.tables.checkpoint_contents.iter().count(), 0);
    assert_eq!(cps.tables.extra_transactions.iter().count(), 3);

    assert_eq!(cps.next_checkpoint(), 0);

    // You cannot make a checkpoint without processing all transactions
    assert!(cps
        .update_new_checkpoint(
            0,
            &CheckpointContents::new([t1, t2, t4, t5].into_iter()),
            PendCertificateForExecutionNoop
        )
        .is_err());

    // Now process the extra transactions in the checkpoint
    cps.update_processed_transactions(&[(4, t4), (5, t5)])
        .unwrap();

    cps.update_new_checkpoint(
        0,
        &CheckpointContents::new([t1, t2, t4, t5].into_iter()),
        PendCertificateForExecutionNoop,
    )
    .unwrap();
    assert_eq!(cps.tables.checkpoint_contents.iter().count(), 1);
    assert_eq!(cps.tables.extra_transactions.iter().count(), 1);

    cps.update_processed_transactions(&[(6, t6)]).unwrap();
    assert_eq!(cps.tables.checkpoint_contents.iter().count(), 1);
    assert_eq!(cps.tables.extra_transactions.iter().count(), 2); // t3 & t6

    let (_cp_seq, tx_seq) = cps
        .tables
        .transactions_to_checkpoint
        .get(&t4)
        .unwrap()
        .unwrap();
    assert_eq!(tx_seq, 4);
}

#[test]
fn make_proposals() {
    let (committee, _keys, mut stores) = random_ckpoint_store();
    let (_, mut cps1) = stores.pop().unwrap();
    let (_, mut cps2) = stores.pop().unwrap();
    let (_, mut cps3) = stores.pop().unwrap();
    let (_, mut cps4) = stores.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let p3 = cps3.set_proposal(committee.epoch).unwrap();

    let ckp_items: Vec<_> = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned()
        .collect();

    // if not all transactions are processed we fail
    assert!(cps1
        .update_new_checkpoint(
            0,
            &CheckpointContents::new(ckp_items.iter().cloned()),
            PendCertificateForExecutionNoop
        )
        .is_err());

    cps1.update_processed_transactions(&[(3, t1), (4, t4)])
        .unwrap();

    cps2.update_processed_transactions(&[(3, t3), (4, t4)])
        .unwrap();

    cps3.update_processed_transactions(&[(3, t1), (4, t2)])
        .unwrap();

    cps4.update_processed_transactions(&[(3, t1), (4, t2), (5, t3)])
        .unwrap();

    cps1.update_new_checkpoint(
        0,
        &CheckpointContents::new(ckp_items.iter().cloned()),
        PendCertificateForExecutionNoop,
    )
    .unwrap();
    cps2.update_new_checkpoint(
        0,
        &CheckpointContents::new(ckp_items.iter().cloned()),
        PendCertificateForExecutionNoop,
    )
    .unwrap();
    cps3.update_new_checkpoint(
        0,
        &CheckpointContents::new(ckp_items.iter().cloned()),
        PendCertificateForExecutionNoop,
    )
    .unwrap();
    cps4.update_new_checkpoint(
        0,
        &CheckpointContents::new(ckp_items.iter().cloned()),
        PendCertificateForExecutionNoop,
    )
    .unwrap();

    assert_eq!(
        cps4.tables
            .extra_transactions
            .keys()
            .collect::<HashSet<_>>(),
        [t5].into_iter().collect::<HashSet<_>>()
    );
}

#[test]
fn make_diffs() {
    let (committee, _keys, mut stores) = random_ckpoint_store();
    let (_, mut cps1) = stores.pop().unwrap();
    let (_, mut cps2) = stores.pop().unwrap();
    let (_, mut cps3) = stores.pop().unwrap();
    let (_, mut cps4) = stores.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let p3 = cps3.set_proposal(committee.epoch).unwrap();
    let p4 = cps4.set_proposal(committee.epoch).unwrap();

    let diff12 = p1.fragment_with(&p2);
    let diff23 = p2.fragment_with(&p3);

    let mut global = GlobalCheckpoint::<AuthorityName, ExecutionDigests>::new();
    global.insert(diff12.diff.clone()).unwrap();
    global.insert(diff23.diff).unwrap();

    // P4 proposal not selected
    let diff41 = p4.fragment_with(&p1);
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
    let (committee, _keys, mut stores) = random_ckpoint_store();
    let (_, mut cps1) = stores.pop().unwrap();
    let (_, mut cps2) = stores.pop().unwrap();
    let (_, mut cps3) = stores.pop().unwrap();
    let (_, mut cps4) = stores.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    let t6 = ExecutionDigests::random();

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

    let response = cps1.handle_proposal(false).expect("no errors");
    assert!(response.detail.is_none());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::CheckpointProposal {
            proposal: None,
            prev_cert: None,
        }
    ));

    // ---

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let p3 = cps3.set_proposal(committee.epoch).unwrap();

    // --- TEST 1 ---

    // First checkpoint condition

    // Check the latest checkpoint with no detail
    let response = cps1.handle_proposal(false).expect("no errors");
    assert!(response.detail.is_none());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::CheckpointProposal { .. }
    ));
    if let AuthorityCheckpointInfo::CheckpointProposal {
        proposal,
        prev_cert,
    } = response.info
    {
        assert!(proposal.is_some());
        assert!(prev_cert.is_none());

        let current_proposal = proposal.unwrap();
        current_proposal
            .verify(&committee, None)
            .expect("no signature error");
        assert_eq!(current_proposal.summary.sequence_number, 0);
    }

    // --- TEST 2 ---

    // Check the latest checkpoint with detail
    let response = cps1.handle_proposal(true).expect("no errors");
    assert!(response.detail.is_some());
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::CheckpointProposal { .. }
    ));
    if let AuthorityCheckpointInfo::CheckpointProposal {
        proposal,
        prev_cert,
    } = response.info
    {
        assert!(proposal.is_some());
        assert!(prev_cert.is_none());

        let current_proposal = proposal.unwrap();
        current_proposal
            .verify(&committee, response.detail.as_ref())
            .expect("no signature error");
        assert_eq!(current_proposal.summary.sequence_number, 0);
    }

    // ---

    let ckp_items = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned();

    let transactions = CheckpointContents::new(ckp_items);
    let summary = CheckpointSummary::new(committee.epoch, 0, &transactions, None);

    // Fail to set if transactions not processed.
    assert!(cps1
        .sign_new_checkpoint(
            summary.epoch,
            summary.sequence_number,
            &transactions,
            summary.previous_digest,
            TestCausalOrderPendCertNoop,
        )
        .is_err());

    // Set the transactions as executed.
    let batch: Vec<_> = transactions
        .transactions
        .iter()
        .enumerate()
        .map(|(u, c)| (u as u64, *c))
        .collect();
    cps1.handle_internal_batch(0, &batch).unwrap();
    cps2.handle_internal_batch(0, &batch).unwrap();
    cps3.handle_internal_batch(0, &batch).unwrap();
    cps4.handle_internal_batch(0, &batch).unwrap();

    // Try to get checkpoint
    cps1.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    cps2.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    cps3.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    cps4.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();

    // --- TEST3 ---

    // Proposals are not cleared until we have a cert.
    assert!(cps1.get_locals().current_proposal.is_some());

    let signed: Vec<_> = [
        cps1.latest_stored_checkpoint().unwrap(),
        cps2.latest_stored_checkpoint().unwrap(),
    ]
    .into_iter()
    .map(|a| match a {
        AuthenticatedCheckpoint::Signed(s) => s,
        _ => panic!("Unexpected type"),
    })
    .collect();
    // We only need f+1 to make a cert. 2 is sufficient.
    let cert = CertifiedCheckpointSummary::aggregate(signed, &committee).unwrap();
    cps1.promote_signed_checkpoint_to_cert(&cert, &committee, &CheckpointMetrics::new_for_tests())
        .unwrap();

    let response = cps1.handle_proposal(false).expect("no errors");
    assert!(response.detail.is_none());
    // The proposal should have been cleared now.
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::CheckpointProposal {
            proposal: None,
            prev_cert: None,
        }
    ));

    // ---
    cps1.update_processed_transactions(&[(6, t6)]).unwrap();

    // Create a new proposal.
    let _p1 = cps1.set_proposal(committee.epoch).unwrap();

    // --- TEST 4 ---

    // Get the full proposal with previous proposal
    let response = cps1.handle_proposal(true).expect("no errors");
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::CheckpointProposal { .. }
    ));
    if let AuthorityCheckpointInfo::CheckpointProposal {
        proposal,
        prev_cert,
    } = response.info
    {
        assert!(proposal.is_some());
        assert!(matches!(prev_cert, Some(_)));

        let current_proposal = proposal.unwrap();
        current_proposal
            .verify(&committee, None)
            .expect("no signature error");
        assert_eq!(current_proposal.summary.sequence_number, 1);
    }
}

#[test]
fn set_get_checkpoint() {
    let (committee, _keys, mut stores) = random_ckpoint_store();
    let metrics = CheckpointMetrics::new_for_tests();
    let (_, mut cps1) = stores.pop().unwrap();
    let (_, mut cps2) = stores.pop().unwrap();
    let (_, mut cps3) = stores.pop().unwrap();
    let (_, mut cps4) = stores.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let p3 = cps3.set_proposal(committee.epoch).unwrap();

    // --- TEST 0 ---

    // There is no previous checkpoint
    let response = cps1
        .handle_authenticated_checkpoint(&Some(0), true)
        .unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::AuthenticatedCheckpoint(None)
    ));
    assert!(response.detail.is_none());

    // There is no previous checkpoint
    let response = cps1
        .handle_authenticated_checkpoint(&Some(0), true)
        .unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::AuthenticatedCheckpoint(None)
    ));
    assert!(response.detail.is_none());

    // ---

    let ckp_items = p1
        .transactions()
        .chain(p2.transactions())
        .chain(p3.transactions())
        .cloned();

    let transactions = CheckpointContents::new(ckp_items);
    let summary = CheckpointSummary::new(committee.epoch, 0, &transactions, None);

    // Need to load the transactions as processed, before getting a checkpoint.
    assert!(cps1
        .sign_new_checkpoint(
            summary.epoch,
            summary.sequence_number,
            &transactions,
            summary.previous_digest,
            TestCausalOrderPendCertNoop,
        )
        .is_err());
    let batch: Vec<_> = transactions
        .transactions
        .iter()
        .enumerate()
        .map(|(u, c)| (u as u64, *c))
        .collect();
    cps1.handle_internal_batch(0, &batch).unwrap();
    cps2.handle_internal_batch(0, &batch).unwrap();
    cps3.handle_internal_batch(0, &batch).unwrap();

    cps1.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    cps2.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    cps3.sign_new_checkpoint(
        summary.epoch,
        summary.sequence_number,
        &transactions,
        summary.previous_digest,
        TestCausalOrderPendCertNoop,
    )
    .unwrap();
    // cps4.handle_internal_set_checkpoint(summary, &transactions)
    //     .unwrap();

    // --- TEST 1 ---

    // Now we have a signed checkpoint
    let response = cps1
        .handle_authenticated_checkpoint(&Some(0), true)
        .unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(AuthenticatedCheckpoint::Signed(..)))
    ));
    if let AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(
        AuthenticatedCheckpoint::Signed(signed),
    )) = response.info
    {
        signed.verify(&committee, response.detail.as_ref()).unwrap();
    }

    // Make a certificate
    let mut signed_checkpoint: Vec<SignedCheckpointSummary> = Vec::new();
    for x in [&mut cps1, &mut cps2, &mut cps3] {
        match x
            .handle_authenticated_checkpoint(&Some(0), true)
            .unwrap()
            .info
        {
            AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(
                AuthenticatedCheckpoint::Signed(signed),
            )) => signed_checkpoint.push(signed),
            _ => unreachable!(),
        };
    }

    // --- TEST 2 ---

    // We can set the checkpoint cert to those that have it

    let checkpoint_cert =
        CertifiedCheckpointSummary::aggregate(signed_checkpoint, &committee).unwrap();

    // Send the certificate to a party that has the data
    cps1.promote_signed_checkpoint_to_cert(&checkpoint_cert, &committee, &metrics)
        .unwrap();

    // Now we have a certified checkpoint
    let response = cps1
        .handle_authenticated_checkpoint(&Some(0), true)
        .unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(AuthenticatedCheckpoint::Certified(
            ..
        )))
    ));

    // --- TEST 3 ---

    // Setting with contents succeeds BUT has not processed transactions
    let response_ckp = cps4.process_new_checkpoint_certificate(
        &checkpoint_cert,
        &transactions.clone(),
        &committee,
        TestCausalOrderPendCertNoop,
        &metrics,
    );
    assert!(response_ckp.is_err());

    // Process transactions and then ask for checkpoint.
    cps4.handle_internal_batch(0, &batch).unwrap();
    cps4.process_new_checkpoint_certificate(
        &checkpoint_cert,
        &transactions,
        &committee,
        TestCausalOrderPendCertNoop,
        &metrics,
    )
    .unwrap();

    // Now we have a certified checkpoint
    let response = cps4
        .handle_authenticated_checkpoint(&Some(0), true)
        .unwrap();
    assert!(matches!(
        response.info,
        AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(AuthenticatedCheckpoint::Certified(
            ..
        )))
    ));
}

#[test]
fn checkpoint_integration() {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key(&mut rng);
    let k = keys[0].copy();

    // Setup

    let dir = env::temp_dir();
    let path = dir.join(format!("SC_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    // Make a checkpoint store:

    let mut cps = CheckpointStore::open(
        &path,
        None,
        committee.epoch,
        k.public().into(),
        Arc::pin(k.copy()),
    )
    .unwrap();

    let mut next_tx_num: TxSequenceNumber = 0;
    let mut unprocessed = Vec::new();
    let mut checkpoint_opt: Option<(CheckpointSummary, CheckpointContents)> = None;
    while cps.get_locals().next_checkpoint < 10 {
        let old_checkpoint = cps.get_locals().next_checkpoint;

        let some_fresh_transactions: Vec<_> = (0..7)
            .map(|_| ExecutionDigests::random())
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

        // If we have a previous checkpoint, now lets try to process again?
        if let Some((summary, transactions)) = checkpoint_opt.take() {
            assert!(cps
                .sign_new_checkpoint(
                    summary.epoch,
                    summary.sequence_number,
                    &transactions,
                    summary.previous_digest,
                    TestCausalOrderPendCertNoop,
                )
                .is_ok());
            // Turn the signed checkpoint to a cert. This is required to make progress.
            let checkpoint = match cps.latest_stored_checkpoint().unwrap() {
                AuthenticatedCheckpoint::Signed(s) => s.summary,
                _ => unreachable!(),
            };
            let signatures: Vec<_> = keys
                .iter()
                .map(|key| {
                    let name = key.public().into();
                    SignedCheckpointSummary::new_from_summary(checkpoint.clone(), name, key)
                })
                .collect();
            let cert = CertifiedCheckpointSummary::aggregate(signatures, &committee).unwrap();
            cps.promote_signed_checkpoint_to_cert(
                &cert,
                &committee,
                &CheckpointMetrics::new_for_tests(),
            )
            .unwrap();

            // Loop invariant to ensure termination or error
            assert_eq!(cps.get_locals().next_checkpoint, old_checkpoint + 1);
        }

        // Step 1. Make a proposal
        let initial_proposal = cps.set_proposal(committee.epoch).unwrap();

        // Step 2. Continue to process transactions while a proposal is out.
        let some_fresh_transactions: Vec<_> = (0..7)
            .map(|_| ExecutionDigests::random())
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
            .map(|_| ExecutionDigests::random())
            .into_iter()
            .chain(some_fresh_transactions.iter().cloned().map(|(_, d)| d))
            .collect();
        let transactions = CheckpointContents::new(unprocessed.clone().into_iter());
        let next_checkpoint = cps.get_locals().next_checkpoint;
        let summary = CheckpointSummary::new(
            committee.epoch,
            next_checkpoint,
            &transactions,
            cps.get_prev_checkpoint_digest(next_checkpoint)
                .expect("previous checkpoint should exist"),
        );

        // Cannot register the checkpoint while there are no-executed transactions.
        assert!(cps
            .sign_new_checkpoint(
                committee.epoch,
                summary.sequence_number,
                &transactions,
                summary.previous_digest,
                TestCausalOrderPendCertNoop
            )
            .is_err());

        checkpoint_opt = Some((summary, transactions));

        // Cannot make a checkpoint proposal before adding the unprocessed transactions
        // This returns the old proposal.
        let latest_proposal = cps.set_proposal(committee.epoch).unwrap();
        assert_eq!(*latest_proposal.sequence_number(), next_checkpoint);
        assert_eq!(
            latest_proposal.sequence_number(),
            initial_proposal.sequence_number()
        );
    }
}

// Now check the connection between state / bacth and checkpoint mechanism

#[tokio::test]
async fn test_batch_to_checkpointing() {
    // Create an authority
    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));

    let authority_state = Arc::new(
        AuthorityState::new_for_testing(committee.clone(), &authority_key, None, None, None).await,
    );

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

        authority_state
            .database
            .side_sequence(t1.seq(), &ExecutionDigests::random());
        authority_state
            .database
            .side_sequence(t3.seq(), &ExecutionDigests::random());
        authority_state
            .database
            .side_sequence(t2.seq(), &ExecutionDigests::random());
        authority_state
            .database
            .side_sequence(t0.seq(), &ExecutionDigests::random());
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
    assert_eq!(
        authority_state
            .checkpoints
            .as_ref()
            .unwrap()
            .lock()
            .next_transaction_sequence_expected(),
        4
    );

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

    // Make a test key pair
    let seed = [1u8; 32];
    let (committee, _, authority_key) =
        init_state_parameters_from_rng(&mut StdRng::from_seed(seed));

    // Scope to ensure all variables are dropped
    {
        // TODO: May need to set checkpoint store to be None.
        let authority_state = Arc::new(
            AuthorityState::new_for_testing(
                committee.clone(),
                &authority_key,
                Some(path.clone()),
                None,
                None,
            )
            .await,
        );

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

            authority_state
                .database
                .side_sequence(t1.seq(), &ExecutionDigests::random());
            authority_state
                .database
                .side_sequence(t3.seq(), &ExecutionDigests::random());
            authority_state
                .database
                .side_sequence(t2.seq(), &ExecutionDigests::random());
            authority_state
                .database
                .side_sequence(t0.seq(), &ExecutionDigests::random());
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
        let authority_state = Arc::new(
            AuthorityState::new_for_testing(committee, &authority_key, Some(path), None, None)
                .await,
        );

        // Init feeds the transactions in
        assert_eq!(
            authority_state
                .checkpoints
                .as_ref()
                .unwrap()
                .lock()
                .next_transaction_sequence_expected(),
            4
        );

        // When we close the sending channel we also also end the service task
        authority_state.batch_notifier.close();
    }
}

#[test]
fn set_fragment_external() {
    let (committee, _keys, mut test_objects) = random_ckpoint_store();
    let (test_tx, _rx) = TestConsensus::new();

    let (_, mut cps1) = test_objects.pop().unwrap();
    cps1.set_consensus(Box::new(test_tx.clone()))
        .expect("No issues setting the consensus");
    let (_, mut cps2) = test_objects.pop().unwrap();
    cps2.set_consensus(Box::new(test_tx.clone()))
        .expect("No issues setting the consensus");
    let (_, mut cps3) = test_objects.pop().unwrap();
    cps3.set_consensus(Box::new(test_tx.clone()))
        .expect("No issues setting the consensus");
    let (_, mut cps4) = test_objects.pop().unwrap();
    cps4.set_consensus(Box::new(test_tx))
        .expect("No issues setting the consensus");

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let _p3 = cps3.set_proposal(committee.epoch).unwrap();

    let fragment12 = p1.fragment_with(&p2);
    // let fragment13 = p1.diff_with(&p3);

    // When the fragment concern the authority it processes it
    assert!(cps1
        .submit_local_fragment_to_consensus(&fragment12, &committee)
        .is_ok());
    assert!(cps2
        .submit_local_fragment_to_consensus(&fragment12, &committee)
        .is_ok());

    // When the fragment does not concern the authority it does not process it.
    assert!(cps3
        .submit_local_fragment_to_consensus(&fragment12, &committee)
        .is_err());
}

#[test]
fn set_fragment_reconstruct() {
    let (committee, _keys, mut test_objects) = random_ckpoint_store();
    let (_, mut cps1) = test_objects.pop().unwrap();
    let (_, mut cps2) = test_objects.pop().unwrap();
    let (_, mut cps3) = test_objects.pop().unwrap();
    let (_, mut cps4) = test_objects.pop().unwrap();

    let t1 = ExecutionDigests::random();
    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    let t4 = ExecutionDigests::random();
    let t5 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    cps1.update_processed_transactions(&[(1, t2), (2, t3)])
        .unwrap();

    cps2.update_processed_transactions(&[(1, t1), (2, t2)])
        .unwrap();

    cps3.update_processed_transactions(&[(1, t3), (2, t4)])
        .unwrap();

    cps4.update_processed_transactions(&[(1, t4), (2, t5)])
        .unwrap();

    let p1 = cps1.set_proposal(committee.epoch).unwrap();
    let p2 = cps2.set_proposal(committee.epoch).unwrap();
    let p3 = cps3.set_proposal(committee.epoch).unwrap();
    let p4 = cps4.set_proposal(committee.epoch).unwrap();

    let fragment12 = p1.fragment_with(&p2);
    let fragment34 = p3.fragment_with(&p4);

    let attempt1 = FragmentReconstruction::construct(
        0,
        committee.clone(),
        &[fragment12.clone(), fragment34.clone()],
    );
    assert!(matches!(attempt1, Err(_)));

    let fragment41 = p4.fragment_with(&p1);
    let attempt2 =
        FragmentReconstruction::construct(0, committee, &[fragment12, fragment34, fragment41]);
    assert!(attempt2.is_ok());

    let reconstruction = attempt2.unwrap();
    assert_eq!(reconstruction.global.authority_waypoints.len(), 4);
}

#[test]
fn set_fragment_reconstruct_two_components() {
    let (committee, _keys, mut test_objects) = random_ckpoint_store_num(2 * 3 + 1);

    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    for (_, cps) in &mut test_objects {
        cps.update_processed_transactions(&[(1, t2), (2, t3)])
            .unwrap();
    }

    let mut proposals: Vec<_> = test_objects
        .iter_mut()
        .map(|(_, cps)| cps.set_proposal(committee.epoch).unwrap())
        .collect();

    // Get out the last two
    let p_x = proposals.pop().unwrap();
    let p_y = proposals.pop().unwrap();

    let fragment_xy = p_x.fragment_with(&p_y);

    let attempt1 = FragmentReconstruction::construct(0, committee.clone(), &[fragment_xy.clone()]);
    assert!(matches!(attempt1, Err(_)));

    // Make a daisy chain of the other proposals
    let mut fragments = vec![fragment_xy];

    while let Some(proposal) = proposals.pop() {
        if !proposals.is_empty() {
            let fragment_xy = proposal.fragment_with(&proposals[0]);
            fragments.push(fragment_xy);
        }

        if proposals.len() == 1 {
            break;
        }

        let attempt2 = FragmentReconstruction::construct(0, committee.clone(), &fragments);
        // Error until we have the full 5 others
        assert!(matches!(attempt2, Err(_)));
    }

    let attempt2 = FragmentReconstruction::construct(0, committee, &fragments);
    assert!(attempt2.is_ok());

    let reconstruction = attempt2.unwrap();
    assert_eq!(reconstruction.global.authority_waypoints.len(), 5);
}

#[test]
fn set_fragment_reconstruct_two_mutual() {
    let (committee, _, mut test_objects) = random_ckpoint_store_num(4);

    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();

    for (_, cps) in &mut test_objects {
        cps.update_processed_transactions(&[(1, t2), (2, t3)])
            .unwrap();
    }

    let mut proposals: Vec<_> = test_objects
        .iter_mut()
        .map(|(_, cps)| cps.set_proposal(committee.epoch).unwrap())
        .collect();

    // Get out the last two
    let p_x = proposals.pop().unwrap();
    let p_y = proposals.pop().unwrap();

    let fragment_xy = p_x.fragment_with(&p_y);
    let fragment_yx = p_y.fragment_with(&p_x);

    let attempt1 = FragmentReconstruction::construct(0, committee, &[fragment_xy, fragment_yx]);
    assert!(matches!(attempt1, Err(_)));
}

#[derive(Clone)]
struct TestConsensus {
    sender: Arc<std::sync::Mutex<std::sync::mpsc::Sender<CheckpointFragment>>>,
}

impl ConsensusSender for TestConsensus {
    fn send_to_consensus(&self, fragment: CheckpointFragment) -> Result<(), SuiError> {
        self.sender
            .lock()
            .expect("Locking failed")
            .send(fragment)
            .expect("Failed to send");
        Ok(())
    }
}

impl TestConsensus {
    pub fn new() -> (TestConsensus, std::sync::mpsc::Receiver<CheckpointFragment>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            TestConsensus {
                sender: Arc::new(std::sync::Mutex::new(tx)),
            },
            rx,
        )
    }
}

#[test]
fn test_fragment_full_flow() {
    let (committee, _keys, mut test_objects) = random_ckpoint_store_num(2 * 3 + 1);

    let (test_tx, rx) = TestConsensus::new();

    let t2 = ExecutionDigests::random();
    let t3 = ExecutionDigests::random();
    // let t6 = TransactionDigest::random();

    for (_, cps) in &mut test_objects {
        cps.set_consensus(Box::new(test_tx.clone()))
            .expect("No issues setting the consensus");
        cps.update_processed_transactions(&[(1, t2), (2, t3)])
            .unwrap();
    }

    let mut proposals: Vec<_> = test_objects
        .iter_mut()
        .map(|(_, cps)| cps.set_proposal(committee.epoch).unwrap())
        .collect();

    // Get out the last two
    let p_x = proposals.pop().unwrap();
    let p_y = proposals.pop().unwrap();

    let fragment_xy = p_x.fragment_with(&p_y);

    // TEST 1 -- submitting a fragment not involving a validator gets rejected by the
    //           validator.

    // Validator 3 is not validator 5 or 6
    assert!(test_objects[3]
        .1
        .submit_local_fragment_to_consensus(&fragment_xy, &committee)
        .is_err());
    // Nothing is sent to consensus
    assert!(rx.try_recv().is_err());

    // But accept it on both the 5 and 6
    assert!(test_objects[5]
        .1
        .submit_local_fragment_to_consensus(&fragment_xy, &committee)
        .is_ok());
    assert!(test_objects[6]
        .1
        .submit_local_fragment_to_consensus(&fragment_xy, &committee)
        .is_ok());

    // Check we registered one local fragment
    assert_eq!(test_objects[5].1.tables.local_fragments.iter().count(), 1);

    // Make a daisy chain of the other proposals
    let mut fragments = vec![fragment_xy];

    while let Some(proposal) = proposals.pop() {
        if !proposals.is_empty() {
            let fragment_xy = proposal.fragment_with(&proposals[proposals.len() - 1]);
            assert!(test_objects[proposals.len() - 1]
                .1
                .submit_local_fragment_to_consensus(&fragment_xy, &committee)
                .is_ok());
            fragments.push(fragment_xy);
        }

        if proposals.len() == 1 {
            break;
        }
    }

    // TEST 2 -- submit fragments to all validators, and construct checkpoint.

    let mut seq = ExecutionIndices::default();
    let cps0 = &mut test_objects[0].1;
    let mut all_fragments = Vec::new();
    while let Ok(fragment) = rx.try_recv() {
        all_fragments.push(fragment.clone());
        assert!(cps0
            .handle_internal_fragment(
                seq.clone(),
                fragment,
                &committee,
                PendCertificateForExecutionNoop
            )
            .is_ok());
        seq.next(
            /* total_batches */ 100, /* total_transactions */ 100,
        );
    }
    cps0.attempt_to_construct_checkpoint(TestCausalOrderPendCertNoop, &committee)
        .unwrap();

    // Two fragments for 5-6, and then 0-1, 1-2, 2-3, 3-4
    assert_eq!(seq.next_transaction_index, 6);
    // We don't update next checkpoint yet until we get a cert.
    assert_eq!(cps0.next_checkpoint(), 0);

    let response = cps0
        .handle_authenticated_checkpoint(&Some(0), true)
        .expect("No errors on response");
    // Ensure the reconstruction worked
    assert_eq!(response.detail.unwrap().transactions.len(), 2);

    // TEST 3 -- feed the framents to the node 6 which cannot decode the
    // sequence of fragments.

    let mut seq = ExecutionIndices::default();
    let cps6 = &mut test_objects[6].1;
    for fragment in &all_fragments {
        let _ = cps6.handle_internal_fragment(
            seq.clone(),
            fragment.clone(),
            &committee,
            PendCertificateForExecutionNoop,
        );
        seq.next(
            /* total_batches */ 100, /* total_transactions */ 100,
        );
    }

    // Two fragments for 5-6, and then 0-1, 1-2, 2-3, 3-4
    assert_eq!(cps6.tables.fragments.iter().count(), 6);
    // Cannot advance to next checkpoint
    assert!(cps6.latest_stored_checkpoint().is_none());
    // But recording of fragments is closed

    // However recording has stopped
    // and no more fragments are recorded.

    for fragment in &all_fragments {
        let _ = cps6.handle_internal_fragment(
            seq.clone(),
            fragment.clone(),
            &committee,
            PendCertificateForExecutionNoop,
        );
        seq.next(
            /* total_batches */ 100, /* total_transactions */ 100,
        );
    }

    // Two fragments for 5-6, and then 0-1, 1-2, 2-3, 3-4
    assert_eq!(cps6.tables.fragments.iter().count(), 12);
    // Cannot advance to next checkpoint
    assert_eq!(cps6.next_checkpoint(), 0);
    // But recording of fragments is closed
}

#[derive(Clone)]
struct AsyncTestConsensus {
    sender: Arc<std::sync::Mutex<tokio::sync::mpsc::UnboundedSender<CheckpointFragment>>>,
}

impl ConsensusSender for AsyncTestConsensus {
    fn send_to_consensus(&self, fragment: CheckpointFragment) -> Result<(), SuiError> {
        self.sender
            .lock()
            .expect("Locking failed")
            .send(fragment)
            .expect("Failed to send");
        Ok(())
    }
}

#[allow(clippy::disallowed_methods)]
impl AsyncTestConsensus {
    pub fn new() -> (
        AsyncTestConsensus,
        tokio::sync::mpsc::UnboundedReceiver<CheckpointFragment>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            AsyncTestConsensus {
                sender: Arc::new(std::sync::Mutex::new(tx)),
            },
            rx,
        )
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct TestAuthority {
    pub store: Arc<AuthorityStore>,
    pub authority: Arc<AuthorityState>,
    pub checkpoint: Arc<Mutex<CheckpointStore>>,
}

#[allow(dead_code)]
pub struct TestSetup {
    pub committee: Committee,
    pub authorities: Vec<TestAuthority>,
    pub transactions: Vec<sui_types::messages::Transaction>,
    pub aggregator: AuthorityAggregator<LocalAuthorityClient>,
}

impl TestSetup {
    pub fn get_authority(&self, name: &AuthorityName) -> &TestAuthority {
        self.authorities
            .iter()
            .find(|t| &t.authority.name == name)
            .unwrap()
    }
}

// TODO use the file name as a seed
const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

pub async fn checkpoint_tests_setup(
    num_objects: usize,
    batch_interval: Duration,
    notify_noop: bool,
) -> TestSetup {
    let mut rng = StdRng::from_seed(RNG_SEED);
    let (keys, committee) = make_committee_key(&mut rng);

    let mut genesis_objects = Vec::new();
    let mut transactions = Vec::new();

    // Generate a large number of objects for testing
    for _i in 0..num_objects {
        let (addr1, key1): (_, AccountKeyPair) = get_key_pair_from_rng(&mut rng);
        let (addr2, _): (_, AccountKeyPair) = get_key_pair_from_rng(&mut rng);
        let gas_object1 = Object::with_owner_for_testing(addr1);
        let gas_object2 = Object::with_owner_for_testing(addr1);

        let tx = transfer_coin_transaction(
            addr1,
            &key1,
            addr2,
            gas_object1.compute_object_reference(),
            gas_object2.compute_object_reference(),
        );

        genesis_objects.push(gas_object1);
        genesis_objects.push(gas_object2);
        transactions.push(tx);
    }

    let genesis_objects_ref: Vec<_> = genesis_objects.iter().collect();

    // Set the fake consensus channel
    let (sender, mut _rx) = AsyncTestConsensus::new();

    let mut authorities = Vec::new();

    // Make all authorities and their services.
    for k in &keys {
        let authority = Arc::new(
            AuthorityState::new_for_testing(
                committee.clone(),
                k,
                None,
                None,
                Some(Box::new(sender.clone())),
            )
            .await,
        );

        // Add objects for testing
        authority
            .insert_genesis_objects_bulk_unsafe(&genesis_objects_ref[..])
            .await;

        let inner_state = authority.clone();
        let _join =
            tokio::task::spawn(
                async move { inner_state.run_batch_service(1000, batch_interval).await },
            );

        let checkpoint = authority.checkpoints.as_ref().unwrap().clone();
        authorities.push(TestAuthority {
            store: authority.database.clone(),
            authority,
            checkpoint,
        });
    }

    // The fake consensus channel for testing
    let checkpoint_stores: Vec<_> = authorities
        .iter()
        .map(|a| (a.authority.clone(), a.checkpoint.clone()))
        .collect();
    let c = committee.clone();
    let _join = tokio::task::spawn(async move {
        let mut seq = ExecutionIndices::default();
        while let Some(msg) = _rx.recv().await {
            println!("Deliver fragment seq={:?}", seq);
            for (authority, cps) in &checkpoint_stores {
                if notify_noop {
                    if let Err(err) = cps.lock().handle_internal_fragment(
                        seq.clone(),
                        msg.clone(),
                        &c,
                        PendCertificateForExecutionNoop,
                    ) {
                        println!("Error: {:?}", err);
                    }
                } else if let Err(err) = cps.lock().handle_internal_fragment(
                    seq.clone(),
                    msg.clone(),
                    &c,
                    authority.database.clone(),
                ) {
                    println!("Error: {:?}", err);
                }
            }
            seq.next(
                /* total_batches */ 100, /* total_transactions */ 100,
            );
        }
        println!("CHANNEL EXIT.");
    });

    // Now make an authority aggregator
    let aggregator = AuthorityAggregator::new(
        committee.clone(),
        Arc::new(EpochStore::new_for_testing(&committee)),
        authorities
            .iter()
            .map(|a| {
                (
                    a.authority.name,
                    LocalAuthorityClient::new_from_authority(a.authority.clone()),
                )
            })
            .collect(),
        AuthAggMetrics::new_for_tests(),
        SafeClientMetrics::new_for_tests(),
    );

    TestSetup {
        committee,
        authorities,
        transactions,
        aggregator,
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_messaging_flow_bug() {
    let mut setup = checkpoint_tests_setup(5, Duration::from_millis(500), true).await;

    // Check that the system is running.
    let t = setup.transactions.pop().unwrap();
    let (_cert, _effects) = setup
        .aggregator
        .execute_transaction(&t)
        .await
        .expect("All ok.");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn checkpoint_messaging_flow() {
    let mut setup = checkpoint_tests_setup(5, Duration::from_millis(500), true).await;
    let metrics = CheckpointMetrics::new_for_tests();

    // Check that the system is running.
    let t = setup.transactions.pop().unwrap();
    let (_cert, effects) = setup
        .aggregator
        .execute_transaction(&t)
        .await
        .expect("All ok.");

    // Check whether this is a success?
    assert!(matches!(
        effects.effects().status,
        ExecutionStatus::Success { .. }
    ));

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Happy path checkpoint flow

    // Step 1 -- get a bunch of proposals
    let mut proposals = Vec::new();
    // First make sure each authority creates a proposal.
    for auth in &setup.authorities {
        auth.checkpoint
            .lock()
            .set_proposal(setup.committee.epoch)
            .unwrap();
    }

    for (auth, client) in &setup.aggregator.authority_clients {
        let response = client
            .handle_checkpoint(CheckpointRequest::proposal(true))
            .await
            .expect("No issues");

        assert!(matches!(
            response.info,
            AuthorityCheckpointInfo::CheckpointProposal { .. }
        ));

        if let AuthorityCheckpointInfo::CheckpointProposal { proposal, .. } = &response.info {
            assert!(proposal.is_some());

            proposals.push((
                *auth,
                CheckpointProposal::new_from_signed_proposal_summary(
                    proposal.as_ref().unwrap().clone(),
                    response.detail.unwrap(),
                ),
            ));
        }
    }

    // Step 2 -- make fragments using the proposals.
    let proposal_len = proposals.len();
    for (i, (auth, proposal)) in proposals.iter().enumerate() {
        let p0 = proposal.fragment_with(&proposals[(i + 1) % proposal_len].1);
        let p1 = proposal.fragment_with(&proposals[(i + 3) % proposal_len].1);

        let authority = setup.get_authority(auth);
        authority
            .checkpoint
            .lock()
            .submit_local_fragment_to_consensus(&p0, &setup.committee)
            .unwrap();
        authority
            .checkpoint
            .lock()
            .submit_local_fragment_to_consensus(&p1, &setup.committee)
            .unwrap();
    }

    // Give time to the receiving task to process (so that consensus can sequence fragments).
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Now make sure each authority makes some progress to create a checkpoint.
    for auth in &setup.authorities {
        auth.checkpoint
            .lock()
            .attempt_to_construct_checkpoint(TestCausalOrderPendCertNoop, &setup.committee)
            .unwrap();
    }

    // Note that some will be having a signed checkpoint and some will not
    // because they were not included in the first two links that make a checkpoint.

    // Step 3 - get the signed checkpoint
    let mut signed_checkpoint = Vec::new();
    let mut contents = None;
    let mut failed_authorities = HashSet::new();
    for (auth, client) in &setup.aggregator.authority_clients {
        let response = client
            .handle_checkpoint(CheckpointRequest::authenticated(Some(0), true))
            .await
            .expect("No issues");

        match &response.info {
            AuthorityCheckpointInfo::AuthenticatedCheckpoint(Some(
                AuthenticatedCheckpoint::Signed(checkpoint),
            )) => {
                signed_checkpoint.push(checkpoint.clone());
                contents = response.detail.clone();
            }
            _ => {
                failed_authorities.insert(*auth);
            }
        }
    }

    let contents = contents.unwrap();
    assert_eq!(contents.transactions.len(), 1);

    // Construct a certificate
    // We need at least f+1 signatures
    assert!(signed_checkpoint.len() > 1);
    let checkpoint_cert =
        CertifiedCheckpointSummary::aggregate(signed_checkpoint, &setup.committee.clone())
            .expect("all ok");

    // Step 4 -- Upload the certificate back up.
    for auth in &setup.authorities {
        if failed_authorities.contains(&auth.authority.name) {
            auth.checkpoint
                .lock()
                .process_new_checkpoint_certificate(
                    &checkpoint_cert,
                    &contents,
                    &setup.committee,
                    TestCausalOrderPendCertNoop,
                    &metrics,
                )
                .unwrap();
        } else {
            auth.checkpoint
                .lock()
                .promote_signed_checkpoint_to_cert(&checkpoint_cert, &setup.committee, &metrics)
                .unwrap();
        }
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_no_more_fragments() {
    let mut setup = checkpoint_tests_setup(5, Duration::from_millis(500), true).await;

    // Check that the system is running.
    let t = setup.transactions.pop().unwrap();
    let (_cert, effects) = setup
        .aggregator
        .execute_transaction(&t)
        .await
        .expect("All ok.");

    // Check whether this is a success?
    assert!(matches!(
        effects.effects().status,
        ExecutionStatus::Success { .. }
    ));

    // Wait for a batch to go through
    // (We do not really wait, we jump there since real-time is not running).
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Happy path checkpoint flow

    // Step 1 -- get a bunch of proposals
    let mut proposals = Vec::new();
    // First make sure each authority creates a proposal.
    for auth in &setup.authorities {
        let proposal = auth
            .checkpoint
            .lock()
            .set_proposal(setup.committee.epoch)
            .unwrap();
        proposals.push(proposal);
    }

    let p3 = proposals.pop().unwrap();
    let p2 = proposals.pop().unwrap();
    let p1 = proposals.pop().unwrap();
    let p0 = proposals.pop().unwrap();

    let f01 = p0.fragment_with(&p1);
    let f02 = p0.fragment_with(&p2);
    let f03 = p0.fragment_with(&p3);

    // put in fragment 0-1 and no checkpoint can be formed

    setup.authorities[0]
        .checkpoint
        .lock()
        .submit_local_fragment_to_consensus(&f01, &setup.committee)
        .unwrap();

    // Give time to the receiving task to process (so that consensus can sequence fragments).
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Expecting more fragments
    assert!(
        !setup.authorities[0]
            .checkpoint
            .lock()
            .get_locals()
            .no_more_fragments
    );

    // put in fragment 0-2, now node 0 can form a checkpoint but not node 3

    setup.authorities[0]
        .checkpoint
        .lock()
        .submit_local_fragment_to_consensus(&f02, &setup.committee)
        .unwrap();

    // Give time to the receiving task to process (so that consensus can sequence fragments).
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert!(setup.authorities[0]
        .checkpoint
        .lock()
        .attempt_to_construct_checkpoint(TestCausalOrderPendCertNoop, &setup.committee)
        .is_ok());

    // Expecting more fragments
    assert!(
        !setup.authorities[0]
            .checkpoint
            .lock()
            .get_locals()
            .no_more_fragments
    );

    // node 3 cannot make one
    assert!(setup.authorities[3]
        .checkpoint
        .lock()
        .attempt_to_construct_checkpoint(TestCausalOrderPendCertNoop, &setup.committee)
        .is_err());

    // Expecting more fragments
    assert!(
        setup.authorities[3]
            .checkpoint
            .lock()
            .get_locals()
            .no_more_fragments
    );

    // Now fie node 3 a link and it can make the checkpoint
    setup.authorities[3]
        .checkpoint
        .lock()
        .submit_local_fragment_to_consensus(&f03, &setup.committee)
        .unwrap();

    assert!(setup.authorities[3]
        .checkpoint
        .lock()
        .attempt_to_construct_checkpoint(TestCausalOrderPendCertNoop, &setup.committee)
        .is_ok());
}

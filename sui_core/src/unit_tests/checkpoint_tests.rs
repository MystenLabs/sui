// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::max_files_authority_tests;
use rand::Rng;
use std::{collections::HashSet, env, fs, path::PathBuf, sync::Arc};
use sui_types::{
    base_types::{AuthorityName, ObjectID},
    utils::make_committee_key,
    waypoint::GlobalCheckpoint,
};

fn random_authority_name() -> AuthorityName {
    let mut rng = rand::thread_rng();
    AuthorityName::try_from(&rng.gen::<[u8; 32]>()[..]).expect("all ok")
}

fn random_ckpoint_store() -> (PathBuf, CheckpointStore) {
    let (keys, committee) = make_committee_key();

    let dir = env::temp_dir();
    let path = dir.join(format!("SC_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());

    let cps = CheckpointStore::open(
        path.clone(),
        Some(opts),
        *keys[0].public_key_bytes(),
        committee,
        Arc::pin(keys[0].copy()),
    );
    (path, cps)
}

#[test]
fn make_checkpoint_db() {
    let (_, mut cps) = random_ckpoint_store();

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

    assert!(cps.next_checkpoint_sequence() == 0);

    cps.update_new_checkpoint(0, &[t1, t2, t4, t5]).unwrap();
    assert!(cps.checkpoint_contents.iter().count() == 4);
    assert_eq!(cps.extra_transactions.iter().count(), 1);
    assert!(cps.unprocessed_transactions.iter().count() == 2);

    assert_eq!(cps.lowest_unprocessed_sequence(), 0);

    let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
    assert!(tx_seq >= u64::MAX / 2);

    assert!(cps.next_checkpoint_sequence() == 1);

    cps.update_processed_transactions(&[(4, t4), (5, t5), (6, t6)])
        .unwrap();
    assert!(cps.checkpoint_contents.iter().count() == 4);
    assert_eq!(cps.extra_transactions.iter().count(), 2); // t3 & t6
    assert!(cps.unprocessed_transactions.iter().count() == 0);

    assert_eq!(cps.lowest_unprocessed_sequence(), 1);

    let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
    assert_eq!(tx_seq, 4);
}

#[test]
fn make_proposals() {
    let (_, mut cps1) = random_ckpoint_store();
    let (_, mut cps2) = random_ckpoint_store();
    let (_, mut cps3) = random_ckpoint_store();
    let (_, mut cps4) = random_ckpoint_store();

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

    let authority1 = random_authority_name();
    let authority2 = random_authority_name();
    let authority3 = random_authority_name();

    let p1 = cps1.set_proposal(authority1).unwrap();
    let p2 = cps2.set_proposal(authority2).unwrap();
    let p3 = cps3.set_proposal(authority3).unwrap();

    let ckp_items: Vec<_> = p1
        .transactions
        .into_iter()
        .chain(p2.transactions.into_iter())
        .chain(p3.transactions.into_iter())
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
    let (_, mut cps1) = random_ckpoint_store();
    let (_, mut cps2) = random_ckpoint_store();
    let (_, mut cps3) = random_ckpoint_store();
    let (_, mut cps4) = random_ckpoint_store();

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

    let authority1 = random_authority_name();
    let authority2 = random_authority_name();
    let authority3 = random_authority_name();
    let authority4 = random_authority_name();

    let p1 = cps1.set_proposal(authority1).unwrap();
    let p2 = cps2.set_proposal(authority2).unwrap();
    let p3 = cps3.set_proposal(authority3).unwrap();
    let p4 = cps4.set_proposal(authority4).unwrap();

    let diff12 = p1.diff_with(&p2);
    let diff23 = p2.diff_with(&p3);

    let mut global = GlobalCheckpoint::<AuthorityName, TransactionDigest>::new(0);
    global.insert(diff12.clone()).unwrap();
    global.insert(diff23).unwrap();

    // P4 proposal not selected
    let diff41 = p4.diff_with(&p1);
    let all_items4 = global
        .checkpoint_items(diff41, p4.transactions.iter().cloned().collect())
        .unwrap();

    // P1 proposal selected
    let all_items1 = global
        .checkpoint_items(diff12, p1.transactions.iter().cloned().collect())
        .unwrap();

    // All get the same set for the proposal
    assert_eq!(all_items1, all_items4);
}

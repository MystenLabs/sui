// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::*;

use std::fs;
use std::{convert::TryInto, env};

#[tokio::test]
async fn test_open_manager() {
    // let (_, authority_key) = get_key_pair();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        // TEST 1: init from an empty database should return to a zero block
        let (_send, manager) = BatcherManager::new(store.clone(), 100);
        let last_block = manager
            .init_from_database()
            .await
            .expect("No error expected.");

        assert_eq!(0, last_block.total_size);

        // TEST 2: init from a db with a transaction not in the sequence makes a new block
        //         when we re-open the database.

        store
            .executed_sequence
            .insert(&0, &TransactionDigest::new([0; 32].try_into().unwrap()))
            .expect("no error on write");
    }
    // drop all
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        let (_send, manager) = BatcherManager::new(store.clone(), 100);
        let last_block = manager
            .init_from_database()
            .await
            .expect("No error expected.");

        assert_eq!(1, last_block.total_size);

        // TEST 3: If the database contains out of order transactions return an error
        store
            .executed_sequence
            .insert(&2, &TransactionDigest::new([0; 32].try_into().unwrap()))
            .expect("no error on write");
    }
    // drop all
    {
        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(&path, Some(opts)));

        let (_send, manager) = BatcherManager::new(store.clone(), 100);
        let last_block = manager.init_from_database().await;

        assert_eq!(last_block, Err(SuiError::StorageCorrupt));
    }
}

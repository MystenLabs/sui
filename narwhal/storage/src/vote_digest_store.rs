// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::NodeStorage;
use crypto::PublicKey;
use store::reopen;
use store::rocks::{open_cf, MetricConf, ReadWriteOptions};
use store::{rocks::DBMap, Map, TypedStoreError};
use types::{Vote, VoteInfo};

/// The storage for the last votes digests per authority
#[derive(Clone)]
pub struct VoteDigestStore {
    store: DBMap<PublicKey, VoteInfo>,
}

impl VoteDigestStore {
    pub fn new(vote_digest_store: DBMap<PublicKey, VoteInfo>) -> VoteDigestStore {
        Self {
            store: vote_digest_store,
        }
    }

    pub fn new_for_tests() -> VoteDigestStore {
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[NodeStorage::VOTES_CF],
        )
        .expect("Cannot open database");
        let map = reopen!(&rocksdb, NodeStorage::VOTES_CF;<PublicKey, VoteInfo>);
        VoteDigestStore::new(map)
    }

    /// Insert the vote's basic details into the database for the corresponding
    /// header author key.
    pub fn write(&self, vote: &Vote) -> Result<(), TypedStoreError> {
        self.store.insert(&vote.origin, &vote.into())
    }

    /// Read the vote info based on the provided corresponding header author key
    pub fn read(&self, header_author: &PublicKey) -> Result<Option<VoteInfo>, TypedStoreError> {
        self.store.get(header_author)
    }
}

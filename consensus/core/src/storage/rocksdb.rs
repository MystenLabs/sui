// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{block::VerifiedBlock, commit::Commit, error::ConsensusResult};

use super::Store;

/// Storage implementation using RocksDB.
pub(crate) struct RocksDB {}

#[allow(unused)]
impl Store for RocksDB {
    fn recover(&self) -> ConsensusResult<(Vec<VerifiedBlock>, Commit)> {
        unimplemented!()
    }

    fn write(&self, blocks: Vec<VerifiedBlock>, commits: Vec<Commit>) -> ConsensusResult<()> {
        unimplemented!()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod rocksdb;

use crate::{block::VerifiedBlock, commit::Commit, error::ConsensusResult};

/// A common interface for consensus storage.
pub(crate) trait Store {
    /// Loads last committed blocks, all uncommitted blocks and last commit from store.
    fn recover(&self) -> ConsensusResult<(Vec<VerifiedBlock>, Commit)>;

    /// Writes blocks and consensus commits to store.
    fn write(&self, blocks: Vec<VerifiedBlock>, commits: Vec<Commit>) -> ConsensusResult<()>;

    // TODO: add methods to read and scan blocks.
}

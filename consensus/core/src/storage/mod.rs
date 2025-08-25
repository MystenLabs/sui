// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod mem_store;
pub mod rocksdb_store;

#[cfg(test)]
mod store_tests;

use std::collections::BTreeMap;

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round, TransactionIndex};

use crate::{
    block::VerifiedBlock,
    commit::{CommitInfo, CommitRange, CommitRef, TrustedCommit},
    error::ConsensusResult,
    CommitIndex,
};

/// A common interface for consensus storage.
pub trait Store: Send + Sync {
    /// Writes blocks, consensus commits and other data to store atomically.
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()>;

    /// Reads blocks for the given refs.
    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>>;

    /// Checks if blocks exist in the store.
    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>>;

    /// Reads blocks for an authority, from start_round.
    fn scan_blocks_by_author(
        &self,
        authority: AuthorityIndex,
        start_round: Round,
    ) -> ConsensusResult<Vec<VerifiedBlock>>;

    // The method returns the last `num_of_rounds` rounds blocks by author in round ascending order.
    // When a `before_round` is defined then the blocks of round `<=before_round` are returned. If not
    // then the max value for round will be used as cut off.
    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
        before_round: Option<Round>,
    ) -> ConsensusResult<Vec<VerifiedBlock>>;

    /// Reads the last commit.
    fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>>;

    /// Reads all commits from start (inclusive) until end (inclusive).
    fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>>;

    /// Reads all blocks voting on a particular commit.
    fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>>;

    /// Reads the last commit info, written atomically with the last commit.
    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>>;

    /// Reads the last finalized commit.
    fn read_last_finalized_commit(&self) -> ConsensusResult<Option<CommitRef>>;

    // Scans finalized commits with their rejected transactions.
    fn scan_finalized_commits(
        &self,
        range: CommitRange,
    ) -> ConsensusResult<Vec<(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)>>;
}

/// Represents data to be written to the store together atomically.
#[derive(Debug, Default)]
pub struct WriteBatch {
    pub blocks: Vec<VerifiedBlock>,
    pub commits: Vec<TrustedCommit>,
    pub commit_info: Vec<(CommitRef, CommitInfo)>,
    pub finalized_commits: Vec<(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)>,
}

impl WriteBatch {
    pub fn new(
        blocks: Vec<VerifiedBlock>,
        commits: Vec<TrustedCommit>,
        commit_info: Vec<(CommitRef, CommitInfo)>,
        finalized_commits: Vec<(CommitRef, BTreeMap<BlockRef, Vec<TransactionIndex>>)>,
    ) -> Self {
        WriteBatch {
            blocks,
            commits,
            commit_info,
            finalized_commits,
        }
    }

    // Test setters.

    #[cfg(test)]
    pub fn blocks(mut self, blocks: Vec<VerifiedBlock>) -> Self {
        self.blocks = blocks;
        self
    }

    #[cfg(test)]
    pub fn commits(mut self, commits: Vec<TrustedCommit>) -> Self {
        self.commits = commits;
        self
    }

    #[cfg(test)]
    pub fn commit_info(mut self, commit_info: Vec<(CommitRef, CommitInfo)>) -> Self {
        self.commit_info = commit_info;
        self
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod mem_store;
pub(crate) mod rocksdb_store;

#[cfg(test)]
mod store_tests;

use consensus_config::AuthorityIndex;

use crate::{
    block::{BlockRef, Round, Slot, VerifiedBlock},
    commit::{CommitInfo, CommitRange, TrustedCommit},
    error::ConsensusResult,
};

/// A common interface for consensus storage.
pub(crate) trait Store: Send + Sync {
    /// Writes blocks, consensus commits and other data to store atomically.
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()>;

    /// Reads blocks for the given refs.
    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>>;

    /// Checks if blocks exist in the store.
    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>>;

    /// Checks whether there is any block at the given slot
    fn contains_block_at_slot(&self, slot: Slot) -> ConsensusResult<bool>;

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

    /// Reads all commits from start (inclusive) until end (exclusive).
    fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>>;

    /// Reads the last commit info, including last committed round per authority.
    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRange, CommitInfo)>>;
}

/// Represents data to be written to the store together atomically.
#[derive(Debug, Default)]
pub(crate) struct WriteBatch {
    pub(crate) blocks: Vec<VerifiedBlock>,
    pub(crate) commits: Vec<TrustedCommit>,
    pub(crate) commit_ranges_with_commit_info: Vec<(CommitRange, CommitInfo)>,
}

impl WriteBatch {
    pub(crate) fn new(
        blocks: Vec<VerifiedBlock>,
        commits: Vec<TrustedCommit>,
        commit_ranges_with_commit_info: Vec<(CommitRange, CommitInfo)>,
    ) -> Self {
        WriteBatch {
            blocks,
            commits,
            commit_ranges_with_commit_info,
        }
    }

    // Test setters.

    #[cfg(test)]
    pub(crate) fn blocks(mut self, blocks: Vec<VerifiedBlock>) -> Self {
        self.blocks = blocks;
        self
    }

    #[cfg(test)]
    pub(crate) fn commits(mut self, commits: Vec<TrustedCommit>) -> Self {
        self.commits = commits;
        self
    }

    #[cfg(test)]
    pub(crate) fn commit_ranges_with_commit_info(
        mut self,
        commit_ranges_with_commit_info: Vec<(CommitRange, CommitInfo)>,
    ) -> Self {
        self.commit_ranges_with_commit_info = commit_ranges_with_commit_info;
        self
    }
}

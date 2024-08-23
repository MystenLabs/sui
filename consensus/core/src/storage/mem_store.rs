// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ops::Bound::Included,
};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use super::{Store, WriteBatch};
use crate::{
    block::{BlockAPI as _, BlockDigest, BlockRef, Round, Slot, VerifiedBlock},
    commit::{
        CommitAPI as _, CommitDigest, CommitIndex, CommitInfo, CommitRange, CommitRef,
        TrustedCommit,
    },
    error::ConsensusResult,
};

/// In-memory storage for testing.
#[allow(unused)]
pub(crate) struct MemStore {
    inner: RwLock<Inner>,
}

#[allow(unused)]
struct Inner {
    blocks: BTreeMap<(Round, AuthorityIndex, BlockDigest), VerifiedBlock>,
    digests_by_authorities: BTreeSet<(AuthorityIndex, Round, BlockDigest)>,
    commits: BTreeMap<(CommitIndex, CommitDigest), TrustedCommit>,
    commit_votes: BTreeSet<(CommitIndex, CommitDigest, BlockRef)>,
    commit_info: BTreeMap<(CommitIndex, CommitDigest), CommitInfo>,
}

impl MemStore {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        MemStore {
            inner: RwLock::new(Inner {
                blocks: BTreeMap::new(),
                digests_by_authorities: BTreeSet::new(),
                commits: BTreeMap::new(),
                commit_votes: BTreeSet::new(),
                commit_info: BTreeMap::new(),
            }),
        }
    }
}

impl Store for MemStore {
    fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()> {
        let mut inner = self.inner.write();

        for block in write_batch.blocks {
            let block_ref = block.reference();
            inner.blocks.insert(
                (block_ref.round, block_ref.author, block_ref.digest),
                block.clone(),
            );
            inner.digests_by_authorities.insert((
                block_ref.author,
                block_ref.round,
                block_ref.digest,
            ));
            for vote in block.commit_votes() {
                inner
                    .commit_votes
                    .insert((vote.index, vote.digest, block_ref));
            }
        }

        for commit in write_batch.commits {
            inner
                .commits
                .insert((commit.index(), commit.digest()), commit);
        }

        for (commit_ref, commit_info) in write_batch.commit_info {
            inner
                .commit_info
                .insert((commit_ref.index, commit_ref.digest), commit_info);
        }

        Ok(())
    }

    fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
        let inner = self.inner.read();
        let blocks = refs
            .iter()
            .map(|r| inner.blocks.get(&(r.round, r.author, r.digest)).cloned())
            .collect();
        Ok(blocks)
    }

    fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>> {
        let inner = self.inner.read();
        let exist = refs
            .iter()
            .map(|r| inner.blocks.contains_key(&(r.round, r.author, r.digest)))
            .collect();
        Ok(exist)
    }

    fn scan_blocks_by_author(
        &self,
        author: AuthorityIndex,
        start_round: Round,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let inner = self.inner.read();
        let mut refs = vec![];
        for &(author, round, digest) in inner.digests_by_authorities.range((
            Included((author, start_round, BlockDigest::MIN)),
            Included((author, Round::MAX, BlockDigest::MAX)),
        )) {
            refs.push(BlockRef::new(round, author, digest));
        }
        let results = self.read_blocks(refs.as_slice())?;
        let mut blocks = vec![];
        for (r, block) in refs.into_iter().zip(results.into_iter()) {
            if let Some(block) = block {
                blocks.push(block);
            } else {
                panic!("Block {:?} not found!", r);
            }
        }
        Ok(blocks)
    }

    fn contains_block_at_slot(&self, slot: Slot) -> ConsensusResult<bool> {
        let inner = self.inner.read();
        let found = inner
            .digests_by_authorities
            .range((
                Included((slot.authority, slot.round, BlockDigest::MIN)),
                Included((slot.authority, slot.round, BlockDigest::MAX)),
            ))
            .next()
            .is_some();
        Ok(found)
    }

    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
        before_round: Option<Round>,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let before_round = before_round.unwrap_or(Round::MAX);
        let mut refs = VecDeque::new();
        for &(author, round, digest) in self
            .inner
            .read()
            .digests_by_authorities
            .range((
                Included((author, Round::MIN, BlockDigest::MIN)),
                Included((author, before_round, BlockDigest::MAX)),
            ))
            .rev()
            .take(num_of_rounds as usize)
        {
            refs.push_front(BlockRef::new(round, author, digest));
        }
        let results = self.read_blocks(refs.as_slices().0)?;
        let mut blocks = vec![];
        for (r, block) in refs.into_iter().zip(results.into_iter()) {
            blocks.push(
                block.unwrap_or_else(|| panic!("Storage inconsistency: block {:?} not found!", r)),
            );
        }
        Ok(blocks)
    }

    fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>> {
        let inner = self.inner.read();
        Ok(inner
            .commits
            .last_key_value()
            .map(|(_, commit)| commit.clone()))
    }

    fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>> {
        let inner = self.inner.read();
        let mut commits = vec![];
        for (_, commit) in inner.commits.range((
            Included((range.start(), CommitDigest::MIN)),
            Included((range.end(), CommitDigest::MAX)),
        )) {
            commits.push(commit.clone());
        }
        Ok(commits)
    }

    fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>> {
        let inner = self.inner.read();
        let votes = inner
            .commit_votes
            .range((
                Included((commit_index, CommitDigest::MIN, BlockRef::MIN)),
                Included((commit_index, CommitDigest::MAX, BlockRef::MAX)),
            ))
            .map(|(_, _, block_ref)| *block_ref)
            .collect();
        Ok(votes)
    }

    fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>> {
        let inner = self.inner.read();
        Ok(inner
            .commit_info
            .last_key_value()
            .map(|(k, v)| (CommitRef::new(k.0, k.1), v.clone())))
    }
}

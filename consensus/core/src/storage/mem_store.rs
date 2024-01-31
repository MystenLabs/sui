// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Bound::{Included, Unbounded},
};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use super::Store;
use crate::{
    block::{BlockDigest, BlockRef, Round, VerifiedBlock},
    commit::{Commit, CommitIndex},
    error::ConsensusResult,
};

/// In-memory storage for testing.
pub(crate) struct MemStore {
    inner: RwLock<Inner>,
}

struct Inner {
    blocks: BTreeMap<(Round, AuthorityIndex, BlockDigest), VerifiedBlock>,
    digests_by_authorities: BTreeSet<(AuthorityIndex, Round, BlockDigest)>,
    commits: BTreeMap<CommitIndex, Commit>,
}

impl MemStore {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        MemStore {
            inner: RwLock::new(Inner {
                blocks: BTreeMap::new(),
                digests_by_authorities: BTreeSet::new(),
                commits: BTreeMap::new(),
            }),
        }
    }
}

#[allow(unused)]
impl Store for MemStore {
    fn write(&self, blocks: Vec<VerifiedBlock>, commits: Vec<Commit>) -> ConsensusResult<()> {
        let mut inner = self.inner.write();

        for block in blocks {
            let block_ref = block.reference();
            inner
                .blocks
                .insert((block_ref.round, block_ref.author, block_ref.digest), block);
            inner.digests_by_authorities.insert((
                block_ref.author,
                block_ref.round,
                block_ref.digest,
            ));
        }
        for commit in commits {
            inner.commits.insert(commit.index, commit);
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

    fn scan_last_blocks_by_author(
        &self,
        author: AuthorityIndex,
        num_of_rounds: u64,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let mut refs = VecDeque::new();
        for &(author, round, digest) in self
            .inner
            .read()
            .digests_by_authorities
            .range((
                Included((author, Round::MIN, BlockDigest::MIN)),
                Included((author, Round::MAX, BlockDigest::MAX)),
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

    fn read_last_commit(&self) -> ConsensusResult<Option<Commit>> {
        let inner = self.inner.read();
        Ok(inner
            .commits
            .last_key_value()
            .map(|(_, commit)| commit.clone()))
    }

    fn scan_commits(&self, start_commit_index: CommitIndex) -> ConsensusResult<Vec<Commit>> {
        let inner = self.inner.read();
        let mut commits = vec![];
        for (_, commit) in inner
            .commits
            .range((Included(start_commit_index), Unbounded))
        {
            commits.push(commit.clone());
        }
        Ok(commits)
    }
}

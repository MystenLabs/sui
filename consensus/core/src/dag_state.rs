// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    block::{BlockRef, Round, Slot, VerifiedBlock},
    commit::Commit,
    context::Context,
    storage::Store,
};

/// Rounds of recently committed blocks cached in memory, per authority.
#[allow(unused)]
const CACHED_ROUNDS: Round = 100;

/// DagState provides the API to write and read accepted blocks from the DAG.
/// Only uncommited and last committed blocks are cached in memory.
/// The rest of blocks are stored on disk.
/// Refs to cached blocks and additional refs are cached as well, to speed up existence checks.
///
/// Note: DagState should be wrapped with Arc<parking_lot::RwLock<_>>, to allow
/// concurrent access from multiple components.
#[allow(unused)]
pub(crate) struct DagState {
    context: Arc<Context>,

    // Caches blocks within CACHED_ROUNDS from the last committed round per authority.
    // Note: uncommitted blocks will always be in memory.
    recent_blocks: BTreeMap<BlockRef, VerifiedBlock>,

    // Accepted blocks have their refs cached. Cached refs are never removed until restart.
    // Each element in the Vec corresponds to the authority with the index.
    cached_refs: Vec<BTreeSet<BlockRef>>,

    // Last consensus commit of the dag.
    last_commit: Option<Commit>,

    // Persistent storage for blocks, commits and other consensus data.
    store: Arc<dyn Store>,
}

#[allow(unused)]
impl DagState {
    /// Initializes DagState from storage.
    pub(crate) fn new(context: Arc<Context>, store: Arc<dyn Store>) -> Self {
        let num_authorities = context.committee.size();
        let last_commit = store.read_last_commit().unwrap();
        let last_committed_rounds = match &last_commit {
            Some(commit) => commit.last_committed_rounds.clone(),
            None => vec![0; num_authorities],
        };

        let mut state = Self {
            context,
            recent_blocks: BTreeMap::new(),
            cached_refs: vec![BTreeSet::new(); num_authorities],
            last_commit,
            store,
        };

        for (i, round) in last_committed_rounds.into_iter().enumerate() {
            let authority_index = state.context.committee.to_authority_index(i).unwrap();
            let blocks = state
                .store
                .scan_blocks_by_author(authority_index, round.saturating_sub(CACHED_ROUNDS))
                .unwrap();
            for block in blocks {
                state.accept_block(block);
            }
        }

        state
    }

    /// Accepts a block into DagState and keeps it in memory.
    pub(crate) fn accept_block(&mut self, block: VerifiedBlock) {
        let block_ref = block.reference();
        self.recent_blocks.insert(block_ref, block);
        self.cached_refs[block_ref.author].insert(block_ref);
    }

    /// Gets a copy of an uncommitted block. Returns None if not found.
    /// Uncommitted must be in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_block(&self, reference: &BlockRef) -> Option<VerifiedBlock> {
        self.recent_blocks.get(reference).cloned()
    }

    pub(crate) fn get_blocks_at_slot(&self, _slot: Slot) -> Vec<VerifiedBlock> {
        unimplemented!()
    }

    pub(crate) fn linked_to_round(
        &self,
        _later_block: &VerifiedBlock,
        _earlier_round: Round,
    ) -> Vec<VerifiedBlock> {
        unimplemented!()
    }

    pub(crate) fn get_blocks_by_round(&self, _round: Round) -> Vec<VerifiedBlock> {
        unimplemented!()
    }
}

// TODO: add unit tests.

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    block::{BlockAPI as _, BlockRef, Round, Slot, VerifiedBlock},
    context::Context,
    storage::Store,
};

/// Recent rounds of blocks to cached in memory, counted from the last committed leader round.
#[allow(unused)]
const BLOCK_CACHED_ROUNDS: Round = 100;

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

    // Caches uncommitted blocks, and recent blocks within BLOCK_CACHED_ROUNDS from the
    // last committed leader round.
    recent_blocks: BTreeMap<BlockRef, VerifiedBlock>,

    // All accepted blocks have their refs cached. Cached refs are never removed for now.
    // Each element in the vector contains refs for the authority corresponding to its index.
    cached_refs: Vec<BTreeSet<BlockRef>>,

    // Persistent storage for blocks, commits and other consensus data.
    store: Arc<dyn Store>,
}

#[allow(unused)]
impl DagState {
    pub(crate) fn new(
        context: Arc<Context>,
        blocks: Vec<VerifiedBlock>,
        store: Arc<dyn Store>,
    ) -> Self {
        let num_authorities = context.committee.size();
        let mut state = Self {
            context,
            recent_blocks: BTreeMap::new(),
            cached_refs: vec![BTreeSet::new(); num_authorities],
            store,
        };

        for block in blocks {
            state.add_block(block);
        }

        state
    }

    pub(crate) fn add_block(&mut self, block: VerifiedBlock) {
        let block_ref = block.reference();
        self.recent_blocks.insert(block_ref, block);
        self.cached_refs[block_ref.author].insert(block_ref);
    }

    pub(crate) fn get_block(&self, _reference: BlockRef) -> Option<VerifiedBlock> {
        unimplemented!()
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

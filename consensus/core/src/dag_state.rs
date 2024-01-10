// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    block::{BlockAPI as _, BlockRef, VerifiedBlock},
    context::Context,
    storage::Store,
};

/// DagState provides the API to write and read accepted blocks from the DAG.
/// The underlying blocks can be cached in memory or stored on disk.
#[allow(unused)]
pub(crate) struct DagState {
    context: Context,

    // Cached refs are never removed for now.
    cached_refs: Vec<BTreeSet<BlockRef>>,

    // Blocks only need to be cached until they are committed.
    cached_blocks: BTreeMap<BlockRef, VerifiedBlock>,

    // Persistent storage of blocks and other consensus data.
    store: Arc<dyn Store>,
}

#[allow(unused)]
impl DagState {
    pub(crate) fn new(context: Context, blocks: Vec<VerifiedBlock>, store: Arc<dyn Store>) -> Self {
        let num_authorities = context.committee.size();
        let mut state = Self {
            context,
            cached_blocks: BTreeMap::new(),
            cached_refs: vec![BTreeSet::new(); num_authorities],
            store,
        };

        for block in blocks {
            state.add_block(block);
        }

        state
    }

    pub(crate) fn add_block(&mut self, block: VerifiedBlock) {
        let block_ref = block.block.reference();
        self.cached_refs[block_ref.author].insert(block_ref);
        self.cached_blocks.insert(block_ref, block);
    }
}

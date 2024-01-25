// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::{BlockAPI, BlockRef, VerifiedBlock};
use std::collections::HashSet;

/// Block manager suspends incoming blocks until they are connected to the existing graph,
/// returning newly connected blocks.
/// TODO: As it is possible to have Byzantine validators who produce Blocks without valid causal
/// history we need to make sure that BlockManager takes care of that and avoid OOM (Out Of Memory)
/// situations.
#[allow(dead_code)]
pub(crate) struct BlockManager {
    // TODO: dummy implementation, just keep all the block references
    accepted_blocks: HashSet<BlockRef>,
}

#[allow(dead_code)]
impl BlockManager {
    pub(crate) fn new() -> Self {
        Self {
            accepted_blocks: HashSet::new(),
        }
    }

    /// Tries to accept the provided blocks assuming that all their causal history exists. The method
    /// returns all the blocks that have been successfully processed in round ascending order, that includes also previously
    /// suspended blocks that have now been able to get accepted.
    pub(crate) fn add_blocks(&mut self, mut blocks: Vec<VerifiedBlock>) -> Vec<VerifiedBlock> {
        // TODO: add implementation - for now just dummy/test. Accept everything assuming history exists and cache them
        // to ensure that they are not returned if they already processed.
        blocks.sort_by_key(|b1| b1.round());
        blocks
            .into_iter()
            .flat_map(|block| {
                self.accepted_blocks
                    .insert(block.reference())
                    .then_some(block)
            })
            .collect()
    }

    /// Returns all the blocks that are currently missing and needed in order to accept suspended
    /// blocks.
    pub(crate) fn missing_blocks(&self) -> Vec<BlockRef> {
        unimplemented!()
    }

    /// Returns all the suspended blocks whose causal history we miss hence we can't accept them yet.
    pub(crate) fn suspended_blocks(&self) -> Vec<BlockRef> {
        unimplemented!()
    }
}

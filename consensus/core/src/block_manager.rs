// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::{Block, BlockRef};

/// Block manager suspends incoming blocks until they are connected to the existing graph,
/// returning newly connected blocks.
/// TODO: As it is possible to have Byzantine validators who produce Blocks without valid causal
/// history we need to make sure that BlockManager takes care of that and avoid OOM (Out Of Memory)
/// situations.
pub(crate) struct BlockManager {}

impl BlockManager {
    pub(crate) fn new() -> Self {
        Self {}
    }

    /// Tries to accept the provided blocks assuming that all their causal history exists. The method
    /// returns all the blocks that have been successfully processed, that includes also previously
    /// suspended blocks that have now been able to get accepted.
    pub(crate) fn add_blocks(&mut self, blocks: Vec<Block>) -> Vec<Block> {
        // TODO: add implementation - for now just return the blocks
        blocks
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: replace with proper storage/interface code.

use crate::{
    block::{Block, BlockRef, BlockSlot},
    types::Round,
};

pub struct BlockStore {}

#[allow(unused)]
impl BlockStore {
    pub fn get_block(&self, _reference: BlockRef) -> Option<Block> {
        unimplemented!()
    }

    pub fn get_blocks_at_block_slot(&self, _block_slot: BlockSlot) -> Vec<Block> {
        unimplemented!()
    }

    pub fn linked_to_round(&self, _later_block: &Block, _earlier_round: Round) -> Vec<Block> {
        unimplemented!()
    }

    pub fn get_blocks_by_round(&self, _round: Round) -> Vec<Block> {
        unimplemented!()
    }
}

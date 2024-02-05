// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::{Block, BlockAPI, BlockRef, VerifiedBlock};
use crate::context::Context;
use crate::dag_state::DagState;
use crate::error::ConsensusResult;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct SuspendedBlock {
    block: VerifiedBlock,
    missing_ancestors: HashSet<BlockRef>,
}

impl SuspendedBlock {
    fn new(block: VerifiedBlock, missing_ancestors: HashSet<BlockRef>) -> Self {
        Self {
            block,
            missing_ancestors,
        }
    }
}

/// Block manager suspends incoming blocks until they are connected to the existing graph,
/// returning newly connected blocks.
/// TODO: As it is possible to have Byzantine validators who produce Blocks without valid causal
/// history we need to make sure that BlockManager takes care of that and avoid OOM (Out Of Memory)
/// situations.
#[allow(dead_code)]
pub(crate) struct BlockManager {
    /// Keeps all the suspended blocks. A suspended block is a block that is missing part of its causal history and thus
    /// can't be immediately processed. A block will remain in this map until all its causal history has been successfully
    /// processed.
    suspended_blocks: HashMap<BlockRef, SuspendedBlock>,
    /// A map that keeps all the blocks that we are missing (keys) and the corresponding blocks that reference the missing blocks
    /// as ancestors and need them to get unsuspended. It is possible for a missing dependency (key) to be a suspended block, so
    /// the block has been already fetched but it self is still missing some of its ancestors to be processed.
    missing_ancestors: HashMap<BlockRef, HashSet<BlockRef>>,
    genesis: HashSet<BlockRef>,
    dag_state: Arc<RwLock<DagState>>,
    context: Arc<Context>,
}

#[allow(dead_code)]
impl BlockManager {
    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let (_, genesis) = Block::genesis(context.clone());
        let genesis = genesis.into_iter().map(|block| block.reference()).collect();
        Self {
            suspended_blocks: HashMap::new(),
            missing_ancestors: HashMap::new(),
            genesis,
            context,
            dag_state,
        }
    }

    /// Tries to accept the provided blocks assuming that all their causal history exists. The method
    /// returns all the blocks that have been successfully processed in round ascending order, that includes also previously
    /// suspended blocks that have now been able to get accepted.
    pub(crate) fn accept_blocks(
        &mut self,
        mut blocks: Vec<VerifiedBlock>,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let mut accepted_blocks: Vec<VerifiedBlock> = vec![];

        blocks.sort_by_key(|b| b.round());
        for block in blocks {
            if let Some(block) = self.try_accept_block(block)? {
                // Try to unsuspend and accept any children blocks
                let mut children_blocks = self.try_accept_children_blocks(&block);
                children_blocks.push(block);

                // Accept the state in DAG here so the next block to be processed can find any DAG/store any accepted blocks.
                self.dag_state
                    .write()
                    .accept_blocks(children_blocks.clone());

                accepted_blocks.extend(children_blocks);
            }
        }

        accepted_blocks.sort_by_key(|b| b.round());
        Ok(accepted_blocks)
    }

    /// Tries to accept the provided block. To accept a block its ancestors must have been already successfully accepted. If
    /// block is accepted then Some result is returned. None is returned when either the block is suspended or the block
    /// has been already accepted before.
    fn try_accept_block(&mut self, block: VerifiedBlock) -> ConsensusResult<Option<VerifiedBlock>> {
        let block_ref = block.reference();
        let mut missing_ancestors = HashSet::new();

        // If block has been already received and suspended, or already processed and stored, then skip it.
        if self.suspended_blocks.contains_key(&block_ref)
            || self
                .dag_state
                .read()
                .contains_block_in_cache_or_store(&block_ref)?
        {
            return Ok(None);
        }

        // make sure that we have all the required ancestors in store
        for ancestor in block.ancestors() {
            if self.genesis.contains(ancestor) {
                continue;
            }

            // If the reference is already missing, or is not included in the store then we mark the block as non processed
            // and we add the block_ref dependency to the `missing_blocks`
            if self.missing_ancestors.contains_key(ancestor)
                || !self
                    .dag_state
                    .read()
                    .contains_block_in_cache_or_store(ancestor)?
            {
                missing_ancestors.insert(*ancestor);

                // mark the block as having missing ancestors
                self.missing_ancestors
                    .entry(*ancestor)
                    .or_default()
                    .insert(block_ref);
            }
        }

        if !missing_ancestors.is_empty() {
            let hostname = self
                .context
                .committee
                .authority(block.author())
                .hostname
                .as_str();
            self.context
                .metrics
                .node_metrics
                .uniquely_suspended_blocks
                .with_label_values(&[hostname])
                .inc();
            self.suspended_blocks
                .insert(block_ref, SuspendedBlock::new(block, missing_ancestors));
            return Ok(None);
        }

        Ok(Some(block))
    }

    /// Given an unsuspended `block` it attempts to accept all the children blocks. All the unsuspended/accepted blocks are
    /// returned as a vector.
    fn try_accept_children_blocks(&mut self, block: &VerifiedBlock) -> Vec<VerifiedBlock> {
        let mut unsuspended_blocks = vec![];
        let mut to_process_blocks = vec![block.clone()];

        while let Some(block) = to_process_blocks.pop() {
            // And try to check if its direct children can be unsuspended
            if let Some(block_refs_with_missing_deps) =
                self.missing_ancestors.remove(&block.reference())
            {
                for r in block_refs_with_missing_deps {
                    // For each dependency try to unsuspend it. If that's successful then we add it to the queue so
                    // we can recursively try to unsuspend its children.
                    if let Some(block) = self.try_unsuspend_block(&r) {
                        unsuspended_blocks.push(block.block.clone());
                        to_process_blocks.push(block.block);
                    }
                }
            }
        }

        // Report the unsuspended blocks
        for block in &unsuspended_blocks {
            let hostname = self
                .context
                .committee
                .authority(block.author())
                .hostname
                .as_str();
            self.context
                .metrics
                .node_metrics
                .unsuspended_blocks
                .with_label_values(&[hostname])
                .inc();
        }

        unsuspended_blocks
    }

    /// Attempts to unsuspend a block by checking its ancestors. If there is no missing dependency then this block
    /// can be unsuspended immediately and is removed from the `suspended_blocks` map.
    fn try_unsuspend_block(&mut self, block_ref: &BlockRef) -> Option<SuspendedBlock> {
        let block = self
            .suspended_blocks
            .get_mut(block_ref)
            .expect("Block should be in suspended map");

        assert!(
            block.missing_ancestors.remove(block_ref),
            "Block reference {} should be present in missing dependencies of {:?}",
            block_ref,
            block.block
        );

        if block.missing_ancestors.is_empty() {
            // we have no missing dependency, so we unsuspend the block and return it
            return self.suspended_blocks.remove(block_ref);
        }
        None
    }

    /// Returns all the blocks that are currently missing and needed in order to accept suspended
    /// blocks.
    pub(crate) fn missing_blocks(&self) -> Vec<BlockRef> {
        self.missing_ancestors
            .iter()
            .flat_map(|(block_ref, _)| {
                (!self.suspended_blocks.contains_key(block_ref)).then_some(*block_ref)
            })
            .collect()
    }

    /// Returns all the suspended blocks whose causal history we miss hence we can't accept them yet.
    pub(crate) fn suspended_blocks(&self) -> Vec<BlockRef> {
        self.suspended_blocks.keys().cloned().collect()
    }
}

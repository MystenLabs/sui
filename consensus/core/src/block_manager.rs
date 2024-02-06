// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::{Block, BlockAPI, BlockRef, VerifiedBlock};
use crate::context::Context;
use crate::dag_state::DagState;
use crate::error::ConsensusResult;
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashSet};
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
    suspended_blocks: BTreeMap<BlockRef, SuspendedBlock>,
    /// A map that keeps all the blocks that we are missing (keys) and the corresponding blocks that reference the missing blocks
    /// as ancestors and need them to get unsuspended. It is possible for a missing dependency (key) to be a suspended block, so
    /// the block has been already fetched but it self is still missing some of its ancestors to be processed.
    missing_ancestors: BTreeMap<BlockRef, HashSet<BlockRef>>,
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
            suspended_blocks: BTreeMap::new(),
            missing_ancestors: BTreeMap::new(),
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
                let mut children_blocks = self.try_unsuspend_children_blocks(&block);
                children_blocks.push(block);

                // Accept the state in DAG here so the next block to be processed can find any DAG/store any accepted blocks.
                self.dag_state
                    .write()
                    .accept_blocks(children_blocks.clone());

                accepted_blocks.extend(children_blocks);
            }
        }

        accepted_blocks.sort_by_key(|b| b.reference());
        Ok(accepted_blocks)
    }

    /// Tries to accept the provided block. To accept a block its ancestors must have been already successfully accepted. If
    /// block is accepted then Some result is returned. None is returned when either the block is suspended or the block
    /// has been already accepted before.
    fn try_accept_block(&mut self, block: VerifiedBlock) -> ConsensusResult<Option<VerifiedBlock>> {
        let block_ref = block.reference();
        let mut missing_ancestors = HashSet::new();

        // If block has been already received and suspended, or already processed and stored, or is a genesis block, then skip it.
        if self.suspended_blocks.contains_key(&block_ref)
            || self
                .dag_state
                .read()
                .contains_block_in_cache_or_store(&block_ref)?
            || self.genesis.contains(&block_ref)
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

    /// Given an accepted block `block` it attempts to accept all the suspended children blocks assuming such exist.
    /// All the unsuspended/accepted blocks are returned as a vector.
    fn try_unsuspend_children_blocks(
        &mut self,
        accepted_block: &VerifiedBlock,
    ) -> Vec<VerifiedBlock> {
        let mut unsuspended_blocks = vec![];
        let mut to_process_blocks = vec![accepted_block.clone()];

        while let Some(block) = to_process_blocks.pop() {
            // And try to check if its direct children can be unsuspended
            if let Some(block_refs_with_missing_deps) =
                self.missing_ancestors.remove(&block.reference())
            {
                for r in block_refs_with_missing_deps {
                    // For each dependency try to unsuspend it. If that's successful then we add it to the queue so
                    // we can recursively try to unsuspend its children.
                    if let Some(block) = self.try_unsuspend_block(&r, &block.reference()) {
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

    /// Attempts to unsuspend a block by checking its ancestors and removing the `accepted_dependency` by its local set.
    /// If there is no missing dependency then this block can be unsuspended immediately and is removed from the `suspended_blocks` map.
    fn try_unsuspend_block(
        &mut self,
        block_ref: &BlockRef,
        accepted_dependency: &BlockRef,
    ) -> Option<SuspendedBlock> {
        let block = self
            .suspended_blocks
            .get_mut(block_ref)
            .expect("Block should be in suspended map");

        assert!(
            block.missing_ancestors.remove(accepted_dependency),
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

#[cfg(test)]
mod tests {
    use crate::block::{Block, BlockAPI, Round, TestBlock, VerifiedBlock};
    use crate::block_manager::BlockManager;
    use crate::context::Context;
    use crate::dag_state::DagState;
    use crate::storage::mem_store::MemStore;
    use parking_lot::RwLock;
    use rand::prelude::StdRng;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    use std::sync::Arc;

    #[test]
    fn suspend_blocks_with_missing_ancestors() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 2 rounds
        let all_blocks = dag(context, 2);

        // Take only the blocks of round 2 and try to accept them
        let round_2_blocks = all_blocks
            .into_iter()
            .filter(|block| block.round() == 2)
            .collect::<Vec<VerifiedBlock>>();

        // WHEN
        let accepted_blocks = block_manager
            .accept_blocks(round_2_blocks.clone())
            .expect("No error was expected");

        // THEN
        assert!(accepted_blocks.is_empty());

        // AND the missing blocks are the parents of the round 2 blocks. Since this is a fully connected DAG taking the
        // ancestors of the first element suffices.
        let missing_blocks = round_2_blocks.first().unwrap().ancestors();
        assert_eq!(block_manager.missing_blocks(), missing_blocks);

        // AND suspended blocks should return the round_2_blocks
        assert_eq!(
            block_manager.suspended_blocks(),
            round_2_blocks
                .into_iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn accept_blocks_with_complete_causal_history() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 2 rounds
        let all_blocks = dag(context, 2);

        // WHEN
        let accepted_blocks = block_manager
            .accept_blocks(all_blocks.clone())
            .expect("No error was expected");

        // THEN
        assert!(accepted_blocks.len() == 8);
        assert_eq!(
            accepted_blocks,
            all_blocks
                .iter()
                .filter(|block| block.round() > 0)
                .cloned()
                .collect::<Vec<VerifiedBlock>>()
        );

        // WHEN trying to accept same blocks again, then none will be returned as those have been already accepted
        let accepted_blocks = block_manager
            .accept_blocks(all_blocks)
            .expect("No error was expected");
        assert!(accepted_blocks.is_empty());
    }

    #[test]
    fn accept_blocks_unsuspend_children_blocks() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);

        // create a DAG of 3 rounds
        let all_blocks = dag(context.clone(), 3);
        // keep only the non-genesis blocks
        let mut all_blocks = all_blocks
            .into_iter()
            .filter(|block| block.round() > 0)
            .collect::<Vec<_>>();

        // Now randomize the sequence of sending the blocks to block manager. In the end all the blocks should be uniquely
        // suspended and no missing blocks should exist.
        for seed in 0..100u8 {
            all_blocks.shuffle(&mut StdRng::from_seed([seed; 32]));

            let store = Arc::new(MemStore::new());
            let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

            let mut block_manager = BlockManager::new(context.clone(), dag_state);

            // WHEN
            let mut all_accepted_blocks = vec![];
            for block in &all_blocks {
                let accepted_blocks = block_manager
                    .accept_blocks(vec![block.clone()])
                    .expect("No error was expected");

                all_accepted_blocks.extend(accepted_blocks);
            }

            // THEN
            all_accepted_blocks.sort_by_key(|b| b.reference());
            all_blocks.sort_by_key(|b| b.reference());

            assert_eq!(
                all_accepted_blocks, all_blocks,
                "Failed acceptance sequence for seed {}",
                seed
            );
            assert!(block_manager.missing_blocks().is_empty());
            assert!(block_manager.suspended_blocks().is_empty());
        }
    }

    /// Creates all the blocks to produce a fully connected DAG from round 0 up to `end_round`.
    /// Note: this method also returns the genesis blocks.
    fn dag(context: Arc<Context>, end_round: u64) -> Vec<VerifiedBlock> {
        let (_, mut last_round_blocks) = Block::genesis(context.clone());
        let mut all_blocks = last_round_blocks.clone();
        for round in 1..=end_round {
            let mut this_round_blocks = Vec::new();
            for (index, _authority) in context.committee.authorities() {
                let block = TestBlock::new(round as Round, index.value() as u32)
                    .set_ancestors(last_round_blocks.iter().map(|b| b.reference()).collect())
                    .build();

                this_round_blocks.push(VerifiedBlock::new_for_test(block));
            }
            all_blocks.extend(this_round_blocks.clone());
            last_round_blocks = this_round_blocks;
        }
        all_blocks
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

use itertools::Itertools as _;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use tracing::{debug, warn};

use crate::{
    block::{BlockAPI, BlockRef, VerifiedBlock, GENESIS_ROUND},
    context::Context,
    dag_state::DagState,
    Round,
};

struct SuspendedBlock {
    block: VerifiedBlock,
    missing_ancestors: BTreeSet<BlockRef>,
    timestamp: Instant,
}

impl SuspendedBlock {
    fn new(block: VerifiedBlock, missing_ancestors: BTreeSet<BlockRef>) -> Self {
        Self {
            block,
            missing_ancestors,
            timestamp: Instant::now(),
        }
    }
}

/// Block manager suspends incoming blocks until they are connected to the existing graph,
/// returning newly connected blocks.
/// TODO: As it is possible to have Byzantine validators who produce Blocks without valid causal
/// history we need to make sure that BlockManager takes care of that and avoid OOM (Out Of Memory)
/// situations.
pub(crate) struct BlockManager {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,

    /// Keeps all the suspended blocks. A suspended block is a block that is missing part of its causal history and thus
    /// can't be immediately processed. A block will remain in this map until all its causal history has been successfully
    /// processed.
    suspended_blocks: BTreeMap<BlockRef, SuspendedBlock>,
    /// A map that keeps all the blocks that we are missing (keys) and the corresponding blocks that reference the missing blocks
    /// as ancestors and need them to get unsuspended. It is possible for a missing dependency (key) to be a suspended block, so
    /// the block has been already fetched but it self is still missing some of its ancestors to be processed.
    missing_ancestors: BTreeMap<BlockRef, BTreeSet<BlockRef>>,
    /// Keeps all the blocks that we actually miss and haven't fetched them yet. That set will basically contain all the
    /// keys from the `missing_ancestors` minus any keys that exist in `suspended_blocks`.
    missing_blocks: BTreeSet<BlockRef>,
    /// A vector that holds a tuple of (lowest_round, highest_round) of received blocks per authority.
    /// This is used for metrics reporting purposes and resets during restarts.
    received_block_rounds: Vec<Option<(Round, Round)>>,
}

impl BlockManager {
    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let committee_size = context.committee.size();
        Self {
            context,
            dag_state,
            suspended_blocks: BTreeMap::new(),
            missing_ancestors: BTreeMap::new(),
            missing_blocks: BTreeSet::new(),
            received_block_rounds: vec![None; committee_size],
        }
    }

    /// Tries to accept the provided blocks assuming that all their causal history exists. The method
    /// returns all the blocks that have been successfully processed in round ascending order, that includes also previously
    /// suspended blocks that have now been able to get accepted. Method also returns a set with the missing ancestor blocks.
    #[tracing::instrument(skip_all)]
    pub(crate) fn try_accept_blocks(
        &mut self,
        blocks: Vec<VerifiedBlock>,
    ) -> (Vec<VerifiedBlock>, BTreeSet<BlockRef>) {
        let _s = monitored_scope("BlockManager::try_accept_blocks");
        self.try_accept_blocks_internal(blocks, false)
    }

    // Tries to accept blocks that have been committed. Returns all the blocks that have been accepted, both from the ones
    // provided and any children blocks.
    #[tracing::instrument(skip_all)]
    pub(crate) fn try_accept_committed_blocks(
        &mut self,
        blocks: Vec<VerifiedBlock>,
    ) -> Vec<VerifiedBlock> {
        // Just accept the blocks
        let _s = monitored_scope("BlockManager::try_accept_committed_blocks");
        let (accepted_blocks, missing_blocks) = self.try_accept_blocks_internal(blocks, true);
        assert!(
            missing_blocks.is_empty(),
            "No missing blocks should be returned for committed blocks"
        );

        accepted_blocks
    }

    /// Attempts to accept the provided blocks. When `committed = true` then the blocks are considered to be committed via certified commits and
    /// are handled differently.
    fn try_accept_blocks_internal(
        &mut self,
        mut blocks: Vec<VerifiedBlock>,
        committed: bool,
    ) -> (Vec<VerifiedBlock>, BTreeSet<BlockRef>) {
        let _s = monitored_scope("BlockManager::try_accept_blocks_internal");

        blocks.sort_by_key(|b| b.round());
        debug!(
            "Trying to accept blocks: {}",
            blocks.iter().map(|b| b.reference().to_string()).join(",")
        );

        let mut accepted_blocks = vec![];
        let mut missing_blocks = BTreeSet::new();

        for block in blocks {
            self.update_block_received_metrics(&block);

            // Try to accept the input block.
            let block_ref = block.reference();

            let mut blocks_to_accept = vec![];
            if committed {
                match self.try_accept_one_committed_block(block) {
                    TryAcceptResult::Accepted(block) => {
                        // As this is a committed block, then it's already accepted and there is no need to verify its timestamps.
                        // Just add it to the accepted blocks list.
                        accepted_blocks.push(block);
                    }
                    TryAcceptResult::Processed => continue,
                    TryAcceptResult::Suspended(_) | TryAcceptResult::Skipped => panic!(
                        "Did not expect to suspend or skip a committed block: {:?}",
                        block_ref
                    ),
                };
            } else {
                match self.try_accept_one_block(block) {
                    TryAcceptResult::Accepted(block) => {
                        blocks_to_accept.push(block);
                    }
                    TryAcceptResult::Suspended(ancestors_to_fetch) => {
                        debug!(
                            "Missing ancestors to fetch for block {block_ref}: {}",
                            ancestors_to_fetch.iter().map(|b| b.to_string()).join(",")
                        );
                        missing_blocks.extend(ancestors_to_fetch);
                        continue;
                    }
                    TryAcceptResult::Processed | TryAcceptResult::Skipped => continue,
                };
            };

            // If the block is accepted, try to unsuspend its children blocks if any.
            let unsuspended_blocks = self.try_unsuspend_children_blocks(block_ref);
            blocks_to_accept.extend(unsuspended_blocks);

            // Insert the accepted blocks into DAG state so future blocks including them as
            // ancestors do not get suspended.
            self.dag_state
                .write()
                .accept_blocks(blocks_to_accept.clone());

            accepted_blocks.extend(blocks_to_accept);
        }

        self.update_stats(missing_blocks.len() as u64);

        // Figure out the new missing blocks
        (accepted_blocks, missing_blocks)
    }

    fn try_accept_one_committed_block(&mut self, block: VerifiedBlock) -> TryAcceptResult {
        if self.dag_state.read().contains_block(&block.reference()) {
            return TryAcceptResult::Processed;
        }

        // Remove the block from missing and suspended blocks
        self.missing_blocks.remove(&block.reference());

        // If the block has been already fetched and parked as suspended block, then remove it. Also find all the references of missing
        // ancestors to remove those as well. If we don't do that then it's possible once the missing ancestor is fetched to cause a panic
        // when trying to unsuspend this children as it won't be found in the suspended blocks map.
        if let Some(suspended_block) = self.suspended_blocks.remove(&block.reference()) {
            suspended_block
                .missing_ancestors
                .iter()
                .for_each(|ancestor| {
                    if let Some(references) = self.missing_ancestors.get_mut(ancestor) {
                        references.remove(&block.reference());
                    }
                });
        }

        // Accept this block before any unsuspended children blocks
        self.dag_state.write().accept_blocks(vec![block.clone()]);

        TryAcceptResult::Accepted(block)
    }

    /// Tries to find the provided block_refs in DagState and BlockManager,
    /// and returns missing block refs.
    pub(crate) fn try_find_blocks(&mut self, block_refs: Vec<BlockRef>) -> BTreeSet<BlockRef> {
        let _s = monitored_scope("BlockManager::try_find_blocks");
        let gc_round = self.dag_state.read().gc_round();

        // No need to fetch blocks that are <= gc_round as they won't get processed anyways and they'll get skipped.
        // So keep only the ones above.
        let mut block_refs = block_refs
            .into_iter()
            .filter(|block_ref| block_ref.round > gc_round)
            .collect::<Vec<_>>();

        if block_refs.is_empty() {
            return BTreeSet::new();
        }

        block_refs.sort_by_key(|b| b.round);

        debug!(
            "Trying to find blocks: {}",
            block_refs.iter().map(|b| b.to_string()).join(",")
        );

        let mut missing_blocks = BTreeSet::new();

        for (found, block_ref) in self
            .dag_state
            .read()
            .contains_blocks(block_refs.clone())
            .into_iter()
            .zip(block_refs.iter())
        {
            if found || self.suspended_blocks.contains_key(block_ref) {
                continue;
            }
            // Fetches the block if it is not in dag state or suspended.
            missing_blocks.insert(*block_ref);
            if self.missing_blocks.insert(*block_ref) {
                // We want to report this as a missing ancestor even if there is no block that is actually references it right now. That will allow us
                // to seamlessly GC the block later if needed.
                self.missing_ancestors.entry(*block_ref).or_default();

                let block_ref_hostname =
                    &self.context.committee.authority(block_ref.author).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .block_manager_missing_blocks_by_authority
                    .with_label_values(&[block_ref_hostname])
                    .inc();
            }
        }

        let metrics = &self.context.metrics.node_metrics;
        metrics
            .missing_blocks_total
            .inc_by(missing_blocks.len() as u64);
        metrics
            .block_manager_missing_blocks
            .set(self.missing_blocks.len() as i64);

        missing_blocks
    }

    /// Tries to accept the provided block. To accept a block its ancestors must have been already successfully accepted. If
    /// block is accepted then Some result is returned. None is returned when either the block is suspended or the block
    /// has been already accepted before.
    fn try_accept_one_block(&mut self, block: VerifiedBlock) -> TryAcceptResult {
        let block_ref = block.reference();
        let mut missing_ancestors = BTreeSet::new();
        let mut ancestors_to_fetch = BTreeSet::new();
        let dag_state = self.dag_state.read();
        let gc_round = dag_state.gc_round();

        // If block has been already received and suspended, or already processed and stored, or is a genesis block, then skip it.
        if self.suspended_blocks.contains_key(&block_ref) || dag_state.contains_block(&block_ref) {
            return TryAcceptResult::Processed;
        }

        // If the block is <= gc_round, then we simply skip its processing as there is no meaning do any action on it or even store it.
        if block.round() <= gc_round {
            let hostname = self
                .context
                .committee
                .authority(block.author())
                .hostname
                .as_str();
            self.context
                .metrics
                .node_metrics
                .block_manager_skipped_blocks
                .with_label_values(&[hostname])
                .inc();
            return TryAcceptResult::Skipped;
        }

        // Keep only the ancestors that are greater than the GC round to check for their existence.
        let ancestors = block
            .ancestors()
            .iter()
            .filter(|ancestor| ancestor.round == GENESIS_ROUND || ancestor.round > gc_round)
            .cloned()
            .collect::<Vec<_>>();

        // make sure that we have all the required ancestors in store
        for (found, ancestor) in dag_state
            .contains_blocks(ancestors.clone())
            .into_iter()
            .zip(ancestors.iter())
        {
            if !found {
                missing_ancestors.insert(*ancestor);

                // mark the block as having missing ancestors
                self.missing_ancestors
                    .entry(*ancestor)
                    .or_default()
                    .insert(block_ref);

                let ancestor_hostname = &self.context.committee.authority(ancestor.author).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .block_manager_missing_ancestors_by_authority
                    .with_label_values(&[ancestor_hostname])
                    .inc();

                // Add the ancestor to the missing blocks set only if it doesn't already exist in the suspended blocks - meaning
                // that we already have its payload.
                if !self.suspended_blocks.contains_key(ancestor) {
                    // Fetches the block if it is not in dag state or suspended.
                    ancestors_to_fetch.insert(*ancestor);
                    if self.missing_blocks.insert(*ancestor) {
                        self.context
                            .metrics
                            .node_metrics
                            .block_manager_missing_blocks_by_authority
                            .with_label_values(&[ancestor_hostname])
                            .inc();
                    }
                }
            }
        }

        // Remove the block ref from the `missing_blocks` - if exists - since we now have received the block. The block
        // might still get suspended, but we won't report it as missing in order to not re-fetch.
        self.missing_blocks.remove(&block.reference());

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
                .block_suspensions
                .with_label_values(&[hostname])
                .inc();
            self.suspended_blocks
                .insert(block_ref, SuspendedBlock::new(block, missing_ancestors));
            return TryAcceptResult::Suspended(ancestors_to_fetch);
        }

        TryAcceptResult::Accepted(block)
    }

    /// Given an accepted block `accepted_block` it attempts to accept all the suspended children blocks assuming such exist.
    /// All the unsuspended / accepted blocks are returned as a vector in causal order.
    fn try_unsuspend_children_blocks(&mut self, accepted_block: BlockRef) -> Vec<VerifiedBlock> {
        let mut unsuspended_blocks = vec![];
        let mut to_process_blocks = vec![accepted_block];

        while let Some(block_ref) = to_process_blocks.pop() {
            // And try to check if its direct children can be unsuspended
            if let Some(block_refs_with_missing_deps) = self.missing_ancestors.remove(&block_ref) {
                for r in block_refs_with_missing_deps {
                    // For each dependency try to unsuspend it. If that's successful then we add it to the queue so
                    // we can recursively try to unsuspend its children.
                    if let Some(block) = self.try_unsuspend_block(&r, &block_ref) {
                        to_process_blocks.push(block.block.reference());
                        unsuspended_blocks.push(block);
                    }
                }
            }
        }

        let now = Instant::now();

        // Report the unsuspended blocks
        for block in &unsuspended_blocks {
            let hostname = self
                .context
                .committee
                .authority(block.block.author())
                .hostname
                .as_str();
            self.context
                .metrics
                .node_metrics
                .block_unsuspensions
                .with_label_values(&[hostname])
                .inc();
            self.context
                .metrics
                .node_metrics
                .suspended_block_time
                .with_label_values(&[hostname])
                .observe(now.saturating_duration_since(block.timestamp).as_secs_f64());
        }

        unsuspended_blocks
            .into_iter()
            .map(|block| block.block)
            .collect()
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

    /// Tries to unsuspend any blocks for the latest gc round. If gc round hasn't changed then no blocks will be unsuspended due to
    /// this action.
    pub(crate) fn try_unsuspend_blocks_for_latest_gc_round(&mut self) {
        let _s = monitored_scope("BlockManager::try_unsuspend_blocks_for_latest_gc_round");
        let gc_round = self.dag_state.read().gc_round();
        let mut blocks_unsuspended_below_gc_round = 0;
        let mut blocks_gc_ed = 0;

        while let Some((block_ref, _children_refs)) = self.missing_ancestors.first_key_value() {
            // If the first block in the missing ancestors is higher than the gc_round, then we can't unsuspend it yet. So we just put it back
            // and we terminate the iteration as any next entry will be of equal or higher round anyways.
            if block_ref.round > gc_round {
                return;
            }

            blocks_gc_ed += 1;

            let hostname = self
                .context
                .committee
                .authority(block_ref.author)
                .hostname
                .as_str();
            self.context
                .metrics
                .node_metrics
                .block_manager_gced_blocks
                .with_label_values(&[hostname])
                .inc();

            assert!(!self.suspended_blocks.contains_key(block_ref), "Block should not be suspended, as we are causally GC'ing and no suspended block should exist for a missing ancestor.");

            // Also remove it from the missing list - we don't want to keep looking for it.
            self.missing_blocks.remove(block_ref);

            // Find all the children blocks that have a dependency on this one and try to unsuspend them
            let unsuspended_blocks = self.try_unsuspend_children_blocks(*block_ref);

            unsuspended_blocks.iter().for_each(|block| {
                if block.round() <= gc_round {
                    blocks_unsuspended_below_gc_round += 1;
                }
            });

            // Now accept the unsuspended blocks
            self.dag_state
                .write()
                .accept_blocks(unsuspended_blocks.clone());

            for block in unsuspended_blocks {
                let hostname = self
                    .context
                    .committee
                    .authority(block.author())
                    .hostname
                    .as_str();
                self.context
                    .metrics
                    .node_metrics
                    .block_manager_gc_unsuspended_blocks
                    .with_label_values(&[hostname])
                    .inc();
            }
        }

        debug!(
            "Total {} blocks unsuspended and total blocks {} gc'ed <= gc_round {}",
            blocks_unsuspended_below_gc_round, blocks_gc_ed, gc_round
        );
    }

    /// Returns all the blocks that are currently missing and needed in order to accept suspended
    /// blocks.
    pub(crate) fn missing_blocks(&self) -> BTreeSet<BlockRef> {
        self.missing_blocks.clone()
    }

    fn update_stats(&mut self, missing_blocks: u64) {
        let metrics = &self.context.metrics.node_metrics;
        metrics.missing_blocks_total.inc_by(missing_blocks);
        metrics
            .block_manager_suspended_blocks
            .set(self.suspended_blocks.len() as i64);
        metrics
            .block_manager_missing_ancestors
            .set(self.missing_ancestors.len() as i64);
        metrics
            .block_manager_missing_blocks
            .set(self.missing_blocks.len() as i64);
    }

    fn update_block_received_metrics(&mut self, block: &VerifiedBlock) {
        let (min_round, max_round) =
            if let Some((curr_min, curr_max)) = self.received_block_rounds[block.author()] {
                (curr_min.min(block.round()), curr_max.max(block.round()))
            } else {
                (block.round(), block.round())
            };
        self.received_block_rounds[block.author()] = Some((min_round, max_round));

        let hostname = &self.context.committee.authority(block.author()).hostname;
        self.context
            .metrics
            .node_metrics
            .lowest_verified_authority_round
            .with_label_values(&[hostname])
            .set(min_round.into());
        self.context
            .metrics
            .node_metrics
            .highest_verified_authority_round
            .with_label_values(&[hostname])
            .set(max_round.into());
    }

    /// Checks if block manager is empty.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.suspended_blocks.is_empty()
            && self.missing_ancestors.is_empty()
            && self.missing_blocks.is_empty()
    }

    /// Returns all the suspended blocks whose causal history we miss hence we can't accept them yet.
    #[cfg(test)]
    fn suspended_blocks(&self) -> Vec<BlockRef> {
        self.suspended_blocks.keys().cloned().collect()
    }
}

// Result of trying to accept one block.
enum TryAcceptResult {
    // The block is accepted. Wraps the block itself.
    Accepted(VerifiedBlock),
    // The block is suspended. Wraps ancestors to be fetched.
    Suspended(BTreeSet<BlockRef>),
    // The block has been processed before and already exists in BlockManager (and is suspended) or
    // in DagState (so has been already accepted). No further processing has been done at this point.
    Processed,
    // When a received block is <= gc_round, then we simply skip its processing as there is no meaning
    // do any action on it or even store it.
    Skipped,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use consensus_config::AuthorityIndex;
    use parking_lot::RwLock;
    use rand::{prelude::StdRng, seq::SliceRandom, SeedableRng};
    use rstest::rstest;

    use crate::{
        block::{BlockAPI, BlockDigest, BlockRef, VerifiedBlock},
        block_manager::BlockManager,
        commit::TrustedCommit,
        context::Context,
        dag_state::DagState,
        storage::mem_store::MemStore,
        test_dag_builder::DagBuilder,
        test_dag_parser::parse_dag,
        CommitDigest, Round,
    };

    #[tokio::test]
    async fn suspend_blocks_with_missing_ancestors() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=2) // 2 rounds
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(2),
            ]) // Create equivocating blocks for 2 authorities
            .equivocate(3)
            .build();

        // Take only the blocks of round 2 and try to accept them
        let round_2_blocks = dag_builder
            .blocks
            .into_iter()
            .filter_map(|(_, block)| (block.round() == 2).then_some(block))
            .collect::<Vec<VerifiedBlock>>();

        // WHEN
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(round_2_blocks.clone());

        // THEN
        assert!(accepted_blocks.is_empty());

        // AND the returned missing ancestors should be the same as the provided block ancestors
        let missing_block_refs = round_2_blocks.first().unwrap().ancestors();
        let missing_block_refs = missing_block_refs.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(missing, missing_block_refs);

        // AND the missing blocks are the parents of the round 2 blocks. Since this is a fully connected DAG taking the
        // ancestors of the first element suffices.
        assert_eq!(block_manager.missing_blocks(), missing_block_refs);

        // AND suspended blocks should return the round_2_blocks
        assert_eq!(
            block_manager.suspended_blocks(),
            round_2_blocks
                .into_iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn try_accept_block_returns_missing_blocks() {
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=4) // 4 rounds
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(2),
            ]) // Create equivocating blocks for 2 authorities
            .equivocate(3) // Use 3 equivocations blocks per authority
            .build();

        // Take the blocks from round 4 up to 2 (included). Only the first block of each round should return missing
        // ancestors when try to accept
        for (_, block) in dag_builder
            .blocks
            .into_iter()
            .rev()
            .take_while(|(_, block)| block.round() >= 2)
        {
            // WHEN
            let (accepted_blocks, missing) = block_manager.try_accept_blocks(vec![block.clone()]);

            // THEN
            assert!(accepted_blocks.is_empty());

            let block_ancestors = block.ancestors().iter().cloned().collect::<BTreeSet<_>>();
            assert_eq!(missing, block_ancestors);
        }
    }

    #[tokio::test]
    async fn accept_blocks_with_complete_causal_history() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 2 rounds
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=2).build();

        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

        // WHEN
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(all_blocks.clone());

        // THEN
        assert_eq!(accepted_blocks.len(), 8);
        assert_eq!(
            accepted_blocks,
            all_blocks
                .iter()
                .filter(|block| block.round() > 0)
                .cloned()
                .collect::<Vec<VerifiedBlock>>()
        );
        assert!(missing.is_empty());
        assert!(block_manager.is_empty());

        // WHEN trying to accept same blocks again, then none will be returned as those have been already accepted
        let (accepted_blocks, _) = block_manager.try_accept_blocks(all_blocks);
        assert!(accepted_blocks.is_empty());
    }

    /// Tests that the block manager accepts blocks when some or all of their causal history is below or equal to the GC round.
    #[tokio::test]
    async fn accept_blocks_with_causal_history_below_gc_round() {
        // GIVEN
        let (mut context, _key_pairs) = Context::new_for_test(4);

        // We set the gc depth to 4
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        // We "fake" the commit for round 10, so we can test the GC round 6 (commit_round - gc_depth = 10 - 4 = 6)
        let last_commit = TrustedCommit::new_for_test(
            10,
            CommitDigest::MIN,
            context.clock.timestamp_utc_ms(),
            BlockRef::new(10, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            vec![],
        );
        dag_state.write().set_last_commit(last_commit);
        assert_eq!(
            dag_state.read().gc_round(),
            6,
            "GC round should have moved to round 6"
        );

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 10 rounds with some weak links for the blocks of round 9
        let dag_str = "DAG {
            Round 0 : { 4 },
            Round 1 : { * },
            Round 2 : { * },
            Round 3 : { * },
            Round 4 : { * },
            Round 5 : { * },
            Round 6 : { * },
            Round 7 : {
                A -> [*],
                B -> [*],
                C -> [*],
            }
            Round 8 : {
                A -> [*],
                B -> [*],
                C -> [*],
            },
            Round 9 : {
                A -> [A8, B8, C8, D6],
                B -> [A8, B8, C8, D6],
                C -> [A8, B8, C8, D6],
                D -> [A8, B8, C8, D6],
            },
            Round 10 : { * },
        }";

        let (_, dag_builder) = parse_dag(dag_str).expect("Invalid dag");

        // Now take all the blocks for round 7 & 8 , which are above the gc_round = 6.
        // All those blocks should eventually be returned as accepted. Pay attention that without GC none of those blocks should get accepted.
        let blocks_ranges = vec![7..=8 as Round, 9..=10 as Round];

        for rounds_range in blocks_ranges {
            let all_blocks = dag_builder
                .blocks
                .values()
                .filter(|block| rounds_range.contains(&block.round()))
                .cloned()
                .collect::<Vec<_>>();

            // WHEN
            let mut reversed_blocks = all_blocks.clone();
            reversed_blocks.sort_by_key(|b| std::cmp::Reverse(b.reference()));
            let (mut accepted_blocks, missing) = block_manager.try_accept_blocks(reversed_blocks);
            accepted_blocks.sort_by_key(|a| a.reference());

            // THEN
            assert_eq!(accepted_blocks, all_blocks.to_vec());
            assert!(missing.is_empty());
            assert!(block_manager.is_empty());

            let (accepted_blocks, _) = block_manager.try_accept_blocks(all_blocks);
            assert!(accepted_blocks.is_empty());
        }
    }

    /// Blocks that are attempted to be accepted but are <= gc_round they will be skipped for processing. Nothing
    /// should be stored or trigger any unsuspension etc.
    #[tokio::test]
    async fn skip_accepting_blocks_below_gc_round() {
        // GIVEN
        let (mut context, _key_pairs) = Context::new_for_test(4);
        // We set the gc depth to 4
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        // We "fake" the commit for round 10, so we can test the GC round 6 (commit_round - gc_depth = 10 - 4 = 6)
        let last_commit = TrustedCommit::new_for_test(
            10,
            CommitDigest::MIN,
            context.clock.timestamp_utc_ms(),
            BlockRef::new(10, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            vec![],
        );
        dag_state.write().set_last_commit(last_commit);
        assert_eq!(
            dag_state.read().gc_round(),
            6,
            "GC round should have moved to round 6"
        );

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 6 rounds
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=6).build();

        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

        // WHEN
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(all_blocks.clone());

        // THEN
        assert!(accepted_blocks.is_empty());
        assert!(missing.is_empty());
        assert!(block_manager.is_empty());
    }

    /// The test generate blocks for a well connected DAG and feed them to block manager in random order. In the end all the
    /// blocks should be uniquely suspended and no missing blocks should exist. We set a high gc_depth value so in this test gc_round will be 0.
    #[tokio::test]
    async fn accept_blocks_unsuspend_children_blocks() {
        // GIVEN
        let (mut context, _key_pairs) = Context::new_for_test(4);
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(10);

        let context = Arc::new(context);

        // create a DAG of rounds 1 ~ 3
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=3).build();

        let mut all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

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
                let (accepted_blocks, _) = block_manager.try_accept_blocks(vec![block.clone()]);

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
            assert!(block_manager.is_empty());
        }
    }

    #[rstest]
    #[tokio::test]
    async fn unsuspend_blocks_for_latest_gc_round(#[values(5, 10, 14)] gc_depth: u32) {
        telemetry_subscribers::init_for_testing();
        // GIVEN
        let (mut context, _key_pairs) = Context::new_for_test(4);
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(gc_depth);

        let context = Arc::new(context);

        // create a DAG of rounds 1 ~ gc_depth * 2
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=gc_depth * 2).build();

        // Pay attention that we start from round 2. Round 1 will always be missing so no matter what we do we can't unsuspend it unless
        // gc_round has advanced to round >= 1.
        let mut all_blocks = dag_builder
            .blocks
            .values()
            .filter(|block| block.round() > 1)
            .cloned()
            .collect::<Vec<_>>();

        // Now randomize the sequence of sending the blocks to block manager. In the end all the blocks should be uniquely
        // suspended and no missing blocks should exist.
        for seed in 0..100u8 {
            all_blocks.shuffle(&mut StdRng::from_seed([seed; 32]));

            let store = Arc::new(MemStore::new());
            let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

            let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

            // WHEN
            for block in &all_blocks {
                let (accepted_blocks, _) = block_manager.try_accept_blocks(vec![block.clone()]);
                assert!(accepted_blocks.is_empty());
            }
            assert!(!block_manager.is_empty());

            // AND also call the try_to_find method with some non existing block refs. Those should be cleaned up as well once GC kicks in.
            let non_existing_refs = (1..=3)
                .map(|round| {
                    BlockRef::new(round, AuthorityIndex::new_for_test(0), BlockDigest::MIN)
                })
                .collect::<Vec<_>>();
            assert_eq!(block_manager.try_find_blocks(non_existing_refs).len(), 3);

            // AND
            // Trigger a commit which will advance GC round
            let last_commit = TrustedCommit::new_for_test(
                gc_depth * 2,
                CommitDigest::MIN,
                context.clock.timestamp_utc_ms(),
                BlockRef::new(
                    gc_depth * 2,
                    AuthorityIndex::new_for_test(0),
                    BlockDigest::MIN,
                ),
                vec![],
            );
            dag_state.write().set_last_commit(last_commit);

            // AND
            block_manager.try_unsuspend_blocks_for_latest_gc_round();

            // THEN
            assert!(block_manager.is_empty());

            // AND ensure that all have been accepted to the DAG
            for block in &all_blocks {
                assert!(dag_state.read().contains_block(&block.reference()));
            }
        }
    }

    #[rstest]
    #[tokio::test]
    async fn try_accept_committed_blocks() {
        // GIVEN
        let (mut context, _key_pairs) = Context::new_for_test(4);
        // We set the gc depth to 4
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        // We "fake" the commit for round 6, so GC round moves to (commit_round - gc_depth = 6 - 4 = 2)
        let last_commit = TrustedCommit::new_for_test(
            10,
            CommitDigest::MIN,
            context.clock.timestamp_utc_ms(),
            BlockRef::new(6, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            vec![],
        );
        dag_state.write().set_last_commit(last_commit);
        assert_eq!(
            dag_state.read().gc_round(),
            2,
            "GC round should have moved to round 2"
        );

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG of 12 rounds
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=12).build();

        // Now try to accept via the normal acceptance block path the blocks of rounds 7 ~ 12. None of them should be accepted
        let blocks = dag_builder.blocks(7..=12);
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(blocks.clone());
        assert!(accepted_blocks.is_empty());
        assert_eq!(missing.len(), 4);

        // Now try to accept via the committed blocks path the blocks of rounds 3 ~ 6. All of them should be accepted and also the blocks
        // of rounds 7 ~ 12 should be unsuspended and accepted as well.
        let blocks = dag_builder.blocks(3..=6);

        // WHEN
        let mut accepted_blocks = block_manager.try_accept_committed_blocks(blocks);

        // THEN
        accepted_blocks.sort_by_key(|b| b.reference());

        let mut all_blocks = dag_builder.blocks(3..=12);
        all_blocks.sort_by_key(|b| b.reference());

        assert_eq!(accepted_blocks, all_blocks);
        assert!(block_manager.is_empty());
    }

    #[tokio::test]
    async fn try_find_blocks() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state);

        // create a DAG
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=2) // 2 rounds
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(2),
            ]) // Create equivocating blocks for 2 authorities
            .equivocate(3)
            .build();

        // Take only the blocks of round 2 and try to accept them
        let round_2_blocks = dag_builder
            .blocks
            .iter()
            .filter_map(|(_, block)| (block.round() == 2).then_some(block.clone()))
            .collect::<Vec<VerifiedBlock>>();

        // All blocks should be missing
        let missing_block_refs_from_find =
            block_manager.try_find_blocks(round_2_blocks.iter().map(|b| b.reference()).collect());
        assert_eq!(missing_block_refs_from_find.len(), 10);
        assert!(missing_block_refs_from_find
            .iter()
            .all(|block_ref| block_ref.round == 2));

        // Try accept blocks which will cause blocks to be suspended and added to missing
        // in block manager.
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(round_2_blocks.clone());
        assert!(accepted_blocks.is_empty());

        let missing_block_refs = round_2_blocks.first().unwrap().ancestors();
        let missing_block_refs_from_accept =
            missing_block_refs.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(missing, missing_block_refs_from_accept);
        assert_eq!(
            block_manager.missing_blocks(),
            missing_block_refs_from_accept
        );

        // No blocks should be accepted and block manager should have made note
        // of the missing & suspended blocks.
        // Now we can check get the result of try find block with all of the blocks
        // from newly created but not accepted round 3.
        dag_builder.layer(3).build();

        let round_3_blocks = dag_builder
            .blocks
            .iter()
            .filter_map(|(_, block)| (block.round() == 3).then_some(block.reference()))
            .collect::<Vec<BlockRef>>();

        let missing_block_refs_from_find = block_manager.try_find_blocks(
            round_2_blocks
                .iter()
                .map(|b| b.reference())
                .chain(round_3_blocks.into_iter())
                .collect(),
        );

        assert_eq!(missing_block_refs_from_find.len(), 4);
        assert!(missing_block_refs_from_find
            .iter()
            .all(|block_ref| block_ref.round == 3));
        assert_eq!(
            block_manager.missing_blocks(),
            missing_block_refs_from_accept
                .into_iter()
                .chain(missing_block_refs_from_find.into_iter())
                .collect()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_verify_block_timestamps_and_accept() {
        telemetry_subscribers::init_for_testing();
        let (context, _key_pairs) = Context::new_for_test(4);

        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

        // create a DAG where authority 0 timestamp is always higher than the others.
        let mut dag_builder = DagBuilder::new(context.clone());
        let authorities = context
            .committee
            .authorities()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        dag_builder
            .layers(1..=1)
            .authorities(authorities.clone())
            .with_timestamps(vec![1000, 500, 550, 580])
            .build();
        dag_builder
            .layers(2..=2)
            .authorities(authorities.clone())
            .with_timestamps(vec![2000, 600, 650, 680])
            .build();
        dag_builder
            .layers(3..=3)
            .authorities(authorities)
            .with_timestamps(vec![3000, 700, 750, 780])
            .build();

        // take all the blocks and try to accept them.
        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

        // All blocks should get accepted
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(all_blocks.clone());

        // If the median based timestamp is enabled then all the blocks should be accepted
        assert_eq!(all_blocks, accepted_blocks);
        assert!(missing.is_empty());
    }
}

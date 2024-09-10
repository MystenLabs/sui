// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    iter,
    sync::Arc,
    time::Instant,
};

use itertools::Itertools as _;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use tracing::{debug, trace, warn};

use crate::{
    block::{BlockAPI, BlockRef, VerifiedBlock},
    block_verifier::BlockVerifier,
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
    block_verifier: Arc<dyn BlockVerifier>,

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
    pub(crate) fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        block_verifier: Arc<dyn BlockVerifier>,
    ) -> Self {
        let committee_size = context.committee.size();
        Self {
            context,
            dag_state,
            block_verifier,
            suspended_blocks: BTreeMap::new(),
            missing_ancestors: BTreeMap::new(),
            missing_blocks: BTreeSet::new(),
            received_block_rounds: vec![None; committee_size],
        }
    }

    /// Tries to accept the provided blocks assuming that all their causal history exists. The method
    /// returns all the blocks that have been successfully processed in round ascending order, that includes also previously
    /// suspended blocks that have now been able to get accepted. Method also returns a set with the missing ancestor blocks.
    pub(crate) fn try_accept_blocks(
        &mut self,
        mut blocks: Vec<VerifiedBlock>,
    ) -> (Vec<VerifiedBlock>, BTreeSet<BlockRef>) {
        let _s = monitored_scope("BlockManager::try_accept_blocks");

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
            let block = match self.try_accept_one_block(block) {
                TryAcceptResult::Accepted(block) => block,
                TryAcceptResult::Suspended(ancestors_to_fetch) => {
                    trace!(
                        "Missing ancestors for block {block_ref}: {}",
                        ancestors_to_fetch.iter().map(|b| b.to_string()).join(",")
                    );
                    missing_blocks.extend(ancestors_to_fetch);
                    continue;
                }
                TryAcceptResult::Processed => continue,
            };

            // If the block is accepted, try to unsuspend its children blocks if any.
            let unsuspended_blocks = self.try_unsuspend_children_blocks(&block);

            // Try to verify the block and its children for timestamp, with ancestor blocks.
            let mut blocks_to_accept: BTreeMap<BlockRef, VerifiedBlock> = BTreeMap::new();
            let mut blocks_to_reject: BTreeMap<BlockRef, VerifiedBlock> = BTreeMap::new();
            {
                'block: for b in iter::once(block).chain(unsuspended_blocks) {
                    let ancestors = self.dag_state.read().get_blocks(b.ancestors());
                    assert_eq!(b.ancestors().len(), ancestors.len());
                    let mut ancestor_blocks = vec![];
                    'ancestor: for (ancestor_ref, found) in
                        b.ancestors().iter().zip(ancestors.into_iter())
                    {
                        if let Some(found_block) = found {
                            // This invariant should be guaranteed by DagState.
                            assert_eq!(ancestor_ref, &found_block.reference());
                            ancestor_blocks.push(found_block);
                            continue 'ancestor;
                        }
                        // blocks_to_accept have not been added to DagState yet, but they
                        // can appear in ancestors.
                        if blocks_to_accept.contains_key(ancestor_ref) {
                            ancestor_blocks.push(blocks_to_accept[ancestor_ref].clone());
                            continue 'ancestor;
                        }
                        // If an ancestor is already rejected, reject this block as well.
                        if blocks_to_reject.contains_key(ancestor_ref) {
                            blocks_to_reject.insert(b.reference(), b);
                            continue 'block;
                        }
                        panic!("Unsuspended block {:?} has a missing ancestor! Ancestor not found in DagState: {:?}", b, ancestor_ref);
                    }
                    if let Err(e) = self.block_verifier.check_ancestors(&b, &ancestor_blocks) {
                        warn!("Block {:?} failed to verify ancestors: {}", b, e);
                        blocks_to_reject.insert(b.reference(), b);
                    } else {
                        blocks_to_accept.insert(b.reference(), b);
                    }
                }
            }
            for (block_ref, block) in blocks_to_reject {
                self.context
                    .metrics
                    .node_metrics
                    .invalid_blocks
                    .with_label_values(&[&block_ref.author.to_string(), "accept_block"])
                    .inc();
                warn!("Invalid block {:?} is rejected", block);
            }

            // TODO: report blocks_to_reject to peers.

            // Insert the accepted blocks into DAG state so future blocks including them as
            // ancestors do not get suspended.
            let blocks_to_accept: Vec<_> = blocks_to_accept.into_values().collect();
            self.dag_state
                .write()
                .accept_blocks(blocks_to_accept.clone());

            accepted_blocks.extend(blocks_to_accept);
        }

        let metrics = &self.context.metrics.node_metrics;
        metrics
            .missing_blocks_total
            .inc_by(missing_blocks.len() as u64);
        metrics
            .block_manager_suspended_blocks
            .set(self.suspended_blocks.len() as i64);
        metrics
            .block_manager_missing_ancestors
            .set(self.missing_ancestors.len() as i64);
        metrics
            .block_manager_missing_blocks
            .set(self.missing_blocks.len() as i64);

        // Figure out the new missing blocks
        (accepted_blocks, missing_blocks)
    }

    /// Tries to accept the provided block. To accept a block its ancestors must have been already successfully accepted. If
    /// block is accepted then Some result is returned. None is returned when either the block is suspended or the block
    /// has been already accepted before.
    fn try_accept_one_block(&mut self, block: VerifiedBlock) -> TryAcceptResult {
        let block_ref = block.reference();
        let mut missing_ancestors = BTreeSet::new();
        let mut ancestors_to_fetch = BTreeSet::new();
        let dag_state = self.dag_state.read();

        // If block has been already received and suspended, or already processed and stored, or is a genesis block, then skip it.
        if self.suspended_blocks.contains_key(&block_ref) || dag_state.contains_block(&block_ref) {
            return TryAcceptResult::Processed;
        }

        let ancestors = block.ancestors();

        // make sure that we have all the required ancestors in store
        for (found, ancestor) in dag_state
            .contains_blocks(ancestors.to_vec())
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
                        to_process_blocks.push(block.block.clone());
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

    /// Returns all the blocks that are currently missing and needed in order to accept suspended
    /// blocks.
    pub(crate) fn missing_blocks(&self) -> BTreeSet<BlockRef> {
        self.missing_blocks.clone()
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
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use consensus_config::AuthorityIndex;
    use parking_lot::RwLock;
    use rand::{prelude::StdRng, seq::SliceRandom, SeedableRng};

    use crate::{
        block::{BlockAPI, BlockRef, SignedBlock, VerifiedBlock},
        block_manager::BlockManager,
        block_verifier::{BlockVerifier, NoopBlockVerifier},
        context::Context,
        dag_state::DagState,
        error::{ConsensusError, ConsensusResult},
        storage::mem_store::MemStore,
        test_dag_builder::DagBuilder,
    };

    #[tokio::test]
    async fn suspend_blocks_with_missing_ancestors() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let mut block_manager =
            BlockManager::new(context.clone(), dag_state, Arc::new(NoopBlockVerifier));

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

        let mut block_manager =
            BlockManager::new(context.clone(), dag_state, Arc::new(NoopBlockVerifier));

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

        let mut block_manager =
            BlockManager::new(context.clone(), dag_state, Arc::new(NoopBlockVerifier));

        // create a DAG of 2 rounds
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=2).build();

        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

        // WHEN
        let (accepted_blocks, missing) = block_manager.try_accept_blocks(all_blocks.clone());

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
        assert!(missing.is_empty());
        assert!(block_manager.is_empty());

        // WHEN trying to accept same blocks again, then none will be returned as those have been already accepted
        let (accepted_blocks, _) = block_manager.try_accept_blocks(all_blocks);
        assert!(accepted_blocks.is_empty());
    }

    #[tokio::test]
    async fn accept_blocks_unsuspend_children_blocks() {
        // GIVEN
        let (context, _key_pairs) = Context::new_for_test(4);
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

            let mut block_manager =
                BlockManager::new(context.clone(), dag_state, Arc::new(NoopBlockVerifier));

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

    struct TestBlockVerifier {
        fail: BTreeSet<BlockRef>,
    }

    impl TestBlockVerifier {
        fn new(fail: BTreeSet<BlockRef>) -> Self {
            Self { fail }
        }
    }

    impl BlockVerifier for TestBlockVerifier {
        fn verify(&self, _block: &SignedBlock) -> ConsensusResult<()> {
            Ok(())
        }

        fn check_ancestors(
            &self,
            block: &VerifiedBlock,
            _ancestors: &[VerifiedBlock],
        ) -> ConsensusResult<()> {
            if self.fail.contains(&block.reference()) {
                Err(ConsensusError::InvalidBlockTimestamp {
                    max_timestamp_ms: 0,
                    block_timestamp_ms: block.timestamp_ms(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn reject_blocks_failing_verifications() {
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);

        // create a DAG of rounds 1 ~ 5.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=5).build();

        let all_blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();

        // Create a test verifier that fails the blocks of round 3
        let test_verifier = TestBlockVerifier::new(
            all_blocks
                .iter()
                .filter(|block| block.round() == 3)
                .map(|block| block.reference())
                .collect(),
        );

        // Create BlockManager.
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let mut block_manager =
            BlockManager::new(context.clone(), dag_state, Arc::new(test_verifier));

        // Try to accept blocks from round 2 ~ 5 into block manager. All of them should be suspended.
        let (accepted_blocks, missing_refs) = block_manager.try_accept_blocks(
            all_blocks
                .iter()
                .filter(|block| block.round() > 1)
                .cloned()
                .collect(),
        );

        // Missing refs should all come from round 1.
        assert!(accepted_blocks.is_empty());
        assert_eq!(missing_refs.len(), 4);
        missing_refs.iter().for_each(|missing_ref| {
            assert_eq!(missing_ref.round, 1);
        });

        // Now add round 1 blocks into block manager.
        let (accepted_blocks, missing_refs) = block_manager.try_accept_blocks(
            all_blocks
                .iter()
                .filter(|block| block.round() == 1)
                .cloned()
                .collect(),
        );

        // Only round 1 and round 2 blocks should be accepted.
        assert_eq!(accepted_blocks.len(), 8);
        accepted_blocks.iter().for_each(|block| {
            assert!(block.round() <= 2);
        });
        assert!(missing_refs.is_empty());

        // Other blocks should be rejected and there should be no remaining suspended block.
        assert!(block_manager.suspended_blocks().is_empty());
    }
}

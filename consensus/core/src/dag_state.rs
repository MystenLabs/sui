// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Bound::{Excluded, Included},
    sync::Arc,
};

use consensus_config::AuthorityIndex;

use crate::{
    block::{BlockAPI, BlockDigest, BlockRef, Round, Slot, VerifiedBlock},
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

    /// Gets an uncommitted block. Returns None if not found.
    /// Uncommitted blocks must exist in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_block(&self, reference: &BlockRef) -> Option<VerifiedBlock> {
        self.recent_blocks.get(reference).cloned()
    }

    /// Gets all uncommitted blocks in a slot.
    /// Uncommitted blocks must exist in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_blocks_at_slot(&self, slot: Slot) -> Vec<VerifiedBlock> {
        let mut blocks = vec![];
        for (block_ref, block) in self.recent_blocks.range((
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MIN)),
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MAX)),
        )) {
            blocks.push(block.clone())
        }
        blocks
    }

    pub(crate) fn get_uncommitted_blocks_at_round(&self, round: Round) -> Vec<VerifiedBlock> {
        if round < self.round_lower_bound() {
            panic!("Round {} blocks may not be cached in memory!", round);
        }

        let mut blocks = vec![];
        for (block_ref, block) in self.recent_blocks.range((
            Included(BlockRef::new(round, AuthorityIndex::ZERO, BlockDigest::MIN)),
            Excluded(BlockRef::new(
                round + 1,
                AuthorityIndex::ZERO,
                BlockDigest::MIN,
            )),
        )) {
            blocks.push(block.clone())
        }
        blocks
    }

    pub(crate) fn ancestors_at_uncommitted_round(
        &self,
        later_block: &VerifiedBlock,
        earlier_round: Round,
    ) -> Vec<VerifiedBlock> {
        if earlier_round < self.round_lower_bound() {
            panic!(
                "Round {} blocks may not be cached in memory!",
                earlier_round
            );
        }
        if earlier_round >= later_block.round() {
            panic!(
                "Round {} is not earlier than block {}!",
                earlier_round,
                later_block.reference()
            );
        }

        let mut linked: BTreeSet<BlockRef> = later_block.ancestors().iter().cloned().collect();
        let mut round = later_block.round() - 1;
        while round > earlier_round {
            let mut next_linked = BTreeSet::new();
            for r in linked.into_iter() {
                let block = self
                    .recent_blocks
                    .get(&r)
                    .unwrap_or_else(|| panic!("Block {:?} not found!", r));
                next_linked.extend(block.ancestors().iter().cloned());
            }
            linked = next_linked;
            round -= 1;
        }
        linked
            .into_iter()
            .map(|r| {
                self.recent_blocks
                    .get(&r)
                    .unwrap_or_else(|| panic!("Block {:?} not found!", r))
                    .clone()
            })
            .collect()
    }

    /// Lowest round where all known blocks are cached in memory.
    fn round_lower_bound(&self) -> Round {
        match &self.last_commit {
            Some(commit) => commit.leader.round.saturating_sub(CACHED_ROUNDS),
            None => 0,
        }
    }
}

#[cfg(test)]
mod test {
    use std::vec;

    use super::*;
    use crate::{
        block::{BlockDigest, BlockRef, BlockTimestampMs, TestBlock, VerifiedBlock},
        storage::mem_store::MemStore,
    };

    #[test]
    fn get_unncommitted_blocks() {
        let context = Arc::new(Context::new_for_test());
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());

        // Populate test blocks for round 1 ~ 10, authorities 0 ~ 2.
        let num_rounds: u32 = 10;
        let non_existent_round: u32 = 100;
        let num_authorities: u32 = 3;
        let num_blocks_per_slot: usize = 3;
        let mut blocks = BTreeMap::new();
        for round in 1..=num_rounds {
            for author in 0..num_authorities {
                // Create 3 blocks per slot, with different timestamps and digests.
                let base_ts = round as BlockTimestampMs * 1000;
                for timestamp in base_ts..base_ts + num_blocks_per_slot as u64 {
                    let block = VerifiedBlock::new_for_test(
                        TestBlock::new(round, author)
                            .set_timestamp_ms(timestamp)
                            .build(),
                    );
                    dag_state.accept_block(block.clone());
                    blocks.insert(block.reference(), block);
                }
            }
        }

        // Check uncommitted blocks that exist.
        for (r, block) in &blocks {
            assert_eq!(dag_state.get_uncommitted_block(r), Some(block.clone()));
        }

        // Check uncommitted blocks that do not exist.
        let last_ref = blocks.keys().last().unwrap();
        assert!(dag_state
            .get_uncommitted_block(&BlockRef::new(
                last_ref.round,
                last_ref.author,
                BlockDigest::MIN
            ))
            .is_none());

        // Check slots with uncommitted blocks.
        for round in 1..=num_rounds {
            for author in 0..num_authorities {
                let slot = Slot::new(
                    round,
                    context
                        .committee
                        .to_authority_index(author as usize)
                        .unwrap(),
                );
                let blocks = dag_state.get_uncommitted_blocks_at_slot(slot);
                assert_eq!(blocks.len(), num_blocks_per_slot);
                for b in blocks {
                    assert_eq!(b.round(), round);
                    assert_eq!(
                        b.author(),
                        context
                            .committee
                            .to_authority_index(author as usize)
                            .unwrap()
                    );
                }
            }
        }

        // Check slots without uncommitted blocks.
        let slot = Slot::new(non_existent_round, AuthorityIndex::ZERO);
        assert!(dag_state.get_uncommitted_blocks_at_slot(slot).is_empty());

        // Check rounds with uncommitted blocks.
        for round in 1..=num_rounds {
            let blocks = dag_state.get_uncommitted_blocks_at_round(round);
            assert_eq!(blocks.len(), num_authorities as usize * num_blocks_per_slot);
            for b in blocks {
                assert_eq!(b.round(), round);
            }
        }

        // Check rounds without uncommitted blocks.
        assert!(dag_state
            .get_uncommitted_blocks_at_round(non_existent_round)
            .is_empty());
    }

    #[test]
    fn ancestors_at_uncommitted_round() {
        let context = Arc::new(Context::new_for_test());
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());

        // Populate a dag of blocks.

        // block_1_0 will be connected to anchor.
        let block_1_0 =
            VerifiedBlock::new_for_test(TestBlock::new(1, 0).set_timestamp_ms(100).build());
        // Slot(1, 1) has 2 blocks. Only block_1_1 will be connected to anchor.
        let block_1_1 =
            VerifiedBlock::new_for_test(TestBlock::new(1, 1).set_timestamp_ms(110).build());
        let block_1_1_1 =
            VerifiedBlock::new_for_test(TestBlock::new(1, 1).set_timestamp_ms(111).build());
        // block_1_2 will not be connected to anchor.
        let block_1_2 =
            VerifiedBlock::new_for_test(TestBlock::new(1, 2).set_timestamp_ms(120).build());
        // block_1_3 will be connected to anchor.
        let block_1_3 =
            VerifiedBlock::new_for_test(TestBlock::new(1, 3).set_timestamp_ms(130).build());
        let round_1 = vec![
            block_1_0.clone(),
            block_1_1.clone(),
            block_1_1_1.clone(),
            block_1_2.clone(),
            block_1_3.clone(),
        ];
        let round_1_ancestors = vec![
            block_1_0.reference(),
            block_1_1.reference(),
            block_1_3.reference(),
        ];
        let round_2 = vec![
            VerifiedBlock::new_for_test(
                TestBlock::new(2, 0)
                    .set_timestamp_ms(200)
                    .set_ancestors(round_1_ancestors.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(2, 2)
                    .set_timestamp_ms(220)
                    .set_ancestors(round_1_ancestors.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(2, 3)
                    .set_timestamp_ms(230)
                    .set_ancestors(round_1_ancestors.clone())
                    .build(),
            ),
        ];
        let round_2_ancestors = round_2.iter().map(|b| b.reference()).collect();
        let anchor = VerifiedBlock::new_for_test(
            TestBlock::new(3, 1)
                .set_timestamp_ms(310)
                .set_ancestors(round_2_ancestors)
                .build(),
        );

        // Add all blocks to DagState.
        for b in round_1
            .iter()
            .chain(round_2.iter())
            .chain([anchor.clone()].iter())
        {
            dag_state.accept_block(b.clone());
        }

        // Check ancestors connected to anchor.
        let ancestors = dag_state.ancestors_at_uncommitted_round(&anchor, 1);
        let mut ancestors_refs: Vec<BlockRef> = ancestors.iter().map(|b| b.reference()).collect();
        ancestors_refs.sort();
        assert_eq!(
            ancestors_refs, round_1_ancestors,
            "Expected round 1 ancestors: {:?}. Got: {:?}",
            round_1_ancestors, ancestors_refs
        );
    }
}

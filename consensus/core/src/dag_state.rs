// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet},
    ops::Bound::{Excluded, Included, Unbounded},
    panic,
    sync::Arc,
};

use consensus_config::AuthorityIndex;

use crate::{
    block::{Block, BlockAPI, BlockDigest, BlockRef, Round, Slot, VerifiedBlock},
    commit::{Commit, CommitIndex},
    context::Context,
    error::ConsensusResult,
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

    // Highest round of blocks accepted.
    highest_accepted_round: Round,

    // Persistent storage for blocks, commits and other consensus data.
    store: Arc<dyn Store>,

    // The genesis blocks
    genesis: BTreeMap<BlockRef, VerifiedBlock>,
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

        let (_, genesis) = Block::genesis(context.clone());
        let genesis = genesis
            .into_iter()
            .map(|block| (block.reference(), block))
            .collect();

        let mut state = Self {
            context,
            recent_blocks: BTreeMap::new(),
            cached_refs: vec![BTreeSet::new(); num_authorities],
            last_commit,
            highest_accepted_round: 0,
            store,
            genesis,
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
        let block_round = block.round();

        // TODO: Move this check to core
        // Ensure we don't write multiple blocks per slot for our own index
        if block_ref.author == self.context.own_index {
            let existing_blocks = self.get_uncommitted_blocks_at_slot(block_ref.into());
            assert!(
                existing_blocks.is_empty(),
                "Block Rejected! Attempted to add block {block} to own slot where \
                block(s) {existing_blocks:#?} already exists."
            );
        }
        self.recent_blocks.insert(block_ref, block);
        self.cached_refs[block_ref.author].insert(block_ref);
        self.highest_accepted_round = max(self.highest_accepted_round, block_round);
    }

    /// Accepts a blocks into DagState and keeps it in memory.
    pub(crate) fn accept_blocks(&mut self, blocks: Vec<VerifiedBlock>) {
        for block in blocks {
            self.accept_block(block);
        }
    }

    /// Gets a copy of an uncommitted block. Returns None if not found.
    /// Uncommitted blocks must exist in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_block(&self, reference: &BlockRef) -> Option<VerifiedBlock> {
        self.recent_blocks.get(reference).cloned()
    }

    /// Gets a copy of the uncommitted blocks. Returns None for each block not found.
    /// Uncommitted blocks must exist in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_blocks(
        &self,
        references: Vec<BlockRef>,
    ) -> Vec<Option<VerifiedBlock>> {
        references
            .into_iter()
            .map(|reference| self.recent_blocks.get(&reference).cloned())
            .collect()
    }

    /// Gets all uncommitted blocks in a slot.
    /// Uncommitted blocks must exist in memory, so only in-memory blocks are checked.
    pub(crate) fn get_uncommitted_blocks_at_slot(&self, slot: Slot) -> Vec<VerifiedBlock> {
        // TODO: evauluate if we should panic `if slot.round <= last_commit_round`
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
        if round <= self.last_commit_round() {
            panic!("Round {} have committed blocks!", round);
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

    /// Gets all ancestors in the history of a block at a certain round.
    /// The round must be higher than the last committed round.
    pub(crate) fn ancestors_at_uncommitted_round(
        &self,
        later_block: &VerifiedBlock,
        earlier_round: Round,
    ) -> Vec<VerifiedBlock> {
        if earlier_round <= self.last_commit_round() {
            panic!("Round {} have committed blocks!", earlier_round);
        }
        if earlier_round >= later_block.round() {
            panic!(
                "Round {} is not earlier than block {}!",
                earlier_round,
                later_block.reference()
            );
        }

        // Use BTreeSet to iterate through ancestors of later_block in round desc order.
        let mut linked: BTreeSet<BlockRef> = later_block.ancestors().iter().cloned().collect();
        while !linked.is_empty() {
            let round = linked.last().unwrap().round;
            // Stop after finishing traversal for ancestors above earlier_round.
            if round <= earlier_round {
                break;
            }
            let block_ref = linked.pop_last().unwrap();
            let Some(block) = self.recent_blocks.get(&block_ref) else {
                panic!("Block {:?} should be available in memory!", block_ref);
            };
            linked.extend(block.ancestors().iter().cloned());
        }
        linked
            .range((
                Included(BlockRef::new(
                    earlier_round,
                    AuthorityIndex::ZERO,
                    BlockDigest::MIN,
                )),
                Unbounded,
            ))
            .map(|r| {
                self.recent_blocks
                    .get(r)
                    .unwrap_or_else(|| panic!("Block {:?} should be available in memory!", r))
                    .clone()
            })
            .collect()
    }

    /// Returns the requested blocks by looking into the cache first, and for any non found block into the store.
    pub(crate) fn get_blocks(
        &self,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
        let mut blocks = vec![None; block_refs.len()];
        let mut missing = Vec::new();

        for (index, block_ref) in block_refs.into_iter().enumerate() {
            if let Some(block) = self.recent_blocks.get(&block_ref) {
                blocks[index] = Some(block.clone());
            } else {
                missing.push((index, block_ref));
            }
        }

        if missing.is_empty() {
            return Ok(blocks);
        }

        let missing_refs = missing
            .iter()
            .map(|(_, block_ref)| *block_ref)
            .collect::<Vec<_>>();
        let store_results = self.store.read_blocks(&missing_refs)?;

        for ((index, _), result) in missing.into_iter().zip(store_results.into_iter()) {
            blocks[index] = result;
        }

        Ok(blocks)
    }

    pub(crate) fn contains_block(&self, block_ref: &BlockRef) -> ConsensusResult<bool> {
        let blocks = self.contains_blocks(vec![*block_ref])?;
        Ok(blocks.first().cloned().expect("Result should be present"))
    }

    /// Checks whether the required blocks are in cache, if exist, or otherwise will check in store. The method is not caching
    /// back the results, so its expensive if keep asking for cache missing blocks.
    pub(crate) fn contains_blocks(&self, block_refs: Vec<BlockRef>) -> ConsensusResult<Vec<bool>> {
        let mut blocks = vec![false; block_refs.len()];
        let mut missing = Vec::new();

        for (index, block_ref) in block_refs.into_iter().enumerate() {
            if self.cached_refs[block_ref.author].contains(&block_ref)
                || self.genesis.contains_key(&block_ref)
            {
                blocks[index] = true;
            } else {
                missing.push((index, block_ref));
            }
        }

        if missing.is_empty() {
            return Ok(blocks);
        }

        let missing_refs = missing
            .iter()
            .map(|(_, block_ref)| *block_ref)
            .collect::<Vec<_>>();
        let store_results = self.store.contains_blocks(&missing_refs)?;

        for ((index, _), result) in missing.into_iter().zip(store_results.into_iter()) {
            blocks[index] = result;
        }

        Ok(blocks)
    }

    pub(crate) fn highest_accepted_round(&self) -> Round {
        self.highest_accepted_round
    }

    /// Index of the last commit.
    pub(crate) fn last_commit_index(&self) -> CommitIndex {
        match &self.last_commit {
            Some(commit) => commit.index,
            None => 0,
        }
    }

    /// Leader slot of the last commit.
    pub(crate) fn last_commit_leader(&self) -> Slot {
        match &self.last_commit {
            Some(commit) => commit.leader.into(),
            None => self
                .genesis
                .iter()
                .next()
                .map(|(genesis_ref, _)| *genesis_ref)
                .expect("Genesis blocks should always be available.")
                .into(),
        }
    }

    /// Last committed round per authority.
    pub(crate) fn last_committed_rounds(&self) -> Vec<Round> {
        match &self.last_commit {
            Some(commit) => commit.last_committed_rounds.clone(),
            None => vec![0; self.context.committee.size()],
        }
    }

    // Write commits to store. Commits should be provided in commit order, meaning
    // the last element in commits is the new last_commit.
    pub(crate) fn write_commits(
        &mut self,
        commits: Vec<Commit>,
        committed_blocks: Vec<VerifiedBlock>,
    ) {
        assert!(!commits.is_empty());
        let last_commit = commits.last().unwrap().clone();
        self.store
            .write(committed_blocks, commits)
            .expect("Writing commits to store should not fail");
        self.set_last_commit(last_commit);
    }

    pub(crate) fn set_last_commit(&mut self, commit: Commit) {
        if let Some(last_commit) = &self.last_commit {
            assert!(commit.index >= last_commit.index);
        }
        self.last_commit = Some(commit);
    }

    /// Highest round where a block is committed, which is last commit's leader round.
    fn last_commit_round(&self) -> Round {
        match &self.last_commit {
            Some(commit) => commit.leader.round,
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
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());
        let own_index = AuthorityIndex::new_for_test(0);

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

                    // Only write one block per slot for own index
                    if AuthorityIndex::new_for_test(author) == own_index {
                        break;
                    }
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

                // We only write one block per slot for own index
                if AuthorityIndex::new_for_test(author) == own_index {
                    assert_eq!(blocks.len(), 1);
                } else {
                    assert_eq!(blocks.len(), num_blocks_per_slot);
                }

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
            // Expect 3 blocks per authority except for own authority which should
            // have 1 block.
            assert_eq!(
                blocks.len(),
                (num_authorities - 1) as usize * num_blocks_per_slot + 1
            );
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
        // Initialize DagState.
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());

        // Populate DagState.

        // Round 10 refs will not have their blocks in DagState.
        let round_10_refs: Vec<_> = (0..4)
            .map(|a| {
                VerifiedBlock::new_for_test(TestBlock::new(10, a).set_timestamp_ms(1000).build())
                    .reference()
            })
            .collect();

        // Round 11 blocks.
        let round_11 = vec![
            // This will connect to round 12.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 0)
                    .set_timestamp_ms(1100)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
            // Slot(11, 1) has 3 blocks.
            // This will connect to round 12.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 1)
                    .set_timestamp_ms(1110)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
            // This will connect to round 13.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 1)
                    .set_timestamp_ms(1111)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
            // This will not connect to any block.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 1)
                    .set_timestamp_ms(1112)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
            // This will not connect to any block.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 2)
                    .set_timestamp_ms(1120)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
            // This will connect to round 12.
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 3)
                    .set_timestamp_ms(1130)
                    .set_ancestors(round_10_refs.clone())
                    .build(),
            ),
        ];

        // Round 12 blocks.
        let ancestors_for_round_12 = vec![
            round_11[0].reference(),
            round_11[1].reference(),
            round_11[5].reference(),
        ];
        let round_12 = vec![
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 0)
                    .set_timestamp_ms(1200)
                    .set_ancestors(ancestors_for_round_12.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 2)
                    .set_timestamp_ms(1220)
                    .set_ancestors(ancestors_for_round_12.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 3)
                    .set_timestamp_ms(1230)
                    .set_ancestors(ancestors_for_round_12.clone())
                    .build(),
            ),
        ];

        // Round 13 blocks.
        let ancestors_for_round_13 = vec![
            round_12[0].reference(),
            round_12[1].reference(),
            round_12[2].reference(),
            round_11[2].reference(),
        ];
        let round_13 = vec![
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 1)
                    .set_timestamp_ms(1300)
                    .set_ancestors(ancestors_for_round_13.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 2)
                    .set_timestamp_ms(1320)
                    .set_ancestors(ancestors_for_round_13.clone())
                    .build(),
            ),
            VerifiedBlock::new_for_test(
                TestBlock::new(12, 3)
                    .set_timestamp_ms(1330)
                    .set_ancestors(ancestors_for_round_13.clone())
                    .build(),
            ),
        ];

        // Round 14 anchor block.
        let ancestors_for_round_14 = round_13.iter().map(|b| b.reference()).collect();
        let anchor = VerifiedBlock::new_for_test(
            TestBlock::new(14, 1)
                .set_timestamp_ms(1410)
                .set_ancestors(ancestors_for_round_14)
                .build(),
        );

        // Add all blocks (at and above round 11) to DagState.
        for b in round_11
            .iter()
            .chain(round_12.iter())
            .chain(round_13.iter())
            .chain([anchor.clone()].iter())
        {
            dag_state.accept_block(b.clone());
        }

        // Check ancestors connected to anchor.
        let ancestors = dag_state.ancestors_at_uncommitted_round(&anchor, 11);
        let mut ancestors_refs: Vec<BlockRef> = ancestors.iter().map(|b| b.reference()).collect();
        ancestors_refs.sort();
        let mut expected_refs = vec![
            round_11[0].reference(),
            round_11[1].reference(),
            round_11[2].reference(),
            round_11[5].reference(),
        ];
        expected_refs.sort(); // we need to sort as blocks with same author and round of round 11 (position 1 & 2) might not be in right lexicographical order.
        assert_eq!(
            ancestors_refs, expected_refs,
            "Expected round 11 ancestors: {:?}. Got: {:?}",
            expected_refs, ancestors_refs
        );
    }

    #[test]
    fn test_contains_blocks_in_cache_or_store() {
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());

        // Create test blocks for round 1 ~ 10
        let num_rounds: u32 = 10;
        let num_authorities: u32 = 4;
        let mut blocks = Vec::new();

        for round in 1..=num_rounds {
            for author in 0..num_authorities {
                let block = VerifiedBlock::new_for_test(TestBlock::new(round, author).build());
                blocks.push(block);
            }
        }

        // Now write in store the blocks from first 4 rounds and the rest to the dag state
        blocks.clone().into_iter().for_each(|block| {
            if block.round() <= 4 {
                store.write(vec![block], vec![]).unwrap();
            } else {
                dag_state.accept_blocks(vec![block]);
            }
        });

        // Now when trying to query whether we have all the blocks, we should successfully retrieve a positive answer
        // where the blocks of first 4 round should be found in DagState and the rest in store.
        let mut block_refs = blocks
            .iter()
            .map(|block| block.reference())
            .collect::<Vec<_>>();
        let result = dag_state.contains_blocks(block_refs.clone()).unwrap();

        // Ensure everything is found
        let mut expected = vec![true; (num_rounds * num_authorities) as usize];
        assert_eq!(result, expected);

        // Now try to ask also for one block ref that is neither in cache nor in store
        block_refs.insert(
            3,
            BlockRef::new(11, AuthorityIndex::new_for_test(3), BlockDigest::default()),
        );
        let result = dag_state.contains_blocks(block_refs).unwrap();

        // Then all should be found apart from the last one
        expected.insert(3, false);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_get_blocks_in_cache_or_store() {
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let mut dag_state = DagState::new(context.clone(), store.clone());

        // Create test blocks for round 1 ~ 10
        let num_rounds: u32 = 10;
        let num_authorities: u32 = 4;
        let mut blocks = Vec::new();

        for round in 1..=num_rounds {
            for author in 0..num_authorities {
                let block = VerifiedBlock::new_for_test(TestBlock::new(round, author).build());
                blocks.push(block);
            }
        }

        // Now write in store the blocks from first 4 rounds and the rest to the dag state
        blocks.clone().into_iter().for_each(|block| {
            if block.round() <= 4 {
                store.write(vec![block], vec![]).unwrap();
            } else {
                dag_state.accept_blocks(vec![block]);
            }
        });

        // Now when trying to query whether we have all the blocks, we should successfully retrieve a positive answer
        // where the blocks of first 4 round should be found in DagState and the rest in store.
        let mut block_refs = blocks
            .iter()
            .map(|block| block.reference())
            .collect::<Vec<_>>();
        let result = dag_state.get_blocks(block_refs.clone()).unwrap();

        let mut expected = blocks
            .into_iter()
            .map(Some)
            .collect::<Vec<Option<VerifiedBlock>>>();

        // Ensure everything is found
        assert_eq!(result, expected.clone());

        // Now try to ask also for one block ref that is neither in cache nor in store
        block_refs.insert(
            3,
            BlockRef::new(11, AuthorityIndex::new_for_test(3), BlockDigest::default()),
        );
        let result = dag_state.get_blocks(block_refs).unwrap();

        // Then all should be found apart from the last one
        expected.insert(3, None);
        assert_eq!(result, expected);
    }
}

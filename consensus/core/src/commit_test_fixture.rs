// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test fixture for commit-related tests.
//! Used by both commit_finalizer.rs tests and randomized_tests.rs.

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Stake};
#[cfg(msim)]
use consensus_types::block::BlockRef;
use consensus_types::block::{Round, TransactionIndex};
use mysten_metrics::monitored_mpsc::unbounded_channel;
use parking_lot::RwLock;
#[cfg(msim)]
use rand::prelude::SliceRandom;
use rand::{Rng, rngs::StdRng};

#[cfg(msim)]
use crate::Transaction;
#[cfg(msim)]
use crate::block::{BlockTransactionVotes, TestBlock, genesis_blocks};
use crate::{
    block::{BlockAPI, VerifiedBlock},
    block_manager::BlockManager,
    block_verifier::NoopBlockVerifier,
    commit::{CommittedSubDag, DecidedLeader},
    commit_finalizer::CommitFinalizer,
    context::Context,
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
    linearizer::Linearizer,
    storage::mem_store::MemStore,
    transaction_certifier::TransactionCertifier,
    universal_committer::{
        UniversalCommitter, universal_committer_builder::UniversalCommitterBuilder,
    },
};

/// A test fixture that provides all the components needed for testing commit processing,
/// similar to the actual logic in Core::try_commit() and CommitFinalizer::run().
pub struct CommitTestFixture {
    pub context: Arc<Context>,
    pub linearizer: Linearizer,
    pub transaction_certifier: TransactionCertifier,
    pub commit_finalizer: CommitFinalizer,

    dag_state: Arc<RwLock<DagState>>,
    block_manager: BlockManager,
    committer: UniversalCommitter,
}

impl CommitTestFixture {
    /// Creates a new CommitTestFixture from a context.
    pub fn new(context: Arc<Context>) -> Self {
        let leader_schedule = Arc::new(LeaderSchedule::new(
            context.clone(),
            LeaderSwapTable::default(),
        ));
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));

        // Create committer with pipelining and only 1 leader per leader round
        let committer =
            UniversalCommitterBuilder::new(context.clone(), leader_schedule, dag_state.clone())
                .with_pipeline(true)
                .build();

        let block_manager = BlockManager::new(context.clone(), dag_state.clone());

        let linearizer = Linearizer::new(context.clone(), dag_state.clone());
        let (blocks_sender, _blocks_receiver) = unbounded_channel("consensus_block_output");
        let transaction_certifier = TransactionCertifier::new(
            context.clone(),
            Arc::new(NoopBlockVerifier {}),
            dag_state.clone(),
            blocks_sender,
        );
        let (commit_sender, _commit_receiver) = unbounded_channel("consensus_commit_output");
        let commit_finalizer = CommitFinalizer::new(
            context.clone(),
            dag_state.clone(),
            transaction_certifier.clone(),
            commit_sender,
        );

        Self {
            context,
            linearizer,
            transaction_certifier,
            commit_finalizer,
            dag_state,
            block_manager,
            committer,
        }
    }

    /// Creates a new CommitTestFixture with more options.
    pub fn with_options(
        num_authorities: usize,
        authority_index: u32,
        gc_depth: Option<u32>,
    ) -> Self {
        Self::new(Self::context_with_options(
            num_authorities,
            authority_index,
            gc_depth,
        ))
    }

    pub fn context_with_options(
        num_authorities: usize,
        authority_index: u32,
        gc_depth: Option<u32>,
    ) -> Arc<Context> {
        let (mut context, _keys) = Context::new_for_test(num_authorities);
        if let Some(gc_depth) = gc_depth {
            context
                .protocol_config
                .set_consensus_gc_depth_for_testing(gc_depth);
        }
        Arc::new(context.with_authority_index(AuthorityIndex::new_for_test(authority_index)))
    }

    // Adds the blocks to the transaction certifier and then tries to accept them via BlockManager.
    /// This registers the blocks for reject vote tracking (with no reject votes).
    pub fn try_accept_blocks(&mut self, blocks: Vec<VerifiedBlock>) {
        self.transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        self.block_manager.try_accept_blocks(blocks);
    }

    /// Adds blocks to the transaction certifier and dag state.
    /// This registers the blocks for reject vote tracking (with no reject votes).
    pub fn add_blocks(&self, blocks: Vec<VerifiedBlock>) {
        let blocks_and_votes = blocks.iter().map(|b| (b.clone(), vec![])).collect();
        self.transaction_certifier
            .add_voted_blocks(blocks_and_votes);
        self.dag_state.write().accept_blocks(blocks);
    }

    pub fn add_blocks_with_own_votes(
        &self,
        blocks_and_votes: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        let blocks = blocks_and_votes.iter().map(|(b, _)| b.clone()).collect();
        self.transaction_certifier
            .add_voted_blocks(blocks_and_votes);
        self.dag_state.write().accept_blocks(blocks);
    }

    /// Checks if the block manager has no suspended blocks.
    #[cfg(test)]
    pub(crate) fn has_no_suspended_blocks(&self) -> bool {
        self.block_manager.is_empty()
    }

    /// Tries to decide leaders, process and finalize commits, returning finalized commits
    /// and the updated last_decided slot.
    pub async fn try_commit(
        &mut self,
        last_decided: crate::block::Slot,
    ) -> (Vec<CommittedSubDag>, crate::block::Slot) {
        let sequence = self.committer.try_decide(last_decided);
        let new_last_decided = sequence
            .last()
            .map(|leader| leader.slot())
            .unwrap_or(last_decided);
        let finalized = self.process_commits(sequence).await;
        (finalized, new_last_decided)
    }

    /// Process decided leaders through linearizer and commit finalizer,
    /// similar to Core::try_commit() and CommitFinalizer::run().
    ///
    /// This extracts leader blocks from DecidedLeader::Commit, creates CommittedSubDags
    /// via the linearizer, and processes them through the commit finalizer.
    pub(crate) async fn process_commits(
        &mut self,
        sequence: Vec<DecidedLeader>,
    ) -> Vec<CommittedSubDag> {
        // Extract leader blocks from DecidedLeader::Commit (skip Skip decisions)
        let leaders: Vec<VerifiedBlock> = sequence
            .into_iter()
            .filter_map(|d| match d {
                DecidedLeader::Commit(block, _) => Some(block),
                DecidedLeader::Skip(_) => None,
            })
            .collect();

        if leaders.is_empty() {
            return vec![];
        }

        // Use linearizer to create CommittedSubDag
        let committed_sub_dags = self.linearizer.handle_commit(leaders);

        // Process through commit finalizer
        let mut finalized_commits = vec![];
        for mut subdag in committed_sub_dags {
            subdag.decided_with_local_blocks = true;
            let finalized = self.commit_finalizer.process_commit(subdag).await;
            finalized_commits.extend(finalized);
        }

        finalized_commits
    }
}

/// Compare commit sequences across all runs, asserting they are identical.
/// Returns the last commit sequence for additional assertions if needed.
pub fn assert_commit_sequences_match(
    mut commit_sequences: Vec<Vec<CommittedSubDag>>,
) -> Vec<CommittedSubDag> {
    let last_commit_sequence = commit_sequences.pop().unwrap();

    for (run, commit_sequence) in commit_sequences.into_iter().enumerate() {
        assert_eq!(
            commit_sequence.len(),
            last_commit_sequence.len(),
            "Commit sequence length mismatch at run {run}"
        );
        for (commit_index, (c1, c2)) in commit_sequence
            .iter()
            .zip(last_commit_sequence.iter())
            .enumerate()
        {
            assert_eq!(
                c1.leader, c2.leader,
                "Leader mismatch at commit {commit_index}"
            );
            assert_eq!(
                c1.commit_ref, c2.commit_ref,
                "Commit sequence mismatch at commit {commit_index}"
            );
            assert_eq!(
                c1.rejected_transactions_by_block, c2.rejected_transactions_by_block,
                "Rejected transactions mismatch at commit {commit_index}"
            );
        }
    }

    let mut total_transactions = 0;
    let mut rejected_transactions = 0;
    let mut reject_votes = 0;
    let mut blocks = 4;
    for commit in last_commit_sequence.iter() {
        total_transactions += commit
            .blocks
            .iter()
            .map(|block| block.transactions().len())
            .sum::<usize>();
        rejected_transactions += commit
            .rejected_transactions_by_block
            .values()
            .map(|transactions| transactions.len())
            .sum::<usize>();
        reject_votes += commit
            .blocks
            .iter()
            .map(|block| block.transaction_votes().len())
            .sum::<usize>();
        blocks += commit.blocks.len();
    }

    tracing::info!(
        "Finished comparing commit sequences. Commits: {}, Blocks: {}, Total transactions: {}, Rejected transactions: {}, Reject votes: {}",
        last_commit_sequence.len(),
        blocks,
        total_transactions,
        rejected_transactions,
        reject_votes
    );

    last_commit_sequence
}

// ---- RandomDag and RandomDagIterator ----

/// A randomly generated DAG for testing commit patterns with reject votes.
pub struct RandomDag {
    context: Arc<Context>,
    pub blocks: Vec<VerifiedBlock>,
    num_rounds: Round,
}

impl RandomDag {
    /// Creates a RandomDag from existing blocks.
    pub fn from_blocks(context: Arc<Context>, blocks: Vec<VerifiedBlock>) -> Self {
        let num_rounds = blocks.iter().map(|b| b.round()).max().unwrap_or(0);
        RandomDag {
            context,
            blocks,
            num_rounds,
        }
    }

    /// Creates an iterator yielding blocks out of order.
    pub fn random_iter<'a>(
        &'a self,
        rng: &'a mut StdRng,
        max_step: Round,
    ) -> RandomDagIterator<'a> {
        RandomDagIterator::new(self, rng, max_step)
    }
}

#[cfg(msim)]
impl RandomDag {
    /// Creates a new RandomDag with generated blocks containing transactions and reject votes.
    pub fn new(
        context: Arc<Context>,
        rng: &mut StdRng,
        num_rounds: Round,
        num_transactions: u32,
        reject_percentage: u8,
    ) -> Self {
        use std::collections::{BTreeMap, BTreeSet, VecDeque};

        let committee = &context.committee;
        let quorum_threshold = committee.quorum_threshold() as usize;
        let committee_size = committee.size();

        // Store all blocks for BFS lookup.
        let mut all_blocks: BTreeMap<BlockRef, VerifiedBlock> = BTreeMap::new();
        let mut blocks: Vec<VerifiedBlock> = vec![];
        let mut last_round_blocks: Vec<VerifiedBlock> = genesis_blocks(&context);

        // Track the latest block per authority.
        let mut latest_block_per_authority = last_round_blocks.clone();

        // Track included blocks per authority (simulates link_causal_history).
        let mut included_per_authority: Vec<BTreeSet<BlockRef>> =
            vec![BTreeSet::new(); committee_size];

        // Initialize with genesis blocks.
        for block in &last_round_blocks {
            all_blocks.insert(block.reference(), block.clone());
        }

        for r in 1..=num_rounds {
            // Select random quorum-or-more authorities to produce blocks this round.
            let n = rng.gen_range(quorum_threshold..=committee_size);
            let mut authorities: Vec<_> = committee.authorities().map(|(a, _)| a).collect();
            authorities.shuffle(rng);
            let selected_authorities = &authorities[..n];

            let mut current_round_blocks = Vec::new();

            for &authority in selected_authorities {
                // First, select exactly quorum blocks from the previous round.
                let mut prev_round_blocks = last_round_blocks.clone();
                prev_round_blocks.shuffle(rng);
                prev_round_blocks.truncate(quorum_threshold);
                let quorum_parents = prev_round_blocks;

                // Collect authorities already included in quorum parents.
                let quorum_authorities: BTreeSet<_> =
                    quorum_parents.iter().map(|b| b.author()).collect();

                // Find unselected authorities (those not in quorum parents).
                let unselected: Vec<_> = committee
                    .authorities()
                    .map(|(a, _)| a)
                    .filter(|a| !quorum_authorities.contains(a))
                    .collect();

                // Randomly select 0-N of unselected authorities' latest blocks.
                let extra_count = rng.gen_range(0..=unselected.len());
                let mut additional_ancestors = unselected;
                additional_ancestors.shuffle(rng);
                additional_ancestors.truncate(extra_count);

                // Combine ancestors: quorum parents + extra ancestors from unselected authorities.
                let mut ancestor_blocks = quorum_parents;
                ancestor_blocks.extend(
                    additional_ancestors
                        .iter()
                        .map(|a| latest_block_per_authority[a.value()].clone()),
                );
                let ancestors: Vec<_> = ancestor_blocks.iter().map(|b| b.reference()).collect();

                // Find newly connected blocks via BFS (similar to link_causal_history).
                let mut newly_connected = Vec::new();
                let mut queue = VecDeque::from_iter(ancestors.iter().copied());
                while let Some(block_ref) = queue.pop_front() {
                    if block_ref.round == 0 {
                        continue; // Skip genesis blocks.
                    }
                    if included_per_authority[authority.value()].contains(&block_ref) {
                        continue; // Already included.
                    }
                    included_per_authority[authority.value()].insert(block_ref);
                    newly_connected.push(block_ref);
                    // Traverse ancestors.
                    if let Some(block) = all_blocks.get(&block_ref) {
                        queue.extend(block.ancestors().iter().cloned());
                    }
                }

                // Generate random reject votes for newly connected blocks only.
                let votes: Vec<_> = newly_connected
                    .iter()
                    .filter(|_| reject_percentage > 0)
                    .filter_map(|&block_ref| {
                        let rejects: Vec<_> = (0..num_transactions)
                            .filter(|_| rng.gen_range(0..100) < reject_percentage)
                            .map(|idx| idx as TransactionIndex)
                            .collect();
                        (!rejects.is_empty())
                            .then_some(BlockTransactionVotes { block_ref, rejects })
                    })
                    .collect();

                let transactions: Vec<_> = (0..num_transactions)
                    .map(|_| Transaction::new(vec![1_u8; 16]))
                    .collect();

                let timestamp =
                    (r as u64) * 1000 + (authority.value() as u64) + rng.gen_range(0..100);

                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(r, authority.value() as u32)
                        .set_transactions(transactions)
                        .set_transaction_votes(votes)
                        .set_ancestors(ancestors)
                        .set_timestamp_ms(timestamp)
                        .build(),
                );

                current_round_blocks.push(block);
            }

            // Update state with current round blocks.
            for block in &current_round_blocks {
                all_blocks.insert(block.reference(), block.clone());
                latest_block_per_authority[block.author().value()] = block.clone();
            }
            blocks.extend(current_round_blocks.iter().cloned());
            last_round_blocks = current_round_blocks;
        }

        RandomDag {
            context,
            blocks,
            num_rounds,
        }
    }
}

/// Per-round state for iteration.
#[derive(Clone, Default)]
struct RoundState {
    // Total stake of visited blocks in this round.
    visited_stake: Stake,
    // Indices of unvisited blocks in this round.
    unvisited: Vec<usize>,
}

/// Iterator yielding blocks in constrained random order. Selects from rounds
/// `visited_round + 1` to `quorum_round + max_step`, simulating arrival with delays.
pub struct RandomDagIterator<'a> {
    dag: &'a RandomDag,
    rng: &'a mut StdRng,
    quorum_threshold: Stake,
    max_step: Round,
    // Highest round where all prior rounds have quorum stake visited.
    quorum_round: Round,
    // Highest round where all prior rounds have all blocks visited.
    visited_round: Round,
    // State of each round.
    round_states: Vec<RoundState>,
    // Number of blocks remaining to visit.
    num_remaining: usize,
}

impl<'a> RandomDagIterator<'a> {
    fn new(dag: &'a RandomDag, rng: &'a mut StdRng, max_step: Round) -> Self {
        let num_rounds = dag.num_rounds as usize;
        let committee = &dag.context.committee;
        let quorum_threshold = committee.quorum_threshold();

        let mut round_states: Vec<RoundState> = vec![RoundState::default(); num_rounds + 1];

        for (idx, block) in dag.blocks.iter().enumerate() {
            let round = block.round() as usize;
            round_states[round].unvisited.push(idx);
        }

        let num_remaining = dag.blocks.len();

        Self {
            dag,
            rng,
            max_step,
            quorum_round: 0,
            visited_round: 0,
            quorum_threshold,
            round_states,
            num_remaining,
        }
    }
}

impl Iterator for RandomDagIterator<'_> {
    type Item = VerifiedBlock;

    fn next(&mut self) -> Option<Self::Item> {
        if self.num_remaining == 0 {
            return None;
        }

        // Eligible rounds: from first unvisited to quorum_round + max_step.
        let min_round = self.visited_round as usize + 1;
        let max_round =
            ((self.quorum_round + self.max_step) as usize).min(self.round_states.len() - 1);
        let eligible_rounds = min_round..=max_round;

        let total_candidates: usize = eligible_rounds
            .clone()
            .map(|r| self.round_states[r].unvisited.len())
            .sum();

        if total_candidates == 0 {
            return None;
        }

        // Select random candidate by index across eligible rounds.
        let mut selection = self.rng.gen_range(0..total_candidates);
        let mut selected_round = 0;
        let mut selected_pos = 0;

        for r in eligible_rounds {
            let count = self.round_states[r].unvisited.len();
            if selection < count {
                selected_round = r;
                selected_pos = selection;
                break;
            }
            selection -= count;
        }

        // Get block index and remove from unvisited.
        let block_idx = self.round_states[selected_round]
            .unvisited
            .swap_remove(selected_pos);
        let block = self.dag.blocks[block_idx].clone();

        // Update visited stake for this round.
        let stake = self.dag.context.committee.stake(block.author());
        self.round_states[selected_round].visited_stake += stake;
        self.num_remaining -= 1;

        // Advance visited_round while next round has all blocks visited.
        while self
            .round_states
            .get(self.visited_round as usize + 1)
            .is_some_and(|s| s.unvisited.is_empty())
        {
            self.visited_round += 1;
        }

        // Advance quorum_round while next round has quorum stake visited.
        while self
            .round_states
            .get(self.quorum_round as usize + 1)
            .is_some_and(|s| s.visited_stake >= self.quorum_threshold)
        {
            self.quorum_round += 1;
        }

        Some(block)
    }
}

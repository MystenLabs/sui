// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test fixture for commit-related tests.
//! Used by both commit_finalizer.rs tests and randomized_tests.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use consensus_config::{AuthorityIndex, Stake};
use consensus_types::block::BlockRef;
use consensus_types::block::{Round, TransactionIndex};
use mysten_metrics::monitored_mpsc::unbounded_channel;
use parking_lot::RwLock;
use rand::prelude::SliceRandom;
use rand::{Rng, rngs::StdRng};

use crate::Transaction;
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

        // After handle_commit(), the GC round is updated. We need to unsuspend any blocks that were
        // suspended because of missing ancestors that are now GC'ed.
        self.block_manager
            .try_unsuspend_blocks_for_latest_gc_round();

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
/// Returns the shortest commit sequence for additional assertions if needed.
pub fn assert_commit_sequences_match(
    commit_sequences: Vec<Vec<CommittedSubDag>>,
) -> Vec<CommittedSubDag> {
    let (shortest_idx, shortest_sequence) = commit_sequences
        .iter()
        .enumerate()
        .min_by_key(|(_, seq)| seq.len())
        .expect("commit_sequences should not be empty");

    for (run, commit_sequence) in commit_sequences.iter().enumerate() {
        // Since INDIRECT_REJECT_DEPTH is 3, the maximum number of commits buffered in CommitFinalizer is 3.
        // And because of the direct finalization optimization, it might happen the last 3 commits are pending in
        // one run but all finalized in another.
        assert!(
            commit_sequence.len() <= shortest_sequence.len() + 3,
            "Commit sequence at run {run} is more than 3 commits longer than shortest (run {shortest_idx}): {} vs {}",
            commit_sequence.len(),
            shortest_sequence.len()
        );

        for (commit_index, (c1, c2)) in commit_sequence
            .iter()
            .zip(shortest_sequence.iter())
            .enumerate()
        {
            assert_eq!(
                c1.leader, c2.leader,
                "Leader mismatch at run {run} commit {commit_index}"
            );
            assert_eq!(
                c1.commit_ref, c2.commit_ref,
                "Commit sequence mismatch at run {run} commit {commit_index}"
            );
            assert_eq!(
                c1.rejected_transactions_by_block, c2.rejected_transactions_by_block,
                "Rejected transactions mismatch at run {run} commit {commit_index}"
            );
        }
    }

    let mut total_transactions = 0;
    let mut rejected_transactions = 0;
    let mut reject_votes = 0;
    let mut blocks = 4;
    for commit in shortest_sequence.iter() {
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
        shortest_sequence.len(),
        blocks,
        total_transactions,
        rejected_transactions,
        reject_votes
    );

    shortest_sequence.clone()
}

// ---- RandomDag and RandomDagIterator ----

/// A randomly generated DAG for testing commit patterns with reject votes.
pub struct RandomDag {
    pub blocks: Vec<VerifiedBlock>,
}

impl RandomDag {
    /// Creates a RandomDag from existing blocks.
    pub fn from_blocks(blocks: Vec<VerifiedBlock>) -> Self {
        RandomDag { blocks }
    }

    /// Creates an iterator yielding blocks in dependency-respecting random order.
    pub fn random_iter<'a>(&'a self, rng: &'a mut StdRng) -> RandomDagIterator<'a> {
        RandomDagIterator::new(self, rng)
    }
}

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
        let quorum_threshold = committee.quorum_threshold();
        let total_stake = committee.total_stake();

        // Store all blocks for BFS lookup.
        let mut all_blocks: BTreeMap<BlockRef, VerifiedBlock> = BTreeMap::new();
        let mut blocks: Vec<VerifiedBlock> = vec![];
        let mut last_round_blocks: Vec<VerifiedBlock> = genesis_blocks(&context);

        // Track the latest block per authority.
        let mut latest_block_per_authority = last_round_blocks.clone();

        // Track included blocks per authority (simulates link_causal_history).
        let mut included_per_authority: Vec<BTreeSet<BlockRef>> =
            vec![BTreeSet::new(); committee.size()];

        // Initialize with genesis blocks.
        for block in &last_round_blocks {
            all_blocks.insert(block.reference(), block.clone());
        }

        for r in 1..=num_rounds {
            // Select random quorum-or-more authorities to produce blocks this round.
            let target_stake = rng.gen_range(quorum_threshold..=total_stake);
            let mut authorities: Vec<_> = committee.authorities().map(|(a, _)| a).collect();
            authorities.shuffle(rng);
            let selected_authorities: Vec<_> = authorities
                .into_iter()
                .scan(0, |acc, a| {
                    if *acc >= target_stake {
                        return None;
                    }
                    *acc += committee.stake(a);
                    Some(a)
                })
                .collect();

            let mut current_round_blocks = Vec::new();

            for authority in selected_authorities {
                // Start with own authority's latest block as first ancestor
                let own_latest_block = latest_block_per_authority[authority.value()].clone();
                let own_latest_ref = own_latest_block.reference();
                // Check if own block is from the previous round
                let own_is_prev_round = own_latest_ref.round == r - 1;
                // Select blocks from the previous round until quorum stake is reached.
                let mut prev_round_blocks: Vec<_> = last_round_blocks
                    .iter()
                    .filter(|b| b.reference() != own_latest_ref)
                    .cloned()
                    .collect();
                prev_round_blocks.shuffle(rng);
                let mut parent_stake: Stake = if own_is_prev_round {
                    committee.stake(authority)
                } else {
                    0
                };
                let mut quorum_selected_count = 0;
                for block in &prev_round_blocks {
                    if parent_stake >= quorum_threshold {
                        break;
                    }
                    parent_stake += committee.stake(block.author());
                    quorum_selected_count += 1;
                }
                prev_round_blocks.truncate(quorum_selected_count);
                let quorum_parents = prev_round_blocks;

                // Collect authorities already included in quorum parents.
                let quorum_authorities: BTreeSet<_> =
                    quorum_parents.iter().map(|b| b.author()).collect();

                // Find unselected authorities (those not in quorum parents).
                let unselected: Vec<_> = committee
                    .authorities()
                    .map(|(a, _)| a)
                    .filter(|a| !quorum_authorities.contains(a) && *a != authority)
                    .collect();

                // Use min of two uniform samples to bias toward fewer additional ancestors
                // while maintaining non-zero probability for all counts.
                let extra_count = rng
                    .gen_range(0..=unselected.len())
                    .min(rng.gen_range(0..=unselected.len()));
                let mut additional_ancestors = unselected;
                additional_ancestors.shuffle(rng);
                additional_ancestors.truncate(extra_count);

                // Combine ancestors: quorum parents + extra ancestors from unselected authorities.
                let mut ancestor_blocks = vec![own_latest_block];
                ancestor_blocks.extend(quorum_parents);
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

        RandomDag { blocks }
    }
}

/// Iterator yielding blocks in dependency-respecting random order.
/// A block becomes a candidate only after all its ancestors have been selected.
pub struct RandomDagIterator<'a> {
    rng: &'a mut StdRng,
    // Map from BlockRef to VerifiedBlock for lookup.
    blocks: BTreeMap<BlockRef, VerifiedBlock>,
    // Blocks ready to be selected (all ancestors already selected).
    candidates: Vec<BlockRef>,
    // Map: block -> set of unselected ancestors it's waiting for.
    pending: BTreeMap<BlockRef, BTreeSet<BlockRef>>,
    // Reverse map: block -> blocks that have this block as an ancestor.
    dependents: BTreeMap<BlockRef, Vec<BlockRef>>,
}

impl<'a> RandomDagIterator<'a> {
    fn new(dag: &'a RandomDag, rng: &'a mut StdRng) -> Self {
        let mut blocks = BTreeMap::new();
        let mut candidates = Vec::new();
        let mut pending = BTreeMap::new();
        let mut dependents: BTreeMap<BlockRef, Vec<BlockRef>> = BTreeMap::new();

        // Build block map.
        for block in &dag.blocks {
            blocks.insert(block.reference(), block.clone());
        }

        // Initialize candidates and pending based on ancestors.
        for block in &dag.blocks {
            let block_ref = block.reference();
            // Collect non-genesis ancestors that are in the DAG.
            let unselected_ancestors: BTreeSet<BlockRef> = block
                .ancestors()
                .iter()
                .filter(|a| a.round > 0 && blocks.contains_key(a))
                .copied()
                .collect();

            if unselected_ancestors.is_empty() {
                // Round 1 blocks (or blocks with only genesis ancestors) are candidates.
                candidates.push(block_ref);
            } else {
                // Register this block as a dependent of each ancestor.
                for ancestor in &unselected_ancestors {
                    dependents.entry(*ancestor).or_default().push(block_ref);
                }
                pending.insert(block_ref, unselected_ancestors);
            }
        }

        Self {
            rng,
            blocks,
            candidates,
            pending,
            dependents,
        }
    }
}

impl Iterator for RandomDagIterator<'_> {
    type Item = VerifiedBlock;

    fn next(&mut self) -> Option<Self::Item> {
        if self.candidates.is_empty() {
            return None;
        }

        // Randomly select a candidate.
        let idx = self.rng.gen_range(0..self.candidates.len());
        let selected_ref = self.candidates.swap_remove(idx);
        let block = self.blocks.remove(&selected_ref)?;

        // Update dependents: for each block waiting on this one, remove from its pending set.
        if let Some(waiting_blocks) = self.dependents.remove(&selected_ref) {
            for dependent_ref in waiting_blocks {
                if let Some(ancestors) = self.pending.get_mut(&dependent_ref) {
                    ancestors.remove(&selected_ref);
                    if ancestors.is_empty() {
                        // All ancestors selected, move to candidates.
                        self.pending.remove(&dependent_ref);
                        self.candidates.push(dependent_ref);
                    }
                }
            }
        }

        Some(block)
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_config::Stake;
use mysten_metrics::{
    monitored_mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    monitored_scope, spawn_logged_monitored_task,
};

use crate::{
    context::Context,
    error::{ConsensusError, ConsensusResult},
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    transaction_certifier::TransactionCertifier,
    BlockAPI, BlockRef, CommitIndex, CommittedSubDag, Round, TransactionIndex,
};

/// Number of rounds between the leader that committed a transaction to the latest leader,
/// where indirect finalization and rejection are allowed and required.
const INDIRECT_FINALIZE_DEPTH: Round = 3;

/// Number of rounds above the leader that committed a transaction,
/// where accept votes are collected.
/// NOTE: it should be possible to remove this limit.
const VOTE_DEPTH: Round = 1;

pub(crate) struct CommitFinalizerHandle {
    sender: UnboundedSender<(CommittedSubDag, bool)>,
}

impl CommitFinalizerHandle {
    pub(crate) fn send(&self, commit: (CommittedSubDag, bool)) -> ConsensusResult<()> {
        self.sender.send(commit).map_err(|e| {
            tracing::warn!("Failed to send to commit finalizer, probably due to shutdown: {e:?}");
            ConsensusError::Shutdown
        })
    }
}

pub(crate) struct CommitFinalizer {
    context: Arc<Context>,
    transaction_certifier: TransactionCertifier,
    commit_sender: UnboundedSender<CommittedSubDag>,

    last_processed_commit: Option<CommitIndex>,
    commits: VecDeque<CommitState>,
    blocks: BTreeMap<BlockRef, BlockState>,
}

#[allow(dead_code)]
impl CommitFinalizer {
    fn new(
        context: Arc<Context>,
        transaction_certifier: TransactionCertifier,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> Self {
        Self {
            context,
            transaction_certifier,
            commit_sender,
            last_processed_commit: None,
            commits: VecDeque::new(),
            blocks: BTreeMap::new(),
        }
    }

    pub(crate) fn start(
        context: Arc<Context>,
        transaction_certifier: TransactionCertifier,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> CommitFinalizerHandle {
        let processor = Self::new(context, transaction_certifier, commit_sender);
        let (sender, receiver) = unbounded_channel("commit_finalizer");
        let _handle = spawn_logged_monitored_task!(processor.run(receiver), "commit_finalizer");
        CommitFinalizerHandle { sender }
    }

    async fn run(self, mut receiver: UnboundedReceiver<(CommittedSubDag, bool)>) {
        while let Some((committed_sub_dag, _direct)) = receiver.recv().await {
            if let Err(e) = self.commit_sender.send(committed_sub_dag) {
                tracing::warn!("Failed to send to commit handler, probably due to shutdown: {e:?}");
                return;
            }
        }
    }

    fn process_commit(
        &mut self,
        committed_sub_dag: CommittedSubDag,
        direct: bool,
    ) -> Vec<CommittedSubDag> {
        let _scope = monitored_scope("CommitFinalizer::process_commit");

        if let Some(last_processed_commit) = self.last_processed_commit {
            assert_eq!(
                last_processed_commit + 1,
                committed_sub_dag.commit_ref.index
            );
        }
        self.last_processed_commit = Some(committed_sub_dag.commit_ref.index);
        self.commits.push_back(CommitState::new(committed_sub_dag));

        let mut finalized_commits = vec![];

        if direct {
            self.try_direct_finalize();
            finalized_commits.extend(self.pop_finalized_commits());
        }

        // If there are remaining commits, initialize them for indirect finalization
        // and try to indirectly finalize a prefix of them.
        if !self.commits.is_empty() {
            self.link_blocks();
            self.inherit_reject_votes();
            // Try to indirectly finalize a prefix of the buffered commits.
            // The last commit cannot be indirectly finalized.
            for i in 0..(self.commits.len() - 1) {
                self.try_indirect_finalize_commit(i);
                let new_finalized_commits = self.pop_finalized_commits();
                if new_finalized_commits.is_empty() {
                    // No future commits can be indirectly finalized.
                    break;
                }
                finalized_commits.extend(new_finalized_commits);
            }
        }

        // Run TransactionCertifier GC only with finalized commits.
        // Other blocks can still be used for finalization.
        if let Some(last_commit) = finalized_commits.last() {
            let gc_round = last_commit
                .leader
                .round
                .saturating_sub(self.context.protocol_config.consensus_gc_depth());
            self.transaction_certifier.run_gc(gc_round);
        }

        finalized_commits
    }

    // Tries to directly finalize blocks and transactions in the last commit.
    fn try_direct_finalize(&mut self) {
        let commit_state = self.commits.back_mut().unwrap();
        // All blocks in a direct commit are finalized. But the transactions contained in them
        // may not have been finalized.
        let pending_blocks = std::mem::take(&mut commit_state.pending_blocks);
        for block_ref in pending_blocks {
            let reject_votes = self.transaction_certifier.get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is likely gc'ed or failed to be recovered after crash."));
            for (transaction_index, stake) in reject_votes {
                let entry = if stake < self.context.committee.quorum_threshold() {
                    commit_state
                        .pending_transactions
                        .entry(block_ref)
                        .or_default()
                } else {
                    commit_state
                        .rejected_transactions
                        .entry(block_ref)
                        .or_default()
                };
                entry.insert(transaction_index);
            }
        }
    }

    // Creates an entry in the blocks map for each block in the commit,
    // and links each block to its ancestors.
    fn link_blocks(&mut self) {
        let commit_state = self.commits.back_mut().unwrap();

        // Link blocks in ascending order of round.
        let mut blocks = commit_state.commit.blocks.clone();
        blocks.sort_by_key(|b| b.round());

        for block in blocks {
            // Initialize the block state.
            let block_state = self.blocks.entry(block.reference()).or_default();
            for votes in block.transaction_votes() {
                block_state
                    .reject_votes
                    .entry(votes.block_ref)
                    .or_default()
                    .extend(votes.rejects.iter());
            }

            // Link the block to its ancestors.
            let block_ref = block.reference();
            for ancestor in block.ancestors() {
                // Ancestor may not exist in the blocks map if it has been finalized or gc'ed.
                if let Some(ancestor_block) = self.blocks.get_mut(ancestor) {
                    ancestor_block.children.insert(block_ref);
                }
            }
        }
    }

    // When block A's same authority ancestor block B rejects a transaction, A is implicitly assumed
    // to reject the transaction as well. Both A and B can link to the block containing
    // the rejected transaction, because not linking to the same ancestor twice is only an optimization,
    // not a requirement.
    //
    // So to simplify counting accept and reject votes for finalization, make reject votes explicit
    // in each block state.
    fn inherit_reject_votes(&mut self) {
        let commit_state = self.commits.back_mut().unwrap();

        // Inherit from lower round blocks to higher round blocks.
        let mut blocks = commit_state.commit.blocks.clone();
        blocks.sort_by_key(|b| b.round());

        for block in blocks {
            // Inherit reject votes from the ancestor of block's own authority.
            // own_ancestor should usually be the 1st ancestor.
            // Also, block verification ensures each authority has at most one ancestor.
            let Some(own_ancestor) = block
                .ancestors()
                .iter()
                .find(|b| b.author == block.author())
                .copied()
            else {
                continue;
            };
            let Some(own_ancestor_rejects) = self
                .blocks
                .get(&own_ancestor)
                .map(|b| b.reject_votes.clone())
            else {
                // The ancestor block may have been finalized or gc'ed.
                continue;
            };
            // No reject votes from the ancestor block to inherit.
            if own_ancestor_rejects.is_empty() {
                continue;
            }
            // Otherwise, inherit the reject votes.
            let block_state = self.blocks.get_mut(&block.reference()).unwrap();
            for (block_ref, reject_votes) in own_ancestor_rejects {
                block_state
                    .reject_votes
                    .entry(block_ref)
                    .or_default()
                    .extend(reject_votes.iter());
            }
        }
    }

    fn try_indirect_finalize_commit(&mut self, index: usize) {
        // Optional optimization: re-check pending transactions to see if they are rejected by a quorum now.
        self.check_pending_transactions(index);

        // Check if remaining pending blocks can be finalized.
        // If a block is finalized, record its pending and rejected transactions.
        // The rest of the transactions in the block are indirectly finalized.
        self.try_indirect_finalize_pending_blocks(index);

        // Check if remaining pending transaction can be finalized.
        self.try_indirect_finalize_pending_transactions(index);

        // Check if remaining pending transactions can be indirectly rejected.
        self.try_indirect_reject_pending_transactions(index);
    }

    fn check_pending_transactions(&mut self, index: usize) {
        let pending_transactions = self.commits[index]
            .pending_transactions
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for block_ref in pending_transactions {
            let reject_votes: BTreeMap<TransactionIndex, Stake> = self
                .transaction_certifier
                .get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is likely gc'ed or failed to be recovered after crash."))
                .into_iter()
                .collect();
            for (transaction_index, stake) in reject_votes {
                if stake < self.context.committee.quorum_threshold() {
                    // pending_transactions do not need to be updated in this case,
                    // whether the transaction exists in pending_transactions or not.
                    continue;
                }
                // Move the rejected transaction from pending_transactions to rejected_transactions.
                let curr_commit_state = &mut self.commits[index];
                let pending_transactions = curr_commit_state
                    .pending_transactions
                    .get_mut(&block_ref)
                    .unwrap();
                pending_transactions.remove(&transaction_index);
                if pending_transactions.is_empty() {
                    curr_commit_state.pending_transactions.remove(&block_ref);
                }
                curr_commit_state
                    .rejected_transactions
                    .entry(block_ref)
                    .or_default()
                    .insert(transaction_index);
            }
        }
    }

    fn try_indirect_finalize_pending_blocks(&mut self, index: usize) {
        let curr_leader_round = self.commits[index].commit.leader.round;
        let pending_blocks = self.commits[index].pending_blocks.clone();
        for block_ref in pending_blocks {
            let (block_finalized, _) =
                self.try_indirect_finalize_block(curr_leader_round, block_ref, BTreeMap::new());
            if !block_finalized {
                continue;
            }
            let curr_commit_state = &mut self.commits[index];
            curr_commit_state.pending_blocks.remove(&block_ref);
            let reject_votes: BTreeMap<TransactionIndex, Stake> = self
                .transaction_certifier
                .get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is likely gc'ed or failed to be recovered after crash."))
                .into_iter()
                .collect();
            for (transaction_index, stake) in reject_votes {
                let entry = if stake < self.context.committee.quorum_threshold() {
                    curr_commit_state
                        .pending_transactions
                        .entry(block_ref)
                        .or_default()
                } else {
                    curr_commit_state
                        .rejected_transactions
                        .entry(block_ref)
                        .or_default()
                };
                entry.insert(transaction_index);
            }
        }
    }

    fn try_indirect_finalize_pending_transactions(&mut self, index: usize) {
        let curr_leader_round = self.commits[index].commit.leader.round;
        let pending_transactions = self.commits[index].pending_transactions.clone();
        for (block_ref, pending_transactions) in pending_transactions {
            let accept_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>> =
                pending_transactions
                    .into_iter()
                    .map(|transaction_index| (transaction_index, StakeAggregator::new()))
                    .collect();
            let (_, finalized_transactions) =
                self.try_indirect_finalize_block(curr_leader_round, block_ref, accept_votes);
            if !finalized_transactions.is_empty() {
                let curr_commit_state = &mut self.commits[index];
                let undecided_txns = curr_commit_state
                    .pending_transactions
                    .get_mut(&block_ref)
                    .unwrap();
                for t in finalized_transactions {
                    undecided_txns.remove(&t);
                }
                if undecided_txns.is_empty() {
                    curr_commit_state.pending_transactions.remove(&block_ref);
                }
            }
        }
    }

    fn try_indirect_reject_pending_transactions(&mut self, index: usize) {
        let curr_leader_round = self.commits[index].commit.leader.round;
        let last_commit_leader_round = self.commits.back().unwrap().commit.leader.round;
        if curr_leader_round + INDIRECT_FINALIZE_DEPTH <= last_commit_leader_round {
            let curr_commit_state = &mut self.commits[index];
            // When the last leader round is no lower than INDIRECT_FINALIZE_DEPTH,
            // all pending blocks should have been finalized.
            assert!(curr_commit_state.pending_blocks.is_empty());
            // All remaining pending transactions are indirectly rejected.
            let pending_transactions = std::mem::take(&mut curr_commit_state.pending_transactions);
            for (block_ref, pending_transactions) in pending_transactions {
                curr_commit_state
                    .rejected_transactions
                    .entry(block_ref)
                    .or_default()
                    .extend(pending_transactions);
            }
        }
    }

    // Returns if the block is indirectly finalized, and requested transactions in accept_votes
    // that are indirectly finalized.
    fn try_indirect_finalize_block(
        &self,
        curr_leader_round: Round,
        block_ref: BlockRef,
        mut accept_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    ) -> (bool, Vec<TransactionIndex>) {
        let mut finalized_transactions = vec![];
        let mut to_visit_blocks = self
            .blocks
            .get(&block_ref)
            .unwrap()
            .children
            .iter()
            .filter_map(|b| {
                if b.round <= curr_leader_round + VOTE_DEPTH {
                    Some(*b)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        let mut visited_stake = StakeAggregator::<QuorumThreshold>::new();
        while let Some(block_ref) = to_visit_blocks.pop() {
            if !visited.insert(block_ref) {
                continue;
            }
            visited_stake.add(block_ref.author, &self.context.committee);
            let visit_block_state = self.blocks.get(&block_ref).unwrap();
            let visit_reject_votes = visit_block_state
                .reject_votes
                .get(&block_ref)
                .cloned()
                .unwrap_or_default();
            let mut newly_finalized = vec![];
            for (index, stake) in &mut accept_votes {
                if visit_reject_votes.contains(index) {
                    continue;
                }
                if !stake.add(block_ref.author, &self.context.committee) {
                    continue;
                }
                newly_finalized.push(*index);
                finalized_transactions.push(*index);
            }
            // There is no need to aggregate additional votes for finalized transactions.
            for index in newly_finalized {
                accept_votes.remove(&index);
            }
            // End traversing if all blocks and requested transactions have reached quorum.
            if visited_stake.reached_threshold(&self.context.committee) && accept_votes.is_empty() {
                break;
            }
            // Visit additional blocks.
            to_visit_blocks.extend(
                visit_block_state
                    .children
                    .iter()
                    .filter(|b| b.round <= curr_leader_round + VOTE_DEPTH && !visited.contains(*b)),
            );
        }
        (
            visited_stake.reached_threshold(&self.context.committee),
            finalized_transactions,
        )
    }

    fn pop_finalized_commits(&mut self) -> Vec<CommittedSubDag> {
        let mut finalized_commits = vec![];
        while let Some(commit_state) = self.commits.front() {
            if !commit_state.pending_blocks.is_empty()
                || !commit_state.pending_transactions.is_empty()
            {
                break;
            }
            let commit_state = self.commits.pop_front().unwrap();
            let mut commit = commit_state.commit;
            for (block_ref, rejected_transactions) in commit_state.rejected_transactions {
                commit
                    .rejected_transactions_by_block
                    .insert(block_ref, rejected_transactions.into_iter().collect());
            }
            finalized_commits.push(commit);
        }
        finalized_commits
    }
}

struct CommitState {
    commit: CommittedSubDag,
    // Blocks pending finalization.
    pending_blocks: BTreeSet<BlockRef>,
    // Transactions pending finalization, where the block is already finalized.
    pending_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    // Transactions rejected by a quorum or indirectly finalized, per block.
    rejected_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

impl CommitState {
    fn new(commit: CommittedSubDag) -> Self {
        let pending_blocks = commit.blocks.iter().map(|b| b.reference()).collect();
        Self {
            commit,
            pending_blocks,
            pending_transactions: BTreeMap::new(),
            rejected_transactions: BTreeMap::new(),
        }
    }
}

#[derive(Default)]
struct BlockState {
    children: BTreeSet<BlockRef>,
    reject_votes: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

#[cfg(test)]
mod tests {
    use mysten_metrics::monitored_mpsc;
    use parking_lot::RwLock;

    use crate::{
        dag_state::DagState, linearizer::Linearizer, storage::mem_store::MemStore,
        test_dag_builder::DagBuilder,
    };

    use super::*;

    struct Fixture {
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_certifier: TransactionCertifier,
        linearizer: Linearizer,
        commit_finalizer: CommitFinalizer,
    }

    fn create_commit_finalizer_fixture() -> Fixture {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let linearizer = Linearizer::new(context.clone(), dag_state.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (commit_sender, _commit_receiver) = unbounded_channel("consensus_commit_output");
        let commit_finalizer = CommitFinalizer::new(
            context.clone(),
            transaction_certifier.clone(),
            commit_sender,
        );
        Fixture {
            context,
            dag_state,
            transaction_certifier,
            linearizer,
            commit_finalizer,
        }
    }

    #[tokio::test]
    async fn test_direct_finalize_no_reject_votes() {
        let mut fixture = create_commit_finalizer_fixture();

        // Create a round 1 and 2 blocks with 10 transactions each.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder
            .layers(1..=2)
            .num_transactions(10)
            .build()
            .persist_layers(fixture.dag_state.clone());
        let blocks = dag_builder.all_blocks();
        fixture
            .transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());

        // Select a round 2 block as the leader and create CommittedSubDag.
        let leader = blocks.iter().find(|b| b.round() == 2).unwrap();
        let committed_sub_dags = fixture.linearizer.handle_commit(vec![leader.clone()]);
        assert_eq!(committed_sub_dags.len(), 1);
        let committed_sub_dag = &committed_sub_dags[0];

        // This committed sub-dag can be directly finalized.
        let finalized_commits = fixture
            .commit_finalizer
            .process_commit(committed_sub_dag.clone(), true);
        assert_eq!(finalized_commits.len(), 1);
        let finalized_commit = &finalized_commits[0];
        assert_eq!(committed_sub_dag, finalized_commit);
    }

    #[tokio::test]
    async fn test_indirect_finalize_no_reject_votes() {
        let mut fixture = create_commit_finalizer_fixture();

        // Create 5 rounds of blocks with 10 transactions each.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder
            .layers(1..=5)
            .num_transactions(10)
            .build()
            .persist_layers(fixture.dag_state.clone());
        let blocks = dag_builder.all_blocks();
        fixture
            .transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());

        // Select a block from round 2-5 as leaders and create CommittedSubDag.
        let leaders = vec![
            blocks[5].clone(),
            blocks[10].clone(),
            blocks[15].clone(),
            blocks[16].clone(),
        ];
        assert_eq!(
            leaders.iter().map(|b| b.round()).collect::<Vec<_>>(),
            vec![2, 3, 4, 5]
        );
        let committed_sub_dags = fixture.linearizer.handle_commit(leaders);
        assert_eq!(committed_sub_dags.len(), 4);

        // Process 1st and 2nd leaders as indirect commits. They will not be finalized.
        for commit in committed_sub_dags[0..2].iter() {
            let finalized_commits = fixture
                .commit_finalizer
                .process_commit(commit.clone(), false);
            assert!(
                finalized_commits.is_empty(),
                "unexpected commits: {:?}",
                finalized_commits
            );
        }

        // Process 3rd leader as indirect commit. The 1st commit will be finalized.
        let finalized_commits = fixture
            .commit_finalizer
            .process_commit(committed_sub_dags[2].clone(), false);
        assert_eq!(finalized_commits.len(), 1);
        assert_eq!(&committed_sub_dags[0], &finalized_commits[0]);

        // Process 4th leader as direct commit. 2nd commit should be finalized.
        let finalized_commits = fixture
            .commit_finalizer
            .process_commit(committed_sub_dags[3].clone(), true);
        assert_eq!(finalized_commits.len(), 1);
        assert_eq!(&committed_sub_dags[1], &finalized_commits[0]);
    }
}

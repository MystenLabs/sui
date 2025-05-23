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
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    transaction_certifier::TransactionCertifier,
    BlockAPI, BlockRef, CommitIndex, CommittedSubDag, Round, TransactionIndex,
};

/// For transaction T committed at leader round R, when a new leader at round >= R + INDIRECT_REJECT_DEPTH
/// commits and T is still not finalized, T is rejected.
/// NOTE: 3 round is the minimum depth possible for indirect finalization and rejection.
const INDIRECT_REJECT_DEPTH: Round = 3;

/// For transaction T committed at leader round R, accept votes are collected
/// from blocks with round <= R + VOTE_DEPTH.
/// NOTE: it should be possible to remove this limit.
const VOTE_DEPTH: Round = 1;

/// Handle to CommitFinalizer, for sending CommittedSubDag and whether it is a direct commit.
pub(crate) struct CommitFinalizerHandle {
    sender: UnboundedSender<(CommittedSubDag, bool)>,
}

impl CommitFinalizerHandle {
    // Sends a CommittedSubDag and whether it is a direct commit to CommitFinalizer,
    // which will finalize the commit before sending it to execution.
    //
    // NOTE: it is always safe to consider a commit as indirect, even if it is a direct commit.
    // The inverse is not true, because a direct commit will trigger optimizations that are invalid for indirect commits.
    pub(crate) fn send(&self, commit: (CommittedSubDag, bool)) -> ConsensusResult<()> {
        self.sender.send(commit).map_err(|e| {
            tracing::warn!("Failed to send to commit finalizer, probably due to shutdown: {e:?}");
            ConsensusError::Shutdown
        })
    }
}

/// CommitFinalizer accepts a continuous stream of CommittedSubDag and outputs
/// them when they are finalized.
/// In finalized commits, every transaction is either finalized or rejected.
/// It runs in a separate thread, to reduce the load on the core thread.
///
/// Life of a finalized commit:
///
/// For efficiency, finalization happens first on the block level, then undecided transactions are
/// individually finalized or rejected. When there is no more undecided transactions, the commit
/// is finalized.
///
/// A finalized block means that blocks has a quorum of certificates, where each certificate
/// has a quorum of votes (links) to the block. A finalized block can contain
/// finalized, rejected or pending transactions.
///
/// When a commit is received, if it is a direct commit, then every block is finalized by the
/// quorum of leader certificates that result in the direct commit. Transactions in each block
/// can immediately move to the finalized, rejected or pending state.
///
/// If the commit is not a direct commit, then the commit is added to the buffer and initialized
/// for indirect finalization. The state of the commit starts with all blocks in the pending state.
///
/// From the earliest buffered commit, pending blocks are checked to see if they are now finalized.
/// New finalized blocks are removed from the pending blocks, and its transactions are moved to the
/// finalized, rejected or pending state. If the commit now has no pending blocks or transactions,
/// the commit is finalized and popped from the buffer. The next earliest commit is then processed
/// similarly, until either the buffer becomes empty or a commit with pending blocks or transactions
/// is encountered.
pub(crate) struct CommitFinalizer {
    context: Arc<Context>,
    transaction_certifier: TransactionCertifier,
    commit_sender: UnboundedSender<CommittedSubDag>,

    last_processed_commit: Option<CommitIndex>,
    pending_commits: VecDeque<CommitState>,
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
            pending_commits: VecDeque::new(),
            blocks: BTreeMap::new(),
        }
    }

    pub(crate) fn start(
        context: Arc<Context>,
        transaction_certifier: TransactionCertifier,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> CommitFinalizerHandle {
        let processor = Self::new(context, transaction_certifier, commit_sender);
        let (sender, receiver) = unbounded_channel("consensus_commit_finalizer");
        let _handle =
            spawn_logged_monitored_task!(processor.run(receiver), "consensus_commit_finalizer");
        CommitFinalizerHandle { sender }
    }

    async fn run(self, mut receiver: UnboundedReceiver<(CommittedSubDag, bool)>) {
        // TODO(fastpath): call process_commit() when crash recovery of rejected transactions are implemented.
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

        self.pending_commits
            .push_back(CommitState::new(committed_sub_dag));

        let mut finalized_commits = vec![];

        // Optional optimization: if the latest commit is direct, there are a quorum of leader certificates.
        // So if a transaction has no reject vote, there must have a quorum of certificates in local DAG.
        if direct {
            self.try_direct_finalize_last_commit();
            finalized_commits.extend(self.pop_finalized_commits());
        }

        // In this case, either the last commit cannot be directly finalized, or there are previous commits
        // that cannot be finalized yet.
        if !self.pending_commits.is_empty() {
            // If there are remaining commits, initialize the added commit for indirect finalization.
            // Even if the added commit has been directly finalized, the initialization is still needed
            // to indirectly finalize previous remaining commits.
            self.link_blocks_in_last_commit();
            self.inherit_reject_votes_in_last_commit();
            // Try to indirectly finalize a prefix of the buffered commits.
            // The last commit cannot be indirectly finalized because there is no commit afterwards,
            // so it is excluded.
            for i in 0..(self.pending_commits.len() - 1) {
                self.try_indirect_finalize_commit(i);
                let new_finalized_commits = self.pop_finalized_commits();
                if new_finalized_commits.is_empty() {
                    // No additional commits can be indirectly finalized.
                    break;
                }
                finalized_commits.extend(new_finalized_commits);
            }
        }

        // GC TransactionCertifier state only with finalized commits, to ensure unfinalized transactions
        // can access their reject votes from TransactionCertifier.
        if let Some(last_commit) = finalized_commits.last() {
            let gc_round = DagState::calculate_gc_round(
                last_commit.leader.round,
                self.context.protocol_config.consensus_gc_depth(),
            );
            self.transaction_certifier.run_gc(gc_round);
        }

        finalized_commits
    }

    // Tries directly finalizing transactions in the last commit.
    fn try_direct_finalize_last_commit(&mut self) {
        let commit_state = self
            .pending_commits
            .back_mut()
            .unwrap_or_else(|| panic!("No pending commit."));
        // Direct commit means every transaction in the commit can be considered to have a quorum of post-commit certificates,
        // unless the transaction has reject votes that do not reach quorum either.
        let pending_blocks = std::mem::take(&mut commit_state.pending_blocks);
        for block_ref in pending_blocks {
            let reject_votes = self.transaction_certifier.get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is either incorrectly gc'ed or failed to be recovered after crash."));
            // If a transaction_index from the block does not exist in the reject_votes,
            // it means the transaction has no reject votes, so it is finalized and does not need
            // to be added to pending_transactions.
            for (transaction_index, stake) in reject_votes {
                // If the transaction has > 0 but < 2f+1 reject votes, it is still pending.
                // Otherwise, it is rejected.
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
    // and have its ancestors link to the block.
    fn link_blocks_in_last_commit(&mut self) {
        let commit_state = self
            .pending_commits
            .back_mut()
            .unwrap_or_else(|| panic!("No pending commit."));

        // Link blocks in ascending order of round, to ensure ancestor block states are created
        // before they are linked from.
        let mut blocks = commit_state.commit.blocks.clone();
        blocks.sort_by_key(|b| b.round());

        for block in blocks {
            // Initialize the block state with the reject votes contained in the block.
            let block_state = self.blocks.entry(block.reference()).or_default();
            for votes in block.transaction_votes() {
                block_state
                    .reject_votes
                    .entry(votes.block_ref)
                    .or_default()
                    .extend(votes.rejects.iter());
            }

            // Link its ancestors to the block.
            let block_ref = block.reference();
            for ancestor in block.ancestors() {
                // Ancestor may not exist in the blocks map if it has been finalized or gc'ed.
                // So skip linking if the ancestor does not exist.
                if let Some(ancestor_block) = self.blocks.get_mut(ancestor) {
                    ancestor_block.children.insert(block_ref);
                }
            }
        }
    }

    // To simplify counting accept and reject votes for finalization, make reject votes explicit
    // in each block state even if the original block does not contain an implicit reject vote.
    //
    // This means when block A and B are from the same authority and B is a direct ancestor of A,
    // all reject votes from B are also added to the block state of A.
    //
    // This computation also helps to clarify the edge case: when both A and B link to another block C
    // and B rejects a transaction in C, A should still be considered to reject the transaction in C
    // even if A does not contain an explicit reject vote for the transaction.
    fn inherit_reject_votes_in_last_commit(&mut self) {
        let commit_state = self
            .pending_commits
            .back_mut()
            .unwrap_or_else(|| panic!("No pending commit."));

        // Inherit in ascending order of round, to ensure all lower round reject votes are included.
        let mut blocks = commit_state.commit.blocks.clone();
        blocks.sort_by_key(|b| b.round());

        for block in blocks {
            // Inherit reject votes from the ancestor of block's own authority.
            // Block verification ensures the 1st ancestor is from the own authority.
            // If this is not the case, the ancestor block must have been gc'ed.
            // Also, block verification ensures each authority has at most one ancestor.
            let Some(own_ancestor) = block.ancestors().first().copied() else {
                continue;
            };
            if own_ancestor.author != block.author() {
                continue;
            }
            let Some(own_ancestor_rejects) = self
                .blocks
                .get(&own_ancestor)
                .map(|b| b.reject_votes.clone())
            else {
                // The ancestor block has been finalized or gc'ed.
                // So its reject votes are no longer needed either.
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

    // Tries indirectly finalizing the buffered commits at the given index.
    fn try_indirect_finalize_commit(&mut self, index: usize) {
        // Optional optimization: re-check pending transactions to see if they are rejected by a quorum now.
        self.check_pending_transactions(index);

        // Check if remaining pending blocks can be finalized.
        // If a block is finalized, record its pending and rejected transactions.
        // The remaining pending transactions in the block will be eventually indirectly finalized or rejected.
        self.try_indirect_finalize_pending_blocks(index);

        // Check if remaining pending transactions can be finalized.
        self.try_indirect_finalize_pending_transactions(index);

        // Check if remaining pending transactions can be indirectly rejected.
        self.try_indirect_reject_pending_transactions(index);
    }

    fn check_pending_transactions(&mut self, index: usize) {
        let blocks_with_pending_transactions =
            self.pending_commits[index].pending_transactions.clone();
        for (block_ref, pending_transactions) in blocks_with_pending_transactions {
            let reject_votes: BTreeMap<TransactionIndex, Stake> = self
                .transaction_certifier
                .get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is likely gc'ed or failed to be recovered after crash."))
                .into_iter()
                .collect();
            let curr_commit_state = &mut self.pending_commits[index];
            // Before reprocessing all pending transactions, block_pending_txns must exist and cannot be empty.
            let block_pending_txns = curr_commit_state
                .pending_transactions
                .get_mut(&block_ref)
                .unwrap();
            for transaction_index in pending_transactions {
                let reject_stake = reject_votes.get(&transaction_index).unwrap_or(&0);
                if *reject_stake < self.context.committee.quorum_threshold() {
                    continue;
                }
                // Otherwise, move the rejected transaction from pending_transactions to rejected_transactions.
                block_pending_txns.remove(&transaction_index);
                curr_commit_state
                    .rejected_transactions
                    .entry(block_ref)
                    .or_default()
                    .insert(transaction_index);
            }
            // Remove the entry for the block if the block has no pending transactions.
            if block_pending_txns.is_empty() {
                curr_commit_state.pending_transactions.remove(&block_ref);
            }
        }
    }

    fn try_indirect_finalize_pending_blocks(&mut self, index: usize) {
        let curr_leader_round = self.pending_commits[index].commit.leader.round;
        let pending_blocks = self.pending_commits[index].pending_blocks.clone();
        for block_ref in pending_blocks {
            // When a block is finalized, all transactions without reject votes are also finalized.
            // Then only transactions with reject votes need to be indirectly finalized or rejected.
            // The returned rejected transactions are ignored since no transaction is requested to be finalized anyway.
            let (block_finalized, _) =
                self.try_indirect_finalize_block(curr_leader_round, block_ref, BTreeMap::new());
            if !block_finalized {
                continue;
            }
            // Remove the finalized block from pending_blocks.
            let curr_commit_state = &mut self.pending_commits[index];
            curr_commit_state.pending_blocks.remove(&block_ref);
            // Determine if each of the remaining transactions with reject votes is pending or rejected.
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
        let curr_leader_round = self.pending_commits[index].commit.leader.round;
        let pending_transactions = self.pending_commits[index].pending_transactions.clone();
        for (block_ref, pending_transactions) in pending_transactions {
            let accept_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>> =
                pending_transactions
                    .into_iter()
                    .map(|transaction_index| (transaction_index, StakeAggregator::new()))
                    .collect();
            // Request finalizing the pending transactions in the block.
            let (_, finalized_transactions) =
                self.try_indirect_finalize_block(curr_leader_round, block_ref, accept_votes);
            if !finalized_transactions.is_empty() {
                let curr_commit_state = &mut self.pending_commits[index];
                // Remove the pending transactions in the block that are indirectly finalized.
                let block_pending_txns = curr_commit_state
                    .pending_transactions
                    .get_mut(&block_ref)
                    .unwrap();
                for t in finalized_transactions {
                    block_pending_txns.remove(&t);
                }
                if block_pending_txns.is_empty() {
                    curr_commit_state.pending_transactions.remove(&block_ref);
                }
            }
        }
    }

    fn try_indirect_reject_pending_transactions(&mut self, index: usize) {
        let curr_leader_round = self.pending_commits[index].commit.leader.round;
        let last_commit_leader_round = self.pending_commits.back().unwrap().commit.leader.round;
        if curr_leader_round + INDIRECT_REJECT_DEPTH <= last_commit_leader_round {
            let curr_commit_state = &mut self.pending_commits[index];
            // When the last leader round is no lower than INDIRECT_REJECT_DEPTH,
            // all pending blocks should have been finalized.
            assert!(curr_commit_state.pending_blocks.is_empty());
            // Pending transactions that can be indirectly finalized are already finalized.
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
    // This function is used for both finalizing blocks and transactions, so it must traverse as many blocks as possible
    // that can contribute to the block‘s and requested transactions' finalization.
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
        while let Some(commit_state) = self.pending_commits.front() {
            if !commit_state.pending_blocks.is_empty()
                || !commit_state.pending_transactions.is_empty()
            {
                break;
            }
            let commit_state = self.pending_commits.pop_front().unwrap();
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
    // Transactions rejected by a quorum or indirectly rejected, per block.
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

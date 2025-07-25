// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_config::Stake;
use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_metrics::{
    monitored_mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    monitored_scope, spawn_logged_monitored_task,
};
use parking_lot::RwLock;

use crate::{
    commit::DEFAULT_WAVE_LENGTH,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    transaction_certifier::TransactionCertifier,
    BlockAPI, CommitIndex, CommittedSubDag,
};

/// For transaction T committed at leader round R, when a new leader at round >= R + INDIRECT_REJECT_DEPTH
/// commits and T is still not finalized, T is rejected.
/// NOTE: 3 round is the minimum depth possible for indirect finalization and rejection.
pub(crate) const INDIRECT_REJECT_DEPTH: Round = 3;

/// Handle to CommitFinalizer, for sending CommittedSubDag.
pub(crate) struct CommitFinalizerHandle {
    sender: UnboundedSender<CommittedSubDag>,
}

impl CommitFinalizerHandle {
    // Sends a CommittedSubDag to CommitFinalizer, which will finalize it before sending it to execution.
    pub(crate) fn send(&self, commit: CommittedSubDag) -> ConsensusResult<()> {
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
/// For efficiency, finalization happens first for transactions without reject votes (common case).
/// The pending undecided transactions with reject votes are individually finalized or rejected.
/// When there is no more pending transactions, the commit is finalized.
///
/// This is correct because regardless if a commit leader was directly or indirectly committed,
/// every committed block can be considered finalized, because at least one leader certificate of the commit
/// will be committed, which can also serve as a certificate for the block and its transactions.
///
/// From the earliest buffered commit, pending blocks are checked to see if they are now finalized.
/// New finalized blocks are removed from the pending blocks, and its transactions are moved to the
/// finalized, rejected or pending state. If the commit now has no pending blocks or transactions,
/// the commit is finalized and popped from the buffer. The next earliest commit is then processed
/// similarly, until either the buffer becomes empty or a commit with pending blocks or transactions
/// is encountered.
pub(crate) struct CommitFinalizer {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
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
        dag_state: Arc<RwLock<DagState>>,
        transaction_certifier: TransactionCertifier,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> Self {
        Self {
            context,
            dag_state,
            transaction_certifier,
            commit_sender,
            last_processed_commit: None,
            pending_commits: VecDeque::new(),
            blocks: BTreeMap::new(),
        }
    }

    pub(crate) fn start(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_certifier: TransactionCertifier,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> CommitFinalizerHandle {
        let processor = Self::new(context, dag_state, transaction_certifier, commit_sender);
        let (sender, receiver) = unbounded_channel("consensus_commit_finalizer");
        let _handle =
            spawn_logged_monitored_task!(processor.run(receiver), "consensus_commit_finalizer");
        CommitFinalizerHandle { sender }
    }

    async fn run(mut self, mut receiver: UnboundedReceiver<CommittedSubDag>) {
        while let Some(committed_sub_dag) = receiver.recv().await {
            let finalized_commits = if self.context.protocol_config.mysticeti_fastpath() {
                self.process_commit(committed_sub_dag)
            } else {
                vec![committed_sub_dag]
            };
            if !finalized_commits.is_empty() {
                let mut dag_state = self.dag_state.write();
                if self.context.protocol_config.mysticeti_fastpath() {
                    // Records commits that have been finalized and their rejected transactions.
                    for commit in &finalized_commits {
                        dag_state.add_finalized_commit(
                            commit.commit_ref,
                            commit.rejected_transactions_by_block.clone(),
                        );
                    }
                }
                // Commits and committed blocks must be persisted to storage before sending them to Sui
                // to execute their finalized transactions.
                // Commit metadata and uncommitted blocks can be persisted more lazily because they are recoverable.
                // But for simplicity, all unpersisted commits and blocks are flushed to storage.
                dag_state.flush();
            }
            for commit in finalized_commits {
                if let Err(e) = self.commit_sender.send(commit) {
                    tracing::warn!(
                        "Failed to send to commit handler, probably due to shutdown: {e:?}"
                    );
                    return;
                }
            }
        }
    }

    fn process_commit(&mut self, committed_sub_dag: CommittedSubDag) -> Vec<CommittedSubDag> {
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

        // The prerequisite for running direct finalization on a commit is that the commit must
        // have either a quorum of leader certificates in the local DAG, or a committed leader certificate.
        //
        // A leader certificate is a finalization certificate for every block in the commit.
        // When the prerequisite holds, all blocks in the current commit can be considered finalized.
        // And any transaction in the current commit that has not observed reject votes will never be rejected.
        // So these transactions are directly finalized.
        //
        // When a commit is direct, there are a quorum of its leader certificates in the local DAG.
        //
        // When a commit is indirect, it implies one of its leader certificates is in the committed blocks.
        // So a leader certificate must exist in the local DAG as well.
        //
        // When a commit is received through commit sync and processed as certified commit, the commit might
        // not have a leader certificate in the local DAG. So a committed transaction might not observe any reject
        // vote from local DAG, although it will eventually get rejected. To finalize blocks in this commit,
        // there must be another commit with leader round >= 3 (WAVE_LENGTH) rounds above the commit leader.
        // From the indirect commit rule, a leader certificate must exist in committed blocks for the earliest commit.
        for i in 0..self.pending_commits.len() {
            let commit_state = &self.pending_commits[i];
            if commit_state.pending_blocks.is_empty() {
                // The commit has already been processed through direct finalization.
                continue;
            }
            // Direct finalization cannot happen when
            // -  This commit is remote.
            // -  And the latest commit is less than 3 (WAVE_LENGTH) rounds above this commit.
            // In this case, this commit's leader certificate is not guaranteed to be in local DAG.
            if !commit_state.commit.local_dag_has_finalization_blocks {
                let last_commit_state = self.pending_commits.back().unwrap();
                if commit_state.commit.leader.round + DEFAULT_WAVE_LENGTH
                    > last_commit_state.commit.leader.round
                {
                    break;
                }
            }
            self.try_direct_finalize_commit(i);
        }
        let direct_finalized_commits = self.pop_finalized_commits();
        self.context
            .metrics
            .node_metrics
            .finalizer_output_commits
            .with_label_values(&["direct"])
            .inc_by(direct_finalized_commits.len() as u64);
        finalized_commits.extend(direct_finalized_commits);

        // Indirect finalization: one or more commits cannot be directly finalized.
        // So the pending transactions need to be checked for indirect finalization.
        if !self.pending_commits.is_empty() {
            // Initialize the state of the last added commit for computing indirect finalization.
            //
            // As long as there are remaining commits, even if the last commit has been directly finalized,
            // its state still needs to be initialized here to help indirectly finalize previous commits.
            // This is because the last commit may have been directly finalized, but its previous commits
            // may not have been directly finalized.
            self.link_blocks_in_last_commit();
            self.inherit_reject_votes_in_last_commit();
            // Try to indirectly finalize a prefix of the buffered commits.
            // If only one commit remains, it cannot be indirectly finalized because there is no commit afterwards,
            // so it is excluded.
            while self.pending_commits.len() > 1 {
                // Stop indirect finalization when the earliest commit has not been processed
                // through direct finalization.
                if !self.pending_commits[0].pending_blocks.is_empty() {
                    break;
                }
                // Otherwise, try to indirectly finalize the earliest commit.
                self.try_indirect_finalize_first_commit();
                let indirect_finalized_commits = self.pop_finalized_commits();
                if indirect_finalized_commits.is_empty() {
                    // No additional commits can be indirectly finalized.
                    break;
                }
                self.context
                    .metrics
                    .node_metrics
                    .finalizer_output_commits
                    .with_label_values(&["indirect"])
                    .inc_by(indirect_finalized_commits.len() as u64);
                finalized_commits.extend(indirect_finalized_commits);
            }
        }

        // GC TransactionCertifier state only with finalized commits, to ensure unfinalized transactions
        // can access their reject votes from TransactionCertifier.
        if let Some(last_commit) = finalized_commits.last() {
            let gc_round = self
                .dag_state
                .read()
                .calculate_gc_round(last_commit.leader.round);
            self.transaction_certifier.run_gc(gc_round);
        }

        self.context
            .metrics
            .node_metrics
            .finalizer_buffered_commits
            .set(self.pending_commits.len() as i64);

        finalized_commits
    }

    // Tries directly finalizing transactions in the commit.
    fn try_direct_finalize_commit(&mut self, index: usize) {
        let num_commits = self.pending_commits.len();
        let commit_state = self
            .pending_commits
            .get_mut(index)
            .unwrap_or_else(|| panic!("Commit {} does not exist. len = {}", index, num_commits,));
        // Direct commit means every transaction in the commit can be considered to have a quorum of post-commit certificates,
        // unless the transaction has reject votes that do not reach quorum either.
        assert!(!commit_state.pending_blocks.is_empty());

        let metrics = &self.context.metrics.node_metrics;
        let pending_blocks = std::mem::take(&mut commit_state.pending_blocks);
        for (block_ref, num_transactions) in pending_blocks {
            let reject_votes = self.transaction_certifier.get_reject_votes(&block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is either incorrectly gc'ed or failed to be recovered after crash."));
            metrics
                .finalizer_transaction_status
                .with_label_values(&["direct_finalize"])
                .inc_by((num_transactions - reject_votes.len()) as u64);
            // If a transaction_index does not exist in reject_votes, the transaction has no reject votes.
            // So it is finalized and does not need to be added to pending_transactions.
            for (transaction_index, stake) in reject_votes {
                // If the transaction has > 0 but < 2f+1 reject votes, it is still pending.
                // Otherwise, it is rejected.
                let entry = if stake < self.context.committee.quorum_threshold() {
                    commit_state
                        .pending_transactions
                        .entry(block_ref)
                        .or_default()
                } else {
                    metrics
                        .finalizer_transaction_status
                        .with_label_values(&["direct_reject"])
                        .inc();
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
            let block_ref = block.reference();

            // Initialize the block state with the reject votes contained in the block.
            let block_state = self.blocks.entry(block_ref).or_default();
            for votes in block.transaction_votes() {
                block_state
                    .reject_votes
                    .entry(votes.block_ref)
                    .or_default()
                    .extend(votes.rejects.iter());
            }

            // Link its ancestors to the block.
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
    fn try_indirect_finalize_first_commit(&mut self) {
        // Ensure direct finalization has been attempted for the commit.
        assert!(!self.pending_commits.is_empty());
        assert!(self.pending_commits[0].pending_blocks.is_empty());

        // Optional optimization: re-check pending transactions to see if they are rejected by a quorum now.
        self.check_pending_transactions_in_first_commit();

        // Check if remaining pending transactions can be finalized.
        self.try_indirect_finalize_pending_transactions_in_first_commit();

        // Check if remaining pending transactions can be indirectly rejected.
        self.try_indirect_reject_pending_transactions_in_first_commit();
    }

    fn check_pending_transactions_in_first_commit(&mut self) {
        let mut all_rejected_transactions: Vec<(BlockRef, Vec<TransactionIndex>)> = vec![];

        // Collect all rejected transactions without modifying state
        for (block_ref, pending_transactions) in &self.pending_commits[0].pending_transactions {
            let reject_votes: BTreeMap<TransactionIndex, Stake> = self
                .transaction_certifier
                .get_reject_votes(block_ref)
                .unwrap_or_else(|| panic!("No vote info found for {block_ref}. It is incorrectly gc'ed or failed to be recovered after crash."))
                .into_iter()
                .collect();
            let mut rejected_transactions = vec![];
            for &transaction_index in pending_transactions {
                // Pending transactions should always have reject votes.
                let reject_stake = reject_votes.get(&transaction_index).copied().unwrap();
                if reject_stake < self.context.committee.quorum_threshold() {
                    // The transaction cannot be rejected yet.
                    continue;
                }
                // Otherwise, mark the transaction for rejection.
                rejected_transactions.push(transaction_index);
            }
            if !rejected_transactions.is_empty() {
                all_rejected_transactions.push((*block_ref, rejected_transactions));
            }
        }

        // Move rejected transactions from pending_transactions.
        for (block_ref, rejected_transactions) in all_rejected_transactions {
            self.context
                .metrics
                .node_metrics
                .finalizer_transaction_status
                .with_label_values(&["direct_late_reject"])
                .inc_by(rejected_transactions.len() as u64);
            let curr_commit_state = &mut self.pending_commits[0];
            curr_commit_state.remove_pending_transactions(&block_ref, &rejected_transactions);
            curr_commit_state
                .rejected_transactions
                .entry(block_ref)
                .or_default()
                .extend(rejected_transactions);
        }
    }

    fn try_indirect_finalize_pending_transactions_in_first_commit(&mut self) {
        let mut all_finalized_transactions: Vec<(BlockRef, Vec<TransactionIndex>)> = vec![];

        // Collect all finalized transactions without modifying state
        for (block_ref, pending_transactions) in &self.pending_commits[0].pending_transactions {
            let finalized_transactions = self.try_indirect_finalize_pending_transactions_in_block(
                *block_ref,
                pending_transactions.clone(),
            );
            if !finalized_transactions.is_empty() {
                all_finalized_transactions.push((*block_ref, finalized_transactions));
            }
        }

        // Apply all changes to remove finalized transactions
        for (block_ref, finalized_transactions) in all_finalized_transactions {
            self.context
                .metrics
                .node_metrics
                .finalizer_transaction_status
                .with_label_values(&["indirect_finalize"])
                .inc_by(finalized_transactions.len() as u64);
            // Remove finalized transactions from pending transactions.
            self.pending_commits[0]
                .remove_pending_transactions(&block_ref, &finalized_transactions);
        }
    }

    fn try_indirect_reject_pending_transactions_in_first_commit(&mut self) {
        let curr_leader_round = self.pending_commits[0].commit.leader.round;
        let last_commit_leader_round = self.pending_commits.back().unwrap().commit.leader.round;
        if curr_leader_round + INDIRECT_REJECT_DEPTH <= last_commit_leader_round {
            let curr_commit_state = &mut self.pending_commits[0];
            // This function is called after trying to indirectly finalize pending blocks.
            // When last commit leader round is INDIRECT_REJECT_DEPTH rounds higher or more,
            // all pending blocks should have been finalized.
            assert!(curr_commit_state.pending_blocks.is_empty());
            // This function is called after trying to indirectly finalize pending transactions.
            // All remaining pending transactions, since they are not finalized, should now be
            // indirectly rejected.
            let pending_transactions = std::mem::take(&mut curr_commit_state.pending_transactions);
            for (block_ref, pending_transactions) in pending_transactions {
                self.context
                    .metrics
                    .node_metrics
                    .finalizer_transaction_status
                    .with_label_values(&["indirect_reject"])
                    .inc_by(pending_transactions.len() as u64);
                curr_commit_state
                    .rejected_transactions
                    .entry(block_ref)
                    .or_default()
                    .extend(pending_transactions);
            }
        }
    }

    // Returns the indices of the requested pending transactions that are indirectly finalized.
    // This function is used for checking finalization of transactions, so it must traverse
    // all blocks which can contribute to the requested transactions' finalizations.
    fn try_indirect_finalize_pending_transactions_in_block(
        &self,
        pending_block_ref: BlockRef,
        pending_transactions: BTreeSet<TransactionIndex>,
    ) -> Vec<TransactionIndex> {
        if pending_transactions.is_empty() {
            return vec![];
        }
        let mut accept_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>> =
            pending_transactions
                .into_iter()
                .map(|transaction_index| (transaction_index, StakeAggregator::new()))
                .collect();
        let mut finalized_transactions = vec![];
        let mut to_visit_blocks = self
            .blocks
            .get(&pending_block_ref)
            .unwrap()
            .children
            .clone();
        let mut visited = BTreeSet::new();
        // Traverse children blocks breadth-first and accumulate accept votes for pending transactions.
        while let Some(curr_block_ref) = to_visit_blocks.pop_first() {
            if !visited.insert(curr_block_ref) {
                continue;
            }
            // Gets the reject votes from current block to the pending block.
            let curr_block_state = self.blocks.get(&curr_block_ref).unwrap_or_else(|| panic!("Block {curr_block_ref} is either incorrectly gc'ed or failed to be recovered after crash."));
            let curr_block_reject_votes = curr_block_state
                .reject_votes
                .get(&pending_block_ref)
                .cloned()
                .unwrap_or_default();
            // Because of lifetime, first collect finalized transactions, and then remove them from accept_votes.
            let mut newly_finalized = vec![];
            for (index, stake) in &mut accept_votes {
                // Since blocks inherit reject votes from their ancestors of the same authority,
                // a transaction will not receive accept votes from the authority that rejects it
                // unless the authority equivocates.
                if curr_block_reject_votes.contains(index) {
                    continue;
                }
                // add() returns true iff the total stake has reached quorum.
                if !stake.add(curr_block_ref.author, &self.context.committee) {
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
            if accept_votes.is_empty() {
                break;
            }
            // Add additional children blocks to visit.
            to_visit_blocks.extend(
                curr_block_state
                    .children
                    .iter()
                    .filter(|b| !visited.contains(*b)),
            );
        }
        finalized_transactions
    }

    fn pop_finalized_commits(&mut self) -> Vec<CommittedSubDag> {
        let mut finalized_commits = vec![];

        while let Some(commit_state) = self.pending_commits.front() {
            if !commit_state.pending_blocks.is_empty()
                || !commit_state.pending_transactions.is_empty()
            {
                // The commit is not finalized yet.
                break;
            }

            // Pop the finalized commit and set its rejected transactions.
            let commit_state = self.pending_commits.pop_front().unwrap();
            let mut commit = commit_state.commit;
            for (block_ref, rejected_transactions) in commit_state.rejected_transactions {
                commit
                    .rejected_transactions_by_block
                    .insert(block_ref, rejected_transactions.into_iter().collect());
            }

            // Clean up committed blocks.
            for block in commit.blocks.iter() {
                self.blocks.remove(&block.reference());
            }

            let round_delay = if let Some(last_commit_state) = self.pending_commits.back() {
                last_commit_state.commit.leader.round - commit.leader.round
            } else {
                0
            };
            self.context
                .metrics
                .node_metrics
                .finalizer_round_delay
                .observe(round_delay as f64);

            finalized_commits.push(commit);
        }

        finalized_commits
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending_commits.is_empty() && self.blocks.is_empty()
    }
}

struct CommitState {
    commit: CommittedSubDag,
    // Blocks pending finalization, mapped to the number of transactions in the block.
    // This field is populated by all blocks in the commit, before direct finalization.
    // After direct finalization, this field becomes empty.
    pending_blocks: BTreeMap<BlockRef, usize>,
    // Transactions pending indirect finalization.
    // This field is populated after direct finalization, if pending transactions exist.
    // Values in this field are removed as transactions are indirectly finalized or directly rejected.
    // When both pending_blocks and pending_transactions are empty, the commit is finalized.
    pending_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    // Transactions rejected by a quorum or indirectly, per block.
    rejected_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

impl CommitState {
    fn new(commit: CommittedSubDag) -> Self {
        let pending_blocks: BTreeMap<_, _> = commit
            .blocks
            .iter()
            .map(|b| (b.reference(), b.transactions().len()))
            .collect();
        assert!(!pending_blocks.is_empty());
        Self {
            commit,
            pending_blocks,
            pending_transactions: BTreeMap::new(),
            rejected_transactions: BTreeMap::new(),
        }
    }

    fn remove_pending_transactions(
        &mut self,
        block_ref: &BlockRef,
        transactions: &[TransactionIndex],
    ) {
        let Some(block_pending_txns) = self.pending_transactions.get_mut(block_ref) else {
            return;
        };
        for t in transactions {
            block_pending_txns.remove(t);
        }
        if block_pending_txns.is_empty() {
            self.pending_transactions.remove(block_ref);
        }
    }
}

#[derive(Default)]
struct BlockState {
    // Blocks which has an explicit ancestor linking to this block.
    children: BTreeSet<BlockRef>,
    // Reject votes casted by this block, and by linked ancestors from the same authority.
    reject_votes: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

#[cfg(test)]
mod tests {
    use mysten_metrics::monitored_mpsc;
    use parking_lot::RwLock;

    use crate::{
        block::BlockTransactionVotes, dag_state::DagState, linearizer::Linearizer,
        storage::mem_store::MemStore, test_dag_builder::DagBuilder, TestBlock, VerifiedBlock,
    };

    use super::*;

    struct Fixture {
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_certifier: TransactionCertifier,
        linearizer: Linearizer,
        commit_finalizer: CommitFinalizer,
    }

    impl Fixture {
        fn add_blocks(&self, blocks: Vec<VerifiedBlock>) {
            self.transaction_certifier
                .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
            self.dag_state.write().accept_blocks(blocks);
        }
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
            dag_state.clone(),
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

    fn create_block(
        round: Round,
        authority: u32,
        mut ancestors: Vec<BlockRef>,
        num_transactions: usize,
        reject_votes: Vec<BlockTransactionVotes>,
    ) -> VerifiedBlock {
        // Move own authority ancestor to the front of the ancestors.
        let i = ancestors
            .iter()
            .position(|b| b.author.value() == authority as usize)
            .unwrap_or_else(|| {
                panic!("Authority {authority} (round {round}) not found in {ancestors:?}")
            });
        let b = ancestors.remove(i);
        ancestors.insert(0, b);
        // Create test block.
        let block = TestBlock::new(round, authority)
            .set_ancestors(ancestors)
            .set_transactions(vec![crate::Transaction::new(vec![1; 16]); num_transactions])
            .set_transaction_votes(reject_votes)
            .build();
        VerifiedBlock::new_for_test(block)
    }

    #[tokio::test]
    async fn test_direct_finalize_no_reject_votes() {
        let mut fixture = create_commit_finalizer_fixture();

        // Create round 1-4 blocks with 10 transactions each. Add these blocks to transaction certifier.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder
            .layers(1..=4)
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
            .process_commit(committed_sub_dag.clone());
        assert_eq!(finalized_commits.len(), 1);
        let finalized_commit = &finalized_commits[0];
        assert_eq!(committed_sub_dag, finalized_commit);

        // CommitFinalizer should be empty.
        assert!(fixture.commit_finalizer.is_empty());
    }

    // Commits can be directly finalized if when they are added to commit finalizer,
    // the rejected votes reach quorum if they exist on any transaction.
    #[tokio::test]
    async fn test_direct_finalize_with_reject_votes() {
        let mut fixture = create_commit_finalizer_fixture();

        // Create round 1 blocks with 10 transactions each.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder
            .layer(1)
            .num_transactions(10)
            .build()
            .persist_layers(fixture.dag_state.clone());
        let round_1_blocks = dag_builder.all_blocks();
        fixture.transaction_certifier.add_voted_blocks(
            round_1_blocks
                .iter()
                .map(|b| {
                    if b.author().value() != 3 {
                        (b.clone(), vec![])
                    } else {
                        (b.clone(), vec![0, 3])
                    }
                })
                .collect(),
        );

        // Select the block with rejected transaction.
        let block_with_rejected_txn = round_1_blocks[3].clone();
        let reject_vote = BlockTransactionVotes {
            block_ref: block_with_rejected_txn.reference(),
            rejects: vec![0, 3],
        };

        // Create round 2 blocks without authority 3's block from round 1.
        let ancestors: Vec<BlockRef> = round_1_blocks[0..3].iter().map(|b| b.reference()).collect();
        // Leader links to block_with_rejected_txn, but other blocks do not.
        let round_2_blocks = vec![
            create_block(
                2,
                0,
                round_1_blocks.iter().map(|b| b.reference()).collect(),
                10,
                vec![reject_vote.clone()],
            ),
            create_block(2, 1, ancestors.clone(), 10, vec![]),
            create_block(2, 2, ancestors.clone(), 10, vec![]),
        ];
        fixture.add_blocks(round_2_blocks.clone());

        // Select round 2 authority 0 block as the leader and create CommittedSubDag.
        let leader = round_2_blocks[0].clone();
        let committed_sub_dags = fixture.linearizer.handle_commit(vec![leader.clone()]);
        assert_eq!(committed_sub_dags.len(), 1);
        let committed_sub_dag = &committed_sub_dags[0];
        assert_eq!(committed_sub_dag.blocks.len(), 5);

        // Create round 3 blocks voting on the leader.
        let ancestors: Vec<BlockRef> = round_2_blocks.iter().map(|b| b.reference()).collect();
        let round_3_blocks = vec![
            create_block(3, 0, ancestors.clone(), 0, vec![]),
            create_block(3, 1, ancestors.clone(), 0, vec![reject_vote.clone()]),
            create_block(3, 2, ancestors.clone(), 0, vec![reject_vote.clone()]),
            create_block(
                3,
                3,
                std::iter::once(round_1_blocks[3].reference())
                    .chain(ancestors.clone())
                    .collect(),
                0,
                vec![reject_vote.clone()],
            ),
        ];
        fixture.add_blocks(round_3_blocks.clone());

        // Create round 4 blocks certifying the leader.
        let ancestors: Vec<BlockRef> = round_3_blocks.iter().map(|b| b.reference()).collect();
        let round_4_blocks = vec![
            create_block(4, 0, ancestors.clone(), 0, vec![]),
            create_block(4, 1, ancestors.clone(), 0, vec![]),
            create_block(4, 2, ancestors.clone(), 0, vec![]),
            create_block(4, 3, ancestors.clone(), 0, vec![]),
        ];
        fixture.add_blocks(round_4_blocks.clone());

        // This committed sub-dag can be directly finalized because the rejected transactions
        // have a quorum of votes.
        let finalized_commits = fixture
            .commit_finalizer
            .process_commit(committed_sub_dag.clone());
        assert_eq!(finalized_commits.len(), 1);
        let finalized_commit = &finalized_commits[0];
        assert_eq!(committed_sub_dag.commit_ref, finalized_commit.commit_ref);
        assert_eq!(committed_sub_dag.blocks, finalized_commit.blocks);
        assert_eq!(finalized_commit.rejected_transactions_by_block.len(), 1);
        assert_eq!(
            finalized_commit
                .rejected_transactions_by_block
                .get(&block_with_rejected_txn.reference())
                .unwrap()
                .clone(),
            vec![0, 3],
        );

        // CommitFinalizer should be empty.
        assert!(fixture.commit_finalizer.is_empty());
    }

    // Test indirect finalization when:
    // 1. Reject votes on transaction does not reach quorum initially, but reach quorum later.
    // 2. Transaction is indirectly rejected.
    // 3. Transaction is indirectly finalized.
    #[tokio::test]
    async fn test_indirect_finalize_with_reject_votes() {
        let mut fixture = create_commit_finalizer_fixture();

        // Create round 1 blocks with 10 transactions each.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder
            .layer(1)
            .num_transactions(10)
            .build()
            .persist_layers(fixture.dag_state.clone());
        let round_1_blocks = dag_builder.all_blocks();
        fixture.transaction_certifier.add_voted_blocks(
            round_1_blocks
                .iter()
                .map(|b| {
                    if b.author().value() != 3 {
                        (b.clone(), vec![])
                    } else {
                        (b.clone(), vec![0, 3])
                    }
                })
                .collect(),
        );

        // Select the block with rejected transaction.
        let block_with_rejected_txn = round_1_blocks[3].clone();
        // How transactions in this block will be voted:
        // Txn 1 (quorum reject): 1 reject vote at round 2, 1 reject vote at round 3, and 1 at round 4.
        // Txn 4 (indirect reject): 1 reject vote at round 3, and 1 at round 4.
        // Txn 7 (indirect finalize): 1 reject vote at round 3.

        // Create round 2 blocks without authority 3.
        let ancestors: Vec<BlockRef> = round_1_blocks[0..3].iter().map(|b| b.reference()).collect();
        // Leader links to block_with_rejected_txn, but other blocks do not.
        let round_2_blocks = vec![
            create_block(
                2,
                0,
                round_1_blocks.iter().map(|b| b.reference()).collect(),
                10,
                vec![BlockTransactionVotes {
                    block_ref: block_with_rejected_txn.reference(),
                    rejects: vec![1, 4],
                }],
            ),
            // Use ancestors without authority 3 to avoid voting on its transactions.
            create_block(2, 1, ancestors.clone(), 10, vec![]),
            create_block(2, 2, ancestors.clone(), 10, vec![]),
        ];
        fixture.add_blocks(round_2_blocks.clone());

        // Select round 2 authority 0 block as the a leader.
        let mut leaders = vec![round_2_blocks[0].clone()];

        // Create round 3 blocks voting on the leader and casting reject votes.
        let ancestors: Vec<BlockRef> = round_2_blocks.iter().map(|b| b.reference()).collect();
        let round_3_blocks = vec![
            create_block(3, 0, ancestors.clone(), 0, vec![]),
            create_block(
                3,
                1,
                ancestors.clone(),
                0,
                vec![BlockTransactionVotes {
                    block_ref: block_with_rejected_txn.reference(),
                    rejects: vec![1, 4, 7],
                }],
            ),
            create_block(
                3,
                3,
                std::iter::once(round_1_blocks[3].reference())
                    .chain(ancestors.clone())
                    .collect(),
                0,
                vec![],
            ),
        ];
        fixture.add_blocks(round_3_blocks.clone());
        leaders.push(round_3_blocks[2].clone());

        // Create round 4 blocks certifying the leader and casting reject votes.
        let ancestors: Vec<BlockRef> = round_3_blocks.iter().map(|b| b.reference()).collect();
        let round_4_blocks = vec![
            create_block(4, 0, ancestors.clone(), 0, vec![]),
            create_block(4, 1, ancestors.clone(), 0, vec![]),
            create_block(
                4,
                2,
                std::iter::once(round_2_blocks[2].reference())
                    .chain(ancestors.clone())
                    .collect(),
                0,
                vec![BlockTransactionVotes {
                    block_ref: block_with_rejected_txn.reference(),
                    rejects: vec![1],
                }],
            ),
            create_block(4, 3, ancestors.clone(), 0, vec![]),
        ];
        fixture.add_blocks(round_4_blocks.clone());
        leaders.push(round_4_blocks[1].clone());

        // Create round 5-7 blocks without casting reject votes.
        // Select the last leader from round 5. It is necessary to have round 5 leader to indirectly finalize
        // transactions committed by round 2 leader.
        let mut last_round_blocks = round_4_blocks.clone();
        for r in 5..=7 {
            let ancestors: Vec<BlockRef> =
                last_round_blocks.iter().map(|b| b.reference()).collect();
            let round_blocks: Vec<_> = (0..4)
                .map(|i| create_block(r, i, ancestors.clone(), 0, vec![]))
                .collect();
            fixture.add_blocks(round_blocks.clone());
            if r == 5 {
                leaders.push(round_blocks[0].clone());
            }
            last_round_blocks = round_blocks;
        }

        // Create CommittedSubDag from leaders.
        assert_eq!(leaders.len(), 4);
        let committed_sub_dags = fixture.linearizer.handle_commit(leaders);
        assert_eq!(committed_sub_dags.len(), 4);

        // Buffering the initial 3 commits should not finalize.
        for commit in committed_sub_dags.iter().take(3) {
            let finalized_commits = fixture.commit_finalizer.process_commit(commit.clone());
            assert_eq!(finalized_commits.len(), 0);
        }

        // Buffering the 4th commit should finalize all commits.
        let finalized_commits = fixture
            .commit_finalizer
            .process_commit(committed_sub_dags[3].clone());
        assert_eq!(finalized_commits.len(), 4);

        // Check rejected transactions.
        let rejected_transactions = finalized_commits[0].rejected_transactions_by_block.clone();
        assert_eq!(rejected_transactions.len(), 1);
        assert_eq!(
            rejected_transactions
                .get(&block_with_rejected_txn.reference())
                .unwrap(),
            &vec![1, 4]
        );

        // Other commits should have no rejected transactions.
        for commit in finalized_commits.iter().skip(1) {
            assert!(commit.rejected_transactions_by_block.is_empty());
        }

        // CommitFinalizer should be empty.
        assert!(fixture.commit_finalizer.is_empty());
    }

    #[tokio::test]
    async fn test_finalize_remote_commits_with_reject_votes() {
        let mut fixture: Fixture = create_commit_finalizer_fixture();
        let mut all_blocks = vec![];

        // Create round 1 blocks with 10 transactions each.
        let mut dag_builder = DagBuilder::new(fixture.context.clone());
        dag_builder.layer(1).num_transactions(10).build();
        let round_1_blocks = dag_builder.all_blocks();
        all_blocks.push(round_1_blocks.clone());

        // Collect leaders from round 1.
        let mut leaders = vec![round_1_blocks[0].clone()];

        // Create round 2-9 blocks and set leaders until round 7.
        let mut last_round_blocks = round_1_blocks.clone();
        for r in 2..=9 {
            let ancestors: Vec<BlockRef> =
                last_round_blocks.iter().map(|b| b.reference()).collect();
            let round_blocks: Vec<_> = (0..4)
                .map(|i| create_block(r, i, ancestors.clone(), 0, vec![]))
                .collect();
            all_blocks.push(round_blocks.clone());
            if r <= 7 && r != 5 {
                leaders.push(round_blocks[r as usize % 4].clone());
            }
            last_round_blocks = round_blocks;
        }

        // Leader rounds: 1, 2, 3, 4, 6, 7.
        assert_eq!(leaders.len(), 6);

        let mut add_blocks_and_process_commit =
            |index: usize, local: bool| -> Vec<CommittedSubDag> {
                let leader = leaders[index].clone();
                // Add blocks related to the commit to DagState and TransactionCertifier.
                if local {
                    for round_blocks in all_blocks.iter().take(leader.round() as usize + 2) {
                        fixture.add_blocks(round_blocks.clone());
                    }
                } else {
                    for round_blocks in all_blocks.iter().take(leader.round() as usize) {
                        fixture.add_blocks(round_blocks.clone());
                    }
                };
                // Generate remote commit from leader.
                let mut committed_sub_dags = fixture.linearizer.handle_commit(vec![leader]);
                assert_eq!(committed_sub_dags.len(), 1);
                let mut remote_commit = committed_sub_dags.pop().unwrap();
                remote_commit.local_dag_has_finalization_blocks = local;
                // Process the remote commit.
                fixture
                    .commit_finalizer
                    .process_commit(remote_commit.clone())
            };

        // Add commit 1-3 as remote commits. There should be no finalized commits.
        for i in 0..3 {
            let finalized_commits = add_blocks_and_process_commit(i, false);
            assert!(finalized_commits.is_empty());
        }

        // Buffer round 4 commit as a remote commit. This should finalize the 1st commit at round 1.
        let finalized_commits = add_blocks_and_process_commit(3, false);
        assert_eq!(finalized_commits.len(), 1);
        assert_eq!(finalized_commits[0].commit_ref.index, 1);
        assert_eq!(finalized_commits[0].leader.round, 1);

        // Buffer round 6 (5th) commit as local commit. This should help finalize the commits at round 2 and 3.
        let finalized_commits = add_blocks_and_process_commit(4, true);
        assert_eq!(finalized_commits.len(), 2);
        assert_eq!(finalized_commits[0].commit_ref.index, 2);
        assert_eq!(finalized_commits[0].leader.round, 2);
        assert_eq!(finalized_commits[1].commit_ref.index, 3);
        assert_eq!(finalized_commits[1].leader.round, 3);

        // Buffer round 7 (6th) commit as local commit. This should help finalize the commits at round 4, 6 and 7 (itself).
        let finalized_commits = add_blocks_and_process_commit(5, true);
        assert_eq!(finalized_commits.len(), 3);
        assert_eq!(finalized_commits[0].commit_ref.index, 4);
        assert_eq!(finalized_commits[0].leader.round, 4);
        assert_eq!(finalized_commits[1].commit_ref.index, 5);
        assert_eq!(finalized_commits[1].leader.round, 6);
        assert_eq!(finalized_commits[2].commit_ref.index, 6);
        assert_eq!(finalized_commits[2].leader.round, 7);

        // CommitFinalizer should be empty.
        assert!(fixture.commit_finalizer.is_empty());
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_metrics::{
    monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    monitored_scope, spawn_logged_monitored_task,
};
use parking_lot::RwLock;

use crate::{
    BlockAPI, CommitIndex, CommittedSubDag, VerifiedBlock,
    commit_finalizer::{CommitFinalizerHandle, persist_finalized_commits},
    context::Context,
    dag_state::DagState,
    leader_slot_decider::INDIRECT_COMMIT_DEPTH,
    stake_aggregator::{
        CertificationThreshold, CommitteeThreshold, QuorumThreshold, StakeAggregator,
    },
    transaction_vote_tracker::TransactionVoteTracker,
};

/// Finalizes transactions in committed sub-DAGs under the Mysticeti v3 transaction voting rules.
///
/// Consensus sends committed sub-DAGs in commit order. Consensus has sequenced the transactions,
/// but it has not yet decided whether each transaction is accepted or rejected. The finalizer
/// buffers these commits and resolves each transaction from its transaction votes.
///
/// The finalizer uses direct and indirect finalizations:
///
/// - Direct finalization uses local descendants as implicit accept votes. For a target block in a
///   commit with leader round L, the first descendant on each authority chain votes through round
///   L + 1. It gets explicit reject votes from [`TransactionVoteTracker`]. The finalizer retries
///   this rule for pending commits when it receives a new commit.
/// - Indirect finalization checks pending transactions when later commits enter the queue. It uses
///   the same voting window, but it uses only committed descendants. It accepts a transaction when
///   the accept stake reaches the certification threshold. When a committed anchor reaches the
///   required depth, it rejects all other pending transactions.
///
/// The finalizer keeps a commit in its pending queue until every transaction in the commit has a decision.
pub(crate) struct CommitFinalizerV3 {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    transaction_vote_tracker: TransactionVoteTracker,
    commit_sender: UnboundedSender<CommittedSubDag>,

    last_processed_commit: Option<CommitIndex>,
    pending_commits: VecDeque<CommitStateV3>,
}

impl CommitFinalizerV3 {
    pub(crate) fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_vote_tracker: TransactionVoteTracker,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> Self {
        assert!(
            context.protocol_config.enable_v3(),
            "CommitFinalizerV3 requires Mysticeti v3"
        );
        assert!(
            context.protocol_config.gc_depth() > INDIRECT_COMMIT_DEPTH,
            "Mysticeti v3 GC depth must be greater than {INDIRECT_COMMIT_DEPTH}"
        );
        // The indirect proof requires a continuous CommittedSubDag input stream
        // that contains all required accept-vote blocks before the depth-two
        // trigger. The local-commit and commit-sync paths must preserve this
        // evidence across GC.
        Self {
            context,
            dag_state,
            transaction_vote_tracker,
            commit_sender,
            last_processed_commit: None,
            pending_commits: VecDeque::new(),
        }
    }

    pub(crate) fn start(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_vote_tracker: TransactionVoteTracker,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> CommitFinalizerHandle {
        let processor = Self::new(context, dag_state, transaction_vote_tracker, commit_sender);
        let (sender, receiver) = unbounded_channel("consensus_commit_finalizer");
        let task =
            spawn_logged_monitored_task!(processor.run(receiver), "consensus_commit_finalizer");
        CommitFinalizerHandle::new(sender, task)
    }

    async fn run(mut self, mut receiver: UnboundedReceiver<CommittedSubDag>) {
        while let Some(committed_sub_dag) = receiver.recv().await {
            let already_finalized = !self.context.protocol_config.transaction_voting_enabled()
                || committed_sub_dag.recovered_rejected_transactions;
            let finalized_commits = if already_finalized {
                vec![committed_sub_dag]
            } else {
                self.process_commit(committed_sub_dag)
            };
            persist_finalized_commits(
                &self.dag_state,
                &self.transaction_vote_tracker,
                &finalized_commits,
                !already_finalized,
            );
            for commit in finalized_commits {
                if let Err(error) = self.commit_sender.send(commit) {
                    tracing::warn!(
                        "Failed to send to commit handler, probably due to shutdown: {error:?}"
                    );
                    return;
                }
            }
        }
    }

    pub(crate) fn process_commit(
        &mut self,
        committed_sub_dag: CommittedSubDag,
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
            .push_back(CommitStateV3::new(committed_sub_dag));

        // Direct finalization applies these steps to every pending block B in a commit whose
        // leader is at round L:
        //
        // 1. The validator traverses local descendants of B through round L + 1.
        // 2. The first block on each authority chain whose causal history includes B casts a vote.
        //    A cutoff that covers B or an explicit reject prevents an implicit accept. Later blocks
        //    on the same authority chain do not vote again. Each side counts an authority once.
        // 3. It reads explicit reject stake from the transaction vote tracker.
        // 4. It accepts the transaction when accept stake reaches quorum. It rejects the
        //    transaction when reject stake reaches quorum. Otherwise, the transaction stays
        //    pending.
        //
        // DagState keeps the direct child links for local blocks.
        for index in 0..self.pending_commits.len() {
            self.try_direct_finalize_commit(index);
        }

        let mut finalized_commits = self.pop_finalized_commits();
        self.context
            .metrics
            .node_metrics
            .finalizer_output_commits
            .with_label_values(&["direct"])
            .inc_by(finalized_commits.len() as u64);

        // Indirect finalization gives a second way to resolve pending transactions. As soon as one
        // later commit exists, the validator checks the earliest pending commit with committed
        // evidence.
        //
        // For each pending transaction in block B, the validator traverses committed descendants
        // through the round after B's commit leader. It applies the same first-vote rule as direct
        // finalization. It accepts the transaction when this stake reaches the certification
        // threshold. The direct pass above has already applied current explicit reject quorums.
        //
        // A commit must leave the queue when the newest leader round is
        // INDIRECT_COMMIT_DEPTH rounds above its leader. At that point, any direct accept quorum
        // must leave an accept certificate in the committed prefix. The validator rejects every
        // transaction that is still pending after the certificate check.
        if self.pending_commits.len() > 1 {
            let committed_voting_graph = CommittedBlockGraph::new(
                self.pending_commits
                    .iter()
                    .flat_map(|state| state.commit.blocks.iter().cloned()),
            );
            while self.pending_commits.len() > 1 {
                let first_leader_round = self.pending_commits.front().unwrap().commit.leader.round;
                let anchor_round = self.pending_commits.back().unwrap().commit.leader.round;
                let reject_remaining =
                    first_leader_round.saturating_add(INDIRECT_COMMIT_DEPTH) <= anchor_round;

                self.try_indirect_finalize_first_commit(&committed_voting_graph, reject_remaining);
                let indirect_finalized_commits = self.pop_finalized_commits();
                if indirect_finalized_commits.is_empty() {
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

        self.report_finalization_latency(&finalized_commits);
        self.context
            .metrics
            .node_metrics
            .finalizer_buffered_commits
            .set(self.pending_commits.len() as i64);

        finalized_commits
    }

    fn try_direct_finalize_commit(&mut self, commit_index: usize) {
        let last_voting_round = self.pending_commits[commit_index]
            .commit
            .leader
            .round
            .saturating_add(1);
        let pending_transactions = self.pending_commits[commit_index]
            .pending_transactions
            .clone();
        for (block_ref, transaction_indices) in pending_transactions {
            let first_votes = {
                let dag_state = self.dag_state.read();
                collect_first_votes(&*dag_state, block_ref, last_voting_round)
            };
            let decisions =
                self.compute_direct_decisions(block_ref, &transaction_indices, &first_votes);
            self.apply_decisions(
                commit_index,
                block_ref,
                decisions,
                "direct_finalize",
                "direct_reject",
            );
        }
    }

    fn compute_direct_decisions(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        first_votes: &[VotingBlock],
    ) -> TransactionDecisions {
        let reject_stake_by_transaction: BTreeMap<_, _> = self
            .transaction_vote_tracker
            .get_reject_votes(&block_ref)
            .unwrap_or_else(|| {
                panic!(
                    "No vote info found for {block_ref}. It is incorrectly GC'ed or failed to be recovered after crash."
                )
            })
            .into_iter()
            .collect();

        let accept_votes: AcceptVotes<QuorumThreshold> =
            self.collect_accept_votes(block_ref, transaction_indices, first_votes);

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = accept_votes.for_transaction(*transaction_index);
            let accepted = transaction_accept_votes.reached_threshold(&self.context.committee);
            let reject_stake = reject_stake_by_transaction
                .get(transaction_index)
                .copied()
                .unwrap_or_default();
            let rejected = reject_stake >= self.context.committee.quorum_threshold();
            assert!(
                !(accepted && rejected),
                "Transaction {} in block {} cannot have both accept and reject quorums. Accept voters: {:?}, reject stake: {}",
                transaction_index,
                block_ref,
                transaction_accept_votes.authorities(),
                reject_stake,
            );
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if rejected {
                decisions.rejected.push(*transaction_index);
            }
        }
        decisions
    }

    fn try_indirect_finalize_first_commit(
        &mut self,
        committed_voting_graph: &CommittedBlockGraph,
        reject_remaining: bool,
    ) {
        let pending_transactions = self.pending_commits[0].pending_transactions.clone();
        let last_voting_round = self.pending_commits[0]
            .commit
            .leader
            .round
            .saturating_add(1);

        for (block_ref, transaction_indices) in pending_transactions {
            // An accept voter is a descendant of the target block. Thus, it cannot commit before
            // the target block. The pending commit prefix contains every committed accept voter.
            let decisions = self.compute_indirect_decisions(
                block_ref,
                &transaction_indices,
                committed_voting_graph,
                last_voting_round,
                reject_remaining,
            );
            self.apply_decisions(
                0,
                block_ref,
                decisions,
                "indirect_finalize",
                "indirect_reject",
            );
        }
    }

    fn compute_indirect_decisions(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        committed_voting_graph: &CommittedBlockGraph,
        last_voting_round: Round,
        reject_remaining: bool,
    ) -> TransactionDecisions {
        let first_votes = collect_first_votes(committed_voting_graph, block_ref, last_voting_round);
        let accept_votes: AcceptVotes<CertificationThreshold> =
            self.collect_accept_votes(block_ref, transaction_indices, &first_votes);

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = accept_votes.for_transaction(*transaction_index);
            let accepted = transaction_accept_votes.reached_threshold(&self.context.committee);
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if reject_remaining {
                // At depth two, any direct accept quorum must leave an accept certificate in the
                // committed prefix. Therefore, no certificate means that rejection is safe.
                decisions.rejected.push(*transaction_index);
            }
        }
        decisions
    }

    fn collect_accept_votes<T: CommitteeThreshold>(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        first_votes: &[VotingBlock],
    ) -> AcceptVotes<T> {
        // Most transactions have no explicit reject. Use one shared aggregator for them, and
        // create transaction-specific aggregators only when a first vote contains a reject.
        let mut shared = StakeAggregator::<T>::new();
        let mut transactions_with_explicit_rejects = BTreeSet::new();
        for voting_block in first_votes {
            if !voting_block.can_accept(block_ref) {
                continue;
            }
            shared.add_unique(voting_block.block_ref.author, &self.context.committee);
            if let Some(explicit_rejects) = voting_block.explicit_rejects.get(&block_ref) {
                transactions_with_explicit_rejects
                    .extend(explicit_rejects.intersection(transaction_indices).copied());
            }
        }

        // Count transaction-specific accept votes instead of subtracting reject voters from the
        // shared aggregator. An equivocating authority can reject on one branch and accept on
        // another branch.
        let mut by_transaction: BTreeMap<_, _> = transactions_with_explicit_rejects
            .iter()
            .map(|transaction_index| (*transaction_index, StakeAggregator::<T>::new()))
            .collect();
        for voting_block in first_votes {
            for transaction_index in voting_block
                .select_accepted_transactions(block_ref, &transactions_with_explicit_rejects)
            {
                by_transaction
                    .get_mut(&transaction_index)
                    .expect("Accept votes are collected only for explicitly rejected transactions")
                    .add_unique(voting_block.block_ref.author, &self.context.committee);
            }
        }

        AcceptVotes {
            shared,
            by_transaction,
        }
    }

    fn apply_decisions(
        &mut self,
        commit_index: usize,
        block_ref: BlockRef,
        decisions: TransactionDecisions,
        accepted_label: &str,
        rejected_label: &str,
    ) {
        if decisions.accepted.is_empty() && decisions.rejected.is_empty() {
            return;
        }

        let metrics = &self.context.metrics.node_metrics;
        metrics
            .finalizer_transaction_status
            .with_label_values(&[accepted_label])
            .inc_by(decisions.accepted.len() as u64);
        metrics
            .finalizer_transaction_status
            .with_label_values(&[rejected_label])
            .inc_by(decisions.rejected.len() as u64);

        let commit_state = &mut self.pending_commits[commit_index];
        commit_state.remove_pending_transactions(&block_ref, &decisions.accepted);
        commit_state.remove_pending_transactions(&block_ref, &decisions.rejected);
        if !decisions.rejected.is_empty() {
            commit_state
                .rejected_transactions
                .entry(block_ref)
                .or_default()
                .extend(decisions.rejected);
        }
    }

    fn pop_finalized_commits(&mut self) -> Vec<CommittedSubDag> {
        let mut finalized_commits = vec![];
        while self
            .pending_commits
            .front()
            .is_some_and(|state| state.pending_transactions.is_empty())
        {
            let commit_state = self.pending_commits.pop_front().unwrap();
            let mut commit = commit_state.commit;
            for (block_ref, rejected_transactions) in commit_state.rejected_transactions {
                commit
                    .rejected_transactions_by_block
                    .insert(block_ref, rejected_transactions.into_iter().collect());
            }

            let round_delay = self
                .pending_commits
                .back()
                .map(|last| last.commit.leader.round.saturating_sub(commit.leader.round))
                .unwrap_or_default();
            self.context
                .metrics
                .node_metrics
                .finalizer_round_delay
                .observe(round_delay as f64);
            finalized_commits.push(commit);
        }
        finalized_commits
    }

    fn report_finalization_latency(&self, finalized_commits: &[CommittedSubDag]) {
        let utc_now = self.context.clock.timestamp_utc_ms();
        for block in finalized_commits
            .iter()
            .flat_map(|commit| &commit.blocks)
            .filter(|block| block.author() == self.context.own_index)
        {
            let latency_ms = utc_now.saturating_sub(block.timestamp_ms());
            self.context
                .metrics
                .node_metrics
                .proposed_block_finalization_latency
                .observe(Duration::from_millis(latency_ms).as_secs_f64());
        }
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending_commits.is_empty()
    }
}

trait ReverseBlockGraph {
    fn block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock>;

    fn children(&self, block_ref: &BlockRef) -> Vec<BlockRef>;
}

impl ReverseBlockGraph for DagState {
    fn block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock> {
        self.get_block(block_ref)
    }

    fn children(&self, block_ref: &BlockRef) -> Vec<BlockRef> {
        self.get_block_children(block_ref).unwrap_or_default()
    }
}

struct CommittedBlockGraph {
    blocks: BTreeMap<BlockRef, VerifiedBlock>,
    children: BTreeMap<BlockRef, BTreeSet<BlockRef>>,
}

impl CommittedBlockGraph {
    fn new(blocks: impl IntoIterator<Item = VerifiedBlock>) -> Self {
        let blocks: BTreeMap<_, _> = blocks
            .into_iter()
            .map(|block| (block.reference(), block))
            .collect();

        let mut children: BTreeMap<BlockRef, BTreeSet<BlockRef>> = BTreeMap::new();
        for block in blocks.values() {
            for ancestor in block.ancestors() {
                children
                    .entry(*ancestor)
                    .or_default()
                    .insert(block.reference());
            }
        }

        Self { blocks, children }
    }
}

impl ReverseBlockGraph for CommittedBlockGraph {
    fn block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock> {
        self.blocks.get(block_ref).cloned()
    }

    fn children(&self, block_ref: &BlockRef) -> Vec<BlockRef> {
        self.children
            .get(block_ref)
            .map(|children| children.iter().copied().collect())
            .unwrap_or_default()
    }
}

fn collect_first_votes(
    graph: &impl ReverseBlockGraph,
    block_ref: BlockRef,
    last_voting_round: Round,
) -> Vec<VotingBlock> {
    let mut to_visit: BTreeSet<_> = graph.children(&block_ref).into_iter().collect();
    let mut visited = BTreeSet::new();
    let mut ignored = BTreeSet::new();
    let mut first_votes = vec![];

    // BlockRef ordering visits lower rounds first. A reachable block votes unless an earlier block
    // on its own-authority chain has voted. The traversal still follows ignored blocks because
    // their descendants from other authorities can cast first votes.
    while let Some(current_ref) = to_visit.pop_first() {
        if current_ref.round > last_voting_round || !visited.insert(current_ref) {
            continue;
        }
        if !ignored.contains(&current_ref) {
            let current_block = graph
                .block(&current_ref)
                .unwrap_or_else(|| panic!("No block data found for voting block {current_ref}"));
            first_votes.push(VotingBlock::new(current_block));
            ignored.insert(current_ref);
            ignore_origin_descendants(graph, current_ref, last_voting_round, &mut ignored);
        }
        to_visit.extend(
            graph
                .children(&current_ref)
                .into_iter()
                .filter(|child| !visited.contains(child)),
        );
    }

    first_votes
}

fn ignore_origin_descendants(
    graph: &impl ReverseBlockGraph,
    block_ref: BlockRef,
    last_voting_round: Round,
    ignored: &mut BTreeSet<BlockRef>,
) {
    let mut to_visit: BTreeSet<_> = graph
        .children(&block_ref)
        .into_iter()
        .filter(|child| child.author == block_ref.author)
        .collect();
    let mut visited = BTreeSet::new();
    while let Some(current_ref) = to_visit.pop_first() {
        if current_ref.round > last_voting_round || !visited.insert(current_ref) {
            continue;
        }
        ignored.insert(current_ref);
        to_visit.extend(
            graph
                .children(&current_ref)
                .into_iter()
                .filter(|child| child.author == block_ref.author && !visited.contains(child)),
        );
    }
}

struct VotingBlock {
    block_ref: BlockRef,
    cutoff_round: Round,
    explicit_rejects: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

impl VotingBlock {
    fn new(block: VerifiedBlock) -> Self {
        let mut explicit_rejects: BTreeMap<BlockRef, BTreeSet<TransactionIndex>> = BTreeMap::new();
        for votes in block.transaction_votes() {
            explicit_rejects
                .entry(votes.block_ref)
                .or_default()
                .extend(votes.rejects.iter().copied());
        }
        Self {
            block_ref: block.reference(),
            cutoff_round: block.transaction_votes_cutoff_round(),
            explicit_rejects,
        }
    }

    fn can_accept(&self, block_ref: BlockRef) -> bool {
        block_ref.round > self.cutoff_round
    }

    fn select_accepted_transactions(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
    ) -> Vec<TransactionIndex> {
        if !self.can_accept(block_ref) {
            return vec![];
        }
        let Some(rejects) = self.explicit_rejects.get(&block_ref) else {
            return transaction_indices.iter().copied().collect();
        };
        transaction_indices.difference(rejects).copied().collect()
    }
}

struct AcceptVotes<T> {
    shared: StakeAggregator<T>,
    by_transaction: BTreeMap<TransactionIndex, StakeAggregator<T>>,
}

impl<T> AcceptVotes<T> {
    fn for_transaction(&self, transaction_index: TransactionIndex) -> &StakeAggregator<T> {
        self.by_transaction
            .get(&transaction_index)
            .unwrap_or(&self.shared)
    }
}

#[derive(Default)]
struct TransactionDecisions {
    accepted: Vec<TransactionIndex>,
    rejected: Vec<TransactionIndex>,
}

struct CommitStateV3 {
    commit: CommittedSubDag,
    pending_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    rejected_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

impl CommitStateV3 {
    fn new(commit: CommittedSubDag) -> Self {
        let pending_transactions = commit
            .blocks
            .iter()
            .filter(|block| !block.transactions().is_empty())
            .map(|block| {
                (
                    block.reference(),
                    (0..block.transactions().len() as TransactionIndex).collect(),
                )
            })
            .collect();
        Self {
            commit,
            pending_transactions,
            rejected_transactions: BTreeMap::new(),
        }
    }

    fn remove_pending_transactions(
        &mut self,
        block_ref: &BlockRef,
        transaction_indices: &[TransactionIndex],
    ) {
        let Some(pending_transactions) = self.pending_transactions.get_mut(block_ref) else {
            return;
        };
        for transaction_index in transaction_indices {
            assert!(
                pending_transactions.remove(transaction_index),
                "Transaction {transaction_index} in block {block_ref} is not pending"
            );
        }
        if pending_transactions.is_empty() {
            self.pending_transactions.remove(block_ref);
        }
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::{AuthorityIndex, Committee};

    use crate::{
        Transaction,
        block::{BlockTransactionVotes, TestBlock, genesis_blocks},
        block_verifier::NoopBlockVerifier,
        commit::{CommitDigest, CommitRef},
        storage::{Store, mem_store::MemStore},
    };

    use super::*;

    const COMMITTEE_SIZE: usize = 6;

    struct Fixture {
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        store: Arc<MemStore>,
        transaction_vote_tracker: TransactionVoteTracker,
        finalizer: CommitFinalizerV3,
    }

    impl Fixture {
        fn new() -> Self {
            let (mut context, _) = Context::new_with_test_options(COMMITTEE_SIZE, false);
            let committee = Committee::new_v3(
                context.committee.epoch(),
                context.committee.authorities_slice().to_vec(),
                1,
                0,
            );
            context = context.with_committee(committee);
            context.protocol_config.set_enable_v3_for_testing(true);
            let context = Arc::new(context);
            assert_eq!(context.committee.quorum_threshold(), 5);
            assert_eq!(context.committee.certification_threshold(), 3);

            let store = Arc::new(MemStore::new());
            let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
            let transaction_vote_tracker = TransactionVoteTracker::new(
                context.clone(),
                Arc::new(NoopBlockVerifier),
                dag_state.clone(),
            );
            let (commit_sender, _commit_receiver) = unbounded_channel("finalizer_v3_test");
            let finalizer = CommitFinalizerV3::new(
                context.clone(),
                dag_state.clone(),
                transaction_vote_tracker.clone(),
                commit_sender,
            );

            Self {
                context,
                dag_state,
                store,
                transaction_vote_tracker,
                finalizer,
            }
        }

        fn make_round_one_blocks(&self, transaction_counts: &[usize]) -> Vec<VerifiedBlock> {
            let genesis_refs: Vec<_> = genesis_blocks(&self.context)
                .into_iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..COMMITTEE_SIZE as u32)
                .map(|author| {
                    let transactions = vec![
                        Transaction::new(vec![1]);
                        transaction_counts
                            .get(author as usize)
                            .copied()
                            .unwrap_or_default()
                    ];
                    VerifiedBlock::new_for_test(
                        TestBlock::new(1, author)
                            .set_ancestors(genesis_refs.clone())
                            .set_transactions(transactions)
                            .build_v3(0),
                    )
                })
                .collect();
            self.add_blocks(&blocks);
            blocks
        }

        fn make_round_one(&self, num_target_transactions: usize) -> (VerifiedBlock, Vec<BlockRef>) {
            let blocks = self.make_round_one_blocks(&[num_target_transactions]);
            let target = blocks[0].clone();
            let block_refs = blocks.iter().map(|block| block.reference()).collect();
            (target, block_refs)
        }

        fn make_voter(
            &self,
            author: u32,
            round_one_refs: &[BlockRef],
            target_ref: BlockRef,
            includes_target: bool,
            rejects: Vec<TransactionIndex>,
            cutoff_round: u32,
            equivocation_marker: Option<u8>,
        ) -> VerifiedBlock {
            let own_authority = AuthorityIndex::new_for_test(author);
            let mut ancestors: Vec<_> = round_one_refs
                .iter()
                .copied()
                .filter(|block_ref| includes_target || *block_ref != target_ref)
                .collect();
            ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
            let transaction_votes = (!rejects.is_empty()).then_some(BlockTransactionVotes {
                block_ref: target_ref,
                rejects,
            });
            let transactions = equivocation_marker
                .map(|marker| vec![Transaction::new(vec![marker])])
                .unwrap_or_default();
            VerifiedBlock::new_for_test(
                TestBlock::new(2, author)
                    .set_ancestors(ancestors)
                    .set_transactions(transactions)
                    .set_transaction_votes(transaction_votes.into_iter().collect())
                    .build_v3(cutoff_round),
            )
        }

        fn make_anchor(&self, ancestors: Vec<BlockRef>) -> VerifiedBlock {
            VerifiedBlock::new_for_test(TestBlock::new(3, 0).set_ancestors(ancestors).build_v3(0))
        }

        fn make_graph_block(
            &self,
            round: Round,
            author: u32,
            mut ancestors: Vec<BlockRef>,
            transaction_votes: Vec<BlockTransactionVotes>,
            cutoff_round: Round,
        ) -> VerifiedBlock {
            let own_authority = AuthorityIndex::new_for_test(author);
            ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
            VerifiedBlock::new_for_test(
                TestBlock::new(round, author)
                    .set_ancestors(ancestors)
                    .set_transaction_votes(transaction_votes)
                    .build_v3(cutoff_round),
            )
        }

        fn add_blocks(&self, blocks: &[VerifiedBlock]) {
            self.dag_state.write().accept_blocks(blocks.to_vec());
            self.transaction_vote_tracker.add_voted_blocks(
                blocks
                    .iter()
                    .cloned()
                    .map(|block| (block, vec![]))
                    .collect(),
            );
        }
    }

    fn make_commit(
        index: CommitIndex,
        leader: &VerifiedBlock,
        blocks: Vec<VerifiedBlock>,
    ) -> CommittedSubDag {
        CommittedSubDag::new(
            leader.reference(),
            blocks,
            0,
            CommitRef::new(index, CommitDigest::default()),
        )
    }

    #[tokio::test]
    async fn direct_accepts_and_rejects_with_next_round_quorums() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(2);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![1],
                    0,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        assert_eq!(
            fixture
                .transaction_vote_tracker
                .get_reject_votes(&target.reference()),
            Some(vec![(1, 5)])
        );

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert_eq!(finalized.len(), 1);
        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![1])
        );
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn direct_keeps_transactions_pending_at_the_voter_cutoff() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(2);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    1,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert!(finalized.is_empty());
        assert_eq!(
            fixture.finalizer.pending_commits[0]
                .pending_transactions
                .get(&target.reference()),
            Some(&BTreeSet::from([0, 1]))
        );
    }

    #[tokio::test]
    async fn direct_keeps_transactions_pending_when_next_round_blocks_do_not_link() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (1..6)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    false,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert!(finalized.is_empty());
        assert_eq!(
            fixture.finalizer.pending_commits[0]
                .pending_transactions
                .get(&target.reference()),
            Some(&BTreeSet::from([0]))
        );
    }

    #[tokio::test]
    async fn direct_uses_mixed_round_votes_for_synced_commit() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);

        let round_two_accept = fixture.make_voter(
            0,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let round_two_non_voters: Vec<_> = (1..6)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    false,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        let round_two_non_voter_refs: Vec<_> = round_two_non_voters
            .iter()
            .map(|block| block.reference())
            .collect();
        fixture.add_blocks(std::slice::from_ref(&round_two_accept));
        fixture.add_blocks(&round_two_non_voters);

        // These round-three blocks first observe the target through a weak link. Their
        // round-two parents do not include the target in their causal history.
        let round_three_accepts: Vec<_> = (1..5)
            .map(|author| {
                let mut ancestors = round_two_non_voter_refs.clone();
                ancestors.push(target.reference());
                fixture.make_graph_block(3, author, ancestors, vec![], 0)
            })
            .collect();
        fixture.add_blocks(&round_three_accepts);

        let mut commit = make_commit(
            1,
            &round_two_accept,
            vec![target.clone(), round_two_accept.clone()],
        );
        commit.decided_with_local_blocks = false;
        let finalized = fixture.finalizer.process_commit(commit);

        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
    }

    #[tokio::test]
    async fn direct_retries_when_new_local_votes_arrive() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let initial_accepts: Vec<_> = (0..4)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&initial_accepts);

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );

        let fifth_accept = fixture.make_voter(
            4,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let later_leader = fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        fixture.add_blocks(&[fifth_accept, later_leader.clone()]);

        // The second commit contains no accept certificate for the target. Only the retried direct
        // rule can use the new local vote and reach quorum.
        let finalized = fixture.finalizer.process_commit(make_commit(
            2,
            &later_leader,
            vec![later_leader.clone()],
        ));

        assert_eq!(finalized.len(), 2);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn direct_ignores_first_votes_after_leader_round_plus_one() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);

        let round_two_accept = fixture.make_voter(
            0,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let round_two_non_voters: Vec<_> = (1..6)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    false,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        let round_two_non_voter_refs: Vec<_> = round_two_non_voters
            .iter()
            .map(|block| block.reference())
            .collect();
        fixture.add_blocks(std::slice::from_ref(&round_two_accept));
        fixture.add_blocks(&round_two_non_voters);

        let round_three_accepts: Vec<_> = (1..4)
            .map(|author| {
                let mut ancestors = round_two_non_voter_refs.clone();
                ancestors.push(target.reference());
                fixture.make_graph_block(3, author, ancestors, vec![], 0)
            })
            .collect();
        fixture.add_blocks(&round_three_accepts);
        let round_four_accept = fixture.make_graph_block(
            4,
            4,
            vec![round_two_non_voters[3].reference(), target.reference()],
            vec![],
            0,
        );
        fixture.add_blocks(std::slice::from_ref(&round_four_accept));

        let finalized = fixture.finalizer.process_commit(make_commit(
            1,
            &round_two_accept,
            vec![target.clone(), round_two_accept.clone()],
        ));

        assert!(finalized.is_empty());
        assert_eq!(
            fixture.finalizer.pending_commits[0]
                .pending_transactions
                .get(&target.reference()),
            Some(&BTreeSet::from([0]))
        );
    }

    #[tokio::test]
    async fn direct_counts_only_the_first_vote_on_an_authority_chain() {
        for (rejects, cutoff_round) in [(vec![], 1), (vec![0], 0)] {
            let mut fixture = Fixture::new();
            let (target, round_one_refs) = fixture.make_round_one(1);
            let accept_voters: Vec<_> = (0..4)
                .map(|author| {
                    fixture.make_voter(
                        author,
                        &round_one_refs,
                        target.reference(),
                        true,
                        vec![],
                        0,
                        None,
                    )
                })
                .collect();
            let first_vote = fixture.make_voter(
                4,
                &round_one_refs,
                target.reference(),
                true,
                rejects,
                cutoff_round,
                None,
            );
            fixture.add_blocks(&accept_voters);
            fixture.add_blocks(std::slice::from_ref(&first_vote));

            // The later block would accept the transaction if it could vote again. Its earlier
            // block has already consumed this authority chain's vote with a cutoff or a reject.
            let later_origin_block =
                fixture.make_graph_block(3, 4, vec![first_vote.reference()], vec![], 0);
            fixture.add_blocks(std::slice::from_ref(&later_origin_block));

            let finalized = fixture.finalizer.process_commit(make_commit(
                1,
                &accept_voters[0],
                vec![target.clone(), accept_voters[0].clone()],
            ));

            assert!(finalized.is_empty());
            assert_eq!(
                fixture.finalizer.pending_commits[0]
                    .pending_transactions
                    .get(&target.reference()),
                Some(&BTreeSet::from([0]))
            );
        }
    }

    #[tokio::test]
    async fn direct_does_not_double_count_an_equivocating_accept_voter() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters: Vec<_> = (0..4)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        voters.push(fixture.make_voter(
            1,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            Some(9),
        ));
        fixture.add_blocks(&voters);

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert!(finalized.is_empty());
    }

    #[tokio::test]
    async fn direct_accepts_when_explicit_reject_votes_are_below_quorum() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        voters.push(fixture.make_voter(
            4,
            &round_one_refs,
            target.reference(),
            true,
            vec![0],
            0,
            Some(4),
        ));
        voters.push(fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            true,
            vec![0],
            0,
            None,
        ));
        fixture.add_blocks(&voters);
        assert_eq!(
            fixture
                .transaction_vote_tracker
                .get_reject_votes(&target.reference()),
            Some(vec![(0, 2)])
        );

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert_eq!(finalized.len(), 1);
        assert!(
            !finalized[0]
                .rejected_transactions_by_block
                .contains_key(&target.reference())
        );
    }

    #[tokio::test]
    #[should_panic(expected = "cannot have both accept and reject quorums")]
    async fn direct_detects_more_than_f_equivocating_stake() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        voters.extend((1..6).map(|author| {
            fixture.make_voter(
                author,
                &round_one_refs,
                target.reference(),
                true,
                vec![0],
                0,
                Some(author as u8),
            )
        }));
        fixture.add_blocks(&voters);
        assert_eq!(
            fixture
                .transaction_vote_tracker
                .get_reject_votes(&target.reference()),
            Some(vec![(0, 5)])
        );

        fixture
            .finalizer
            .process_commit(make_commit(1, &target, vec![target.clone()]));
    }

    #[tokio::test]
    async fn handle_selects_v3_and_persists_rejected_transactions() {
        let fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(2);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![1],
                    0,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);

        let (commit_sender, mut commit_receiver) = unbounded_channel("finalizer_v3_output_test");
        let mut handle = CommitFinalizerHandle::start(
            fixture.context.clone(),
            fixture.dag_state.clone(),
            fixture.transaction_vote_tracker.clone(),
            commit_sender,
        );
        let commit = make_commit(1, &target, vec![target.clone()]);

        handle.send(commit.clone()).unwrap();
        let finalized = tokio::time::timeout(Duration::from_secs(1), commit_receiver.recv())
            .await
            .expect("The v3 finalizer must produce an output")
            .expect("The output channel must stay open");
        handle.stop().await;

        assert_eq!(
            finalized
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![1])
        );
        assert_eq!(
            fixture.store.read_last_finalized_commit().unwrap(),
            Some(commit.commit_ref)
        );
        assert_eq!(
            fixture
                .store
                .read_rejected_transactions(commit.commit_ref)
                .unwrap()
                .unwrap()
                .get(&target.reference()),
            Some(&vec![1])
        );
    }

    #[tokio::test]
    async fn handle_preserves_recovered_finalization_without_recomputing() {
        let fixture = Fixture::new();
        let (target, _) = fixture.make_round_one(1);
        let transaction_vote_tracker = TransactionVoteTracker::new(
            fixture.context.clone(),
            Arc::new(NoopBlockVerifier),
            fixture.dag_state.clone(),
        );
        let (commit_sender, mut commit_receiver) =
            unbounded_channel("finalizer_v3_recovery_output_test");
        let mut handle = CommitFinalizerHandle::start(
            fixture.context.clone(),
            fixture.dag_state.clone(),
            transaction_vote_tracker,
            commit_sender,
        );
        let mut commit = make_commit(1, &target, vec![target.clone()]);
        commit.recovered_rejected_transactions = true;
        commit
            .rejected_transactions_by_block
            .insert(target.reference(), vec![0]);

        handle.send(commit.clone()).unwrap();
        let finalized = tokio::time::timeout(Duration::from_secs(1), commit_receiver.recv())
            .await
            .expect("The v3 finalizer must produce the recovered output")
            .expect("The output channel must stay open");
        handle.stop().await;

        assert!(finalized.recovered_rejected_transactions);
        assert_eq!(
            finalized
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
    }

    #[tokio::test]
    async fn direct_and_indirect_finalize_different_blocks_in_one_commit() {
        let mut fixture = Fixture::new();
        let round_one_blocks = fixture.make_round_one_blocks(&[1, 1]);
        let round_one_refs: Vec<_> = round_one_blocks
            .iter()
            .map(|block| block.reference())
            .collect();
        let target_a = round_one_blocks[0].clone();
        let target_b = round_one_blocks[1].clone();
        let voters: Vec<_> = (0..COMMITTEE_SIZE as u32)
            .map(|author| {
                let own_authority = AuthorityIndex::new_for_test(author);
                let mut ancestors = round_one_refs.clone();
                ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
                let mut transaction_votes = vec![];
                if author == 5 {
                    transaction_votes.push(BlockTransactionVotes {
                        block_ref: target_a.reference(),
                        rejects: vec![0],
                    });
                }
                if author >= 3 {
                    transaction_votes.push(BlockTransactionVotes {
                        block_ref: target_b.reference(),
                        rejects: vec![0],
                    });
                }
                VerifiedBlock::new_for_test(
                    TestBlock::new(2, author)
                        .set_ancestors(ancestors)
                        .set_transaction_votes(transaction_votes)
                        .build_v3(0),
                )
            })
            .collect();
        fixture.add_blocks(&voters);

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(
                    1,
                    &target_a,
                    vec![target_a.clone(), target_b.clone()],
                ))
                .is_empty()
        );
        let pending = &fixture.finalizer.pending_commits[0].pending_transactions;
        assert!(!pending.contains_key(&target_a.reference()));
        assert_eq!(
            pending.get(&target_b.reference()),
            Some(&BTreeSet::from([0]))
        );

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(2, &voters[0], voters.clone()));

        assert_eq!(finalized.len(), 2);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn indirect_accepts_certificate_without_full_voting_quorum_in_prefix() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let accept_voters: Vec<_> = (0..3)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        let reject_voters: Vec<_> = (3..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    false,
                    vec![],
                    0,
                    None,
                )
            })
            .collect();
        let voters: Vec<_> = accept_voters
            .iter()
            .chain(&reject_voters)
            .cloned()
            .collect();
        let anchor = fixture.make_anchor(
            voters
                .iter()
                .map(|voting_block| voting_block.reference())
                .collect(),
        );
        fixture.add_blocks(&voters);
        fixture.add_blocks(std::slice::from_ref(&anchor));

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        // The reject voters can belong to earlier finalized commits. Only the accept
        // certificate must appear after the target block.
        let mut anchor_commit_blocks = accept_voters;
        anchor_commit_blocks.push(anchor.clone());
        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(2, &anchor, anchor_commit_blocks));

        assert_eq!(finalized.len(), 2);
        assert!(
            !finalized[0]
                .rejected_transactions_by_block
                .contains_key(&target.reference())
        );
    }

    #[tokio::test]
    async fn indirect_accepts_mixed_round_first_votes_after_one_later_commit() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let round_two_vote = fixture.make_voter(
            0,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let round_three_vote = fixture.make_graph_block(
            3,
            1,
            vec![round_one_refs[1], round_two_vote.reference()],
            vec![],
            0,
        );
        let round_four_vote = fixture.make_graph_block(
            4,
            2,
            vec![round_one_refs[2], round_three_vote.reference()],
            vec![],
            0,
        );
        fixture.add_blocks(std::slice::from_ref(&round_two_vote));
        fixture.add_blocks(std::slice::from_ref(&round_three_vote));
        fixture.add_blocks(std::slice::from_ref(&round_four_vote));

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(
                    1,
                    &round_three_vote,
                    vec![
                        target.clone(),
                        round_two_vote.clone(),
                        round_three_vote.clone(),
                    ],
                ))
                .is_empty()
        );
        let finalized = fixture.finalizer.process_commit(make_commit(
            2,
            &round_four_vote,
            vec![round_four_vote.clone()],
        ));

        assert_eq!(finalized.len(), 2);
        assert!(
            !finalized[0]
                .rejected_transactions_by_block
                .contains_key(&target.reference())
        );
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn indirect_accepts_at_depth_two_with_one_equivocating_voter() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let accept_0 = fixture.make_voter(
            0,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let accept_1 = fixture.make_voter(
            1,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let accept_2 = fixture.make_voter(
            2,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let reject_2 = fixture.make_voter(
            2,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            Some(2),
        );
        let reject_3 = fixture.make_voter(
            3,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        let reject_4 = fixture.make_voter(
            4,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        fixture.add_blocks(&[
            accept_0.clone(),
            accept_1.clone(),
            accept_2.clone(),
            reject_2,
            reject_3.clone(),
            reject_4.clone(),
        ]);
        let anchor = fixture.make_anchor(vec![
            accept_0.reference(),
            accept_1.reference(),
            accept_2.reference(),
            reject_3.reference(),
            reject_4.reference(),
        ]);
        fixture.add_blocks(std::slice::from_ref(&anchor));

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(2, &accept_0, vec![accept_0.clone()]))
                .is_empty(),
            "Depth one must not make an indirect decision"
        );
        let finalized = fixture.finalizer.process_commit(make_commit(
            3,
            &anchor,
            vec![accept_1, accept_2, reject_3, reject_4, anchor.clone()],
        ));

        assert_eq!(finalized.len(), 3);
        assert!(
            !finalized[0]
                .rejected_transactions_by_block
                .contains_key(&target.reference())
        );
    }

    #[tokio::test]
    async fn indirect_uses_votes_from_all_committed_leaders() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let accept_0 = fixture.make_voter(
            0,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let accept_1 = fixture.make_voter(
            1,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let accept_2 = fixture.make_voter(
            2,
            &round_one_refs,
            target.reference(),
            true,
            vec![],
            0,
            None,
        );
        let reject_2 = fixture.make_voter(
            2,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        let reject_3 = fixture.make_voter(
            3,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        let reject_4 = fixture.make_voter(
            4,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        let reject_5 = fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        fixture.add_blocks(&[
            accept_0.clone(),
            accept_1.clone(),
            accept_2.clone(),
            reject_2.clone(),
            reject_3.clone(),
            reject_4.clone(),
            reject_5.clone(),
        ]);

        let named_leader = fixture.make_anchor(vec![
            accept_0.reference(),
            reject_2.reference(),
            reject_3.reference(),
            reject_4.reference(),
            reject_5.reference(),
        ]);
        let other_leader = VerifiedBlock::new_for_test(
            TestBlock::new(3, 1)
                .set_ancestors(vec![
                    accept_1.reference(),
                    accept_0.reference(),
                    accept_2.reference(),
                    reject_3.reference(),
                    reject_4.reference(),
                ])
                .build_v3(0),
        );
        fixture.add_blocks(&[named_leader.clone(), other_leader.clone()]);

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        let finalized = fixture.finalizer.process_commit(make_commit(
            2,
            &named_leader,
            vec![
                accept_0,
                accept_1,
                accept_2,
                reject_2,
                reject_3,
                reject_4,
                reject_5,
                other_leader,
                named_leader.clone(),
            ],
        ));

        assert_eq!(finalized.len(), 2);
        assert!(
            !finalized[0]
                .rejected_transactions_by_block
                .contains_key(&target.reference())
        );
    }

    #[tokio::test]
    async fn indirect_rejects_without_an_accept_certificate() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters = vec![];
        for author in 0..2 {
            voters.push(fixture.make_voter(
                author,
                &round_one_refs,
                target.reference(),
                true,
                vec![],
                0,
                None,
            ));
        }
        for author in 2..6 {
            voters.push(fixture.make_voter(
                author,
                &round_one_refs,
                target.reference(),
                false,
                vec![],
                0,
                None,
            ));
        }
        fixture.add_blocks(&voters);
        let anchor = fixture.make_anchor(
            voters
                .iter()
                .take(5)
                .map(|block| block.reference())
                .collect(),
        );
        fixture.add_blocks(std::slice::from_ref(&anchor));

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        let mut anchor_commit_blocks = voters.iter().take(5).cloned().collect::<Vec<_>>();
        anchor_commit_blocks.push(anchor.clone());
        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(2, &anchor, anchor_commit_blocks));

        assert_eq!(finalized.len(), 2);
        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
    }
}

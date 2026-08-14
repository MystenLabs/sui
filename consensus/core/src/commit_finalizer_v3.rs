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
    stake_aggregator::{CertificationThreshold, QuorumThreshold, StakeAggregator},
    transaction_vote_tracker::TransactionVoteTracker,
};

/// Finalizes transaction votes with the Mysticeti v3 one-round voting rule.
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
        // The indirect proof depends on FlexCommitter::build_commit reading the GC
        // round of the previous commit before Core::post_commit records the new
        // commit. FlexCommitter::handle_certified_commit must preserve the same
        // committed sub-DAG evidence for synchronized commits.
        let committee = &context.committee;
        let total_stake = u128::from(committee.total_stake());
        let quorum_threshold = u128::from(committee.quorum_threshold());
        let certification_threshold = u128::from(committee.certification_threshold());
        let malicious_stake = u128::from(
            committee
                .validity_threshold()
                .checked_sub(1)
                .expect("The validity threshold must be positive"),
        );
        assert!(
            quorum_threshold + certification_threshold > total_stake + malicious_stake,
            "Mysticeti v3 requires quorum plus certification stake to exceed total plus malicious stake"
        );
        assert!(
            2 * quorum_threshold >= total_stake + malicious_stake + certification_threshold,
            "Mysticeti v3 requires two quorums to preserve a certification after equivocation"
        );
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
        let (sender, receiver) = unbounded_channel("consensus_commit_finalizer_v3");
        let task =
            spawn_logged_monitored_task!(processor.run(receiver), "consensus_commit_finalizer_v3");
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
        let _scope = monitored_scope("CommitFinalizerV3::process_commit");

        if let Some(last_processed_commit) = self.last_processed_commit {
            assert_eq!(
                last_processed_commit + 1,
                committed_sub_dag.commit_ref.index
            );
        }
        self.last_processed_commit = Some(committed_sub_dag.commit_ref.index);
        self.pending_commits
            .push_back(CommitStateV3::new(committed_sub_dag));

        let voting_blocks_by_round = self.prepare_direct_voting_blocks();
        for index in 0..self.pending_commits.len() {
            self.try_direct_finalize_commit(index, &voting_blocks_by_round);
        }

        let mut finalized_commits = self.pop_finalized_commits();
        self.context
            .metrics
            .node_metrics
            .finalizer_output_commits
            .with_label_values(&["direct"])
            .inc_by(finalized_commits.len() as u64);

        while self.pending_commits.len() > 1 {
            let first_leader_round = self.pending_commits.front().unwrap().commit.leader.round;
            let anchor_round = self.pending_commits.back().unwrap().commit.leader.round;
            if first_leader_round.saturating_add(INDIRECT_COMMIT_DEPTH) > anchor_round {
                break;
            }

            self.try_indirect_finalize_first_commit();
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

        self.report_finalization_latency(&finalized_commits);
        self.context
            .metrics
            .node_metrics
            .finalizer_buffered_commits
            .set(self.pending_commits.len() as i64);

        finalized_commits
    }

    fn prepare_direct_voting_blocks(&self) -> BTreeMap<Round, Vec<PreparedVotingBlock>> {
        let voting_rounds: BTreeSet<_> = self
            .pending_commits
            .iter()
            .flat_map(|state| state.pending_transactions.keys())
            .map(|block_ref| block_ref.round.saturating_add(1))
            .collect();
        let dag_state = self.dag_state.read();
        voting_rounds
            .into_iter()
            .map(|round| {
                let voting_blocks = dag_state
                    .get_cached_blocks_at_round(round)
                    .into_iter()
                    .map(PreparedVotingBlock::new)
                    .collect();
                (round, voting_blocks)
            })
            .collect()
    }

    fn try_direct_finalize_commit(
        &mut self,
        commit_index: usize,
        voting_blocks_by_round: &BTreeMap<Round, Vec<PreparedVotingBlock>>,
    ) {
        let pending_transactions = self.pending_commits[commit_index]
            .pending_transactions
            .clone();
        for (block_ref, transaction_indices) in pending_transactions {
            let voting_blocks = voting_blocks_by_round
                .get(&block_ref.round.saturating_add(1))
                .expect("The voting round was collected from the pending transactions");
            let decisions =
                self.compute_direct_decisions(block_ref, &transaction_indices, voting_blocks);
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
        voting_blocks: &[PreparedVotingBlock],
    ) -> TransactionDecisions {
        let mut base_accept_votes = StakeAggregator::<QuorumThreshold>::new();
        let mut base_reject_votes = StakeAggregator::<QuorumThreshold>::new();
        let mut explicitly_rejected_transactions = BTreeSet::new();
        for voting_block in voting_blocks {
            assert_eq!(
                voting_block.reference.round,
                block_ref.round.saturating_add(1),
                "Voting block {} is not in the next round of {}",
                voting_block.reference,
                block_ref,
            );
            if voting_block.can_accept(block_ref) {
                base_accept_votes
                    .add_unique(voting_block.reference.author, &self.context.committee);
                if let Some(explicit_rejects) = voting_block.explicit_rejects.get(&block_ref) {
                    explicitly_rejected_transactions
                        .extend(explicit_rejects.intersection(transaction_indices).copied());
                }
            } else {
                base_reject_votes
                    .add_unique(voting_block.reference.author, &self.context.committee);
            }
        }

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_tallies = explicitly_rejected_transactions
                .contains(transaction_index)
                .then(|| {
                    let mut accept_votes = StakeAggregator::<QuorumThreshold>::new();
                    let mut reject_votes = StakeAggregator::<QuorumThreshold>::new();
                    for voting_block in voting_blocks {
                        // An equivocating authority can count once on each side. It cannot
                        // count twice on one side.
                        let votes =
                            if voting_block.accepts_transaction(block_ref, *transaction_index) {
                                &mut accept_votes
                            } else {
                                &mut reject_votes
                            };
                        votes.add_unique(voting_block.reference.author, &self.context.committee);
                    }
                    (accept_votes, reject_votes)
                });
            let (accept_votes, reject_votes) = match &transaction_tallies {
                Some((accept_votes, reject_votes)) => (accept_votes, reject_votes),
                None => (&base_accept_votes, &base_reject_votes),
            };
            let accepted = accept_votes.reached_threshold(&self.context.committee);
            let rejected = reject_votes.reached_threshold(&self.context.committee);
            assert!(
                !(accepted && rejected),
                "Transaction {} in block {} cannot have both accept and reject quorums. Accept voters: {:?}, reject voters: {:?}",
                transaction_index,
                block_ref,
                accept_votes.authorities(),
                reject_votes.authorities(),
            );
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if rejected {
                decisions.rejected.push(*transaction_index);
            }
        }
        decisions
    }

    fn try_indirect_finalize_first_commit(&mut self) {
        let pending_transactions = self.pending_commits[0].pending_transactions.clone();
        let voting_rounds: BTreeSet<_> = pending_transactions
            .keys()
            .map(|block_ref| block_ref.round.saturating_add(1))
            .collect();
        let mut committed_voting_blocks_by_round: BTreeMap<Round, Vec<PreparedVotingBlock>> =
            voting_rounds
                .into_iter()
                .map(|round| (round, vec![]))
                .collect();
        for block in self
            .pending_commits
            .iter()
            .flat_map(|state| &state.commit.blocks)
        {
            if let Some(voting_blocks) = committed_voting_blocks_by_round.get_mut(&block.round()) {
                voting_blocks.push(PreparedVotingBlock::new(block.clone()));
            }
        }

        for (block_ref, transaction_indices) in pending_transactions {
            let voting_round = block_ref.round.saturating_add(1);
            // An accept voter references the target block. Thus, it cannot commit before the
            // target block. The pending commit prefix contains every committed accept voter.
            let committed_voting_blocks = committed_voting_blocks_by_round
                .get(&voting_round)
                .expect("The voting round was collected from the pending transactions");
            let decisions = self.compute_indirect_decisions(
                block_ref,
                &transaction_indices,
                committed_voting_blocks,
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
        committed_voting_blocks: &[PreparedVotingBlock],
    ) -> TransactionDecisions {
        let mut base_accept_votes = StakeAggregator::<CertificationThreshold>::new();
        let mut explicitly_rejected_transactions = BTreeSet::new();
        for voting_block in committed_voting_blocks {
            if voting_block.can_accept(block_ref) {
                base_accept_votes
                    .add_unique(voting_block.reference.author, &self.context.committee);
                if let Some(explicit_rejects) = voting_block.explicit_rejects.get(&block_ref) {
                    explicitly_rejected_transactions
                        .extend(explicit_rejects.intersection(transaction_indices).copied());
                }
            }
        }

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = explicitly_rejected_transactions
                .contains(transaction_index)
                .then(|| {
                    let mut accept_votes = StakeAggregator::<CertificationThreshold>::new();
                    for voting_block in committed_voting_blocks {
                        if voting_block.accepts_transaction(block_ref, *transaction_index) {
                            accept_votes
                                .add_unique(voting_block.reference.author, &self.context.committee);
                        }
                    }
                    accept_votes
                });
            let accept_votes = transaction_accept_votes
                .as_ref()
                .unwrap_or(&base_accept_votes);
            // A depth-two anchor has a voting-round quorum in its causal history. The v3
            // quorum-intersection checks guarantee that a direct accept quorum leaves an accept
            // certificate in that history after f equivocations. Accept voters cannot commit
            // before the target, so this certificate must be in the buffered commit prefix.
            if accept_votes.reached_threshold(&self.context.committee) {
                decisions.accepted.push(*transaction_index);
            } else {
                decisions.rejected.push(*transaction_index);
            }
        }
        decisions
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

struct PreparedVotingBlock {
    reference: BlockRef,
    cutoff_round: Option<Round>,
    ancestors: BTreeSet<BlockRef>,
    explicit_rejects: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
}

impl PreparedVotingBlock {
    fn new(block: VerifiedBlock) -> Self {
        let mut explicit_rejects: BTreeMap<BlockRef, BTreeSet<TransactionIndex>> = BTreeMap::new();
        for votes in block.transaction_votes() {
            explicit_rejects
                .entry(votes.block_ref)
                .or_default()
                .extend(votes.rejects.iter().copied());
        }
        Self {
            reference: block.reference(),
            cutoff_round: block.transaction_votes_cutoff_round_v3(),
            ancestors: block.ancestors().iter().copied().collect(),
            explicit_rejects,
        }
    }

    fn can_accept(&self, block_ref: BlockRef) -> bool {
        self.cutoff_round
            .is_some_and(|cutoff_round| block_ref.round > cutoff_round)
            && self.ancestors.contains(&block_ref)
    }

    fn accepts_transaction(
        &self,
        block_ref: BlockRef,
        transaction_index: TransactionIndex,
    ) -> bool {
        self.can_accept(block_ref)
            && !self
                .explicit_rejects
                .get(&block_ref)
                .is_some_and(|rejects| rejects.contains(&transaction_index))
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
                transaction_vote_tracker,
                commit_sender,
            );

            Self {
                context,
                dag_state,
                store,
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
            self.dag_state.write().accept_blocks(blocks.clone());
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

        fn add_blocks(&self, blocks: &[VerifiedBlock]) {
            self.dag_state.write().accept_blocks(blocks.to_vec());
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

    #[test]
    #[should_panic(
        expected = "Mysticeti v3 requires two quorums to preserve a certification after equivocation"
    )]
    fn finalizer_rejects_non_v3_committee_thresholds() {
        let (mut context, _) = Context::new_with_test_options(4, false);
        context.protocol_config.set_enable_v3_for_testing(true);
        let context = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let transaction_vote_tracker = TransactionVoteTracker::new(
            context.clone(),
            Arc::new(NoopBlockVerifier),
            dag_state.clone(),
        );
        let (commit_sender, _commit_receiver) = unbounded_channel("invalid_v3_committee_test");

        CommitFinalizerV3::new(context, dag_state, transaction_vote_tracker, commit_sender);
    }

    #[test]
    fn direct_accepts_and_rejects_with_next_round_quorums() {
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

    #[test]
    fn direct_rejects_transactions_at_the_voter_cutoff() {
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

        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0, 1])
        );
    }

    #[test]
    fn direct_rejects_transactions_in_an_omitted_block() {
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

        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
    }

    #[test]
    fn direct_treats_old_block_versions_as_reject_votes() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                let own_authority = AuthorityIndex::new_for_test(author);
                let mut ancestors = round_one_refs.clone();
                ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
                VerifiedBlock::new_for_test(
                    TestBlock::new(2, author).set_ancestors(ancestors).build(),
                )
            })
            .collect();
        fixture.add_blocks(&voters);

        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]));

        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
    }

    #[test]
    fn direct_does_not_double_count_an_equivocating_accept_voter() {
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

    #[test]
    fn direct_counts_an_equivocating_voter_once_on_each_side() {
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
            false,
            vec![],
            0,
            Some(4),
        ));
        voters.push(fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        ));
        fixture.add_blocks(&voters);

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

    #[test]
    #[should_panic(expected = "cannot have both accept and reject quorums")]
    fn direct_detects_more_than_f_equivocating_stake() {
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
                false,
                vec![],
                0,
                Some(author as u8),
            )
        }));
        fixture.add_blocks(&voters);

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

        let transaction_vote_tracker = TransactionVoteTracker::new(
            fixture.context.clone(),
            Arc::new(NoopBlockVerifier),
            fixture.dag_state.clone(),
        );
        let (commit_sender, mut commit_receiver) = unbounded_channel("finalizer_v3_output_test");
        let mut handle = CommitFinalizerHandle::start(
            fixture.context.clone(),
            fixture.dag_state.clone(),
            transaction_vote_tracker,
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

    #[test]
    fn direct_and_indirect_finalize_different_blocks_in_one_commit() {
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

        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(2, &voters[0], voters.clone()))
                .is_empty()
        );
        let anchor = fixture.make_anchor(
            voters
                .iter()
                .map(|voting_block| voting_block.reference())
                .collect(),
        );
        fixture.add_blocks(std::slice::from_ref(&anchor));
        let finalized =
            fixture
                .finalizer
                .process_commit(make_commit(3, &anchor, vec![anchor.clone()]));

        assert_eq!(finalized.len(), 3);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
    }

    #[test]
    fn indirect_accepts_certificate_without_full_voting_quorum_in_prefix() {
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

    #[test]
    fn indirect_accepts_at_depth_two_with_one_equivocating_voter() {
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

    #[test]
    fn indirect_uses_votes_from_all_committed_leaders() {
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

    #[test]
    fn indirect_rejects_without_an_accept_certificate() {
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

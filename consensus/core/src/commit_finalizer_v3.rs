// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use consensus_config::{AuthorityIndex, Stake};
use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_metrics::{
    monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    monitored_scope, spawn_logged_monitored_task,
};
use parking_lot::RwLock;
use tokio::{
    sync::watch,
    time::{Instant, MissedTickBehavior, interval_at},
};

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

const STATUS_INTERVAL: Duration = Duration::from_secs(1);
const SLOW_FINALIZATION: Duration = Duration::from_millis(200);

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
///   L + 1. A first vote whose cutoff covers the target rejects all its transactions. These votes
///   combine with explicit reject votes from [`TransactionVoteTracker`] toward a reject quorum.
///   The finalizer retries this rule for pending commits when it receives a new commit or block
///   evidence. The local DAG only traverses targets above its current GC round, where descendant
///   votes remain available.
/// - Indirect finalization checks pending transactions when later commits enter the queue. It uses
///   the same voting window, but it uses only committed descendants. It accepts a transaction when
///   the target is above L - gc_depth and the accept stake reaches the certification threshold.
///   A quorum of committed first votes can reject transactions through cutoffs or explicit rejects.
///   The GC bound keeps first-vote evidence available through the depth-two decision. When a
///   committed anchor reaches the required depth, it rejects all other pending transactions.
///
/// The finalizer keeps a commit in its pending queue until every transaction in the commit has a decision.
pub(crate) struct CommitFinalizerV3 {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    transaction_vote_tracker: TransactionVoteTracker,
    commit_sender: UnboundedSender<CommittedSubDag>,

    last_processed_commit: Option<CommitIndex>,
    pending_commits: VecDeque<CommitStateV3>,
    last_attempt_at: Instant,
    last_trigger: &'static str,
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
        // The finalizer receives a continuous CommittedSubDag stream. The GC guard below keeps all
        // required accept-vote blocks in this stream until the depth-two decision.
        Self {
            context,
            dag_state,
            transaction_vote_tracker,
            commit_sender,
            last_processed_commit: None,
            pending_commits: VecDeque::new(),
            last_attempt_at: Instant::now(),
            last_trigger: "none",
        }
    }

    pub(crate) fn start(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        transaction_vote_tracker: TransactionVoteTracker,
        commit_sender: UnboundedSender<CommittedSubDag>,
    ) -> CommitFinalizerHandle {
        let (block_update_sender, block_updates) = watch::channel(());
        let processor = Self::new(context, dag_state, transaction_vote_tracker, commit_sender);
        let (sender, receiver) = unbounded_channel("consensus_commit_finalizer");
        let task = spawn_logged_monitored_task!(
            processor.run(receiver, block_updates),
            "consensus_commit_finalizer"
        );
        CommitFinalizerHandle::new(sender, Some(block_update_sender), task)
    }

    async fn run(
        mut self,
        mut receiver: UnboundedReceiver<CommittedSubDag>,
        mut block_updates: watch::Receiver<()>,
    ) {
        let mut status_interval = interval_at(Instant::now() + STATUS_INTERVAL, STATUS_INTERVAL);
        status_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let (committed_sub_dag, shutting_down) = tokio::select! {
                biased;
                commit = receiver.recv() => {
                    let shutting_down = commit.is_none();
                    (commit, shutting_down)
                }
                Ok(()) = block_updates.changed(), if !self.pending_commits.is_empty() => {
                    (None, false)
                }
                _ = status_interval.tick(), if !self.pending_commits.is_empty() => {
                    // Report saved evidence without retrying finalization or consuming block updates.
                    self.report_pending_status();
                    continue;
                }
            };
            // A commit also retries pending work, so consume all preceding block notifications
            // before either kind of pass. Updates arriving during the pass remain unseen and
            // trigger another pass, avoiding lost wakeups.
            block_updates.borrow_and_update();
            let (finalized_commits, already_finalized) = match committed_sub_dag {
                Some(commit)
                    if !self.context.protocol_config.transaction_voting_enabled()
                        || commit.recovered_rejected_transactions =>
                {
                    (vec![commit], true)
                }
                Some(commit) => (self.process_commit(commit), false),
                None if self.pending_commits.is_empty() => (vec![], false),
                None => (
                    self.try_finalize_commits(if shutting_down {
                        "shutdown"
                    } else {
                        "block_update"
                    }),
                    false,
                ),
            };
            let persist_started = Instant::now();
            persist_finalized_commits(
                &self.dag_state,
                &self.transaction_vote_tracker,
                &finalized_commits,
                !already_finalized,
            );
            if !finalized_commits.is_empty() {
                let elapsed = persist_started.elapsed();
                self.context
                    .metrics
                    .node_metrics
                    .finalizer_v3_phase_duration_seconds
                    .with_label_values(&["persist"])
                    .observe(elapsed.as_secs_f64());
                if elapsed >= SLOW_FINALIZATION {
                    tracing::info!(
                        first_commit = finalized_commits.first().unwrap().commit_ref.index,
                        last_commit = finalized_commits.last().unwrap().commit_ref.index,
                        persist_ms = elapsed.as_secs_f64() * 1000.0,
                        "V3 finalizer storage write was slow"
                    );
                }
            }
            for commit in finalized_commits {
                if let Err(error) = self.commit_sender.send(commit) {
                    tracing::warn!(
                        "Failed to send to commit handler, probably due to shutdown: {error:?}"
                    );
                    return;
                }
            }
            if shutting_down {
                return;
            }
        }
    }

    pub(crate) fn process_commit(
        &mut self,
        committed_sub_dag: CommittedSubDag,
    ) -> Vec<CommittedSubDag> {
        if let Some(last_processed_commit) = self.last_processed_commit {
            assert_eq!(
                last_processed_commit + 1,
                committed_sub_dag.commit_ref.index
            );
        }
        self.last_processed_commit = Some(committed_sub_dag.commit_ref.index);
        let commit_state = CommitStateV3::new(committed_sub_dag);
        self.report_gc_guarded_blocks(&commit_state);
        self.pending_commits.push_back(commit_state);

        self.try_finalize_commits("commit")
    }

    fn try_finalize_commits(&mut self, trigger: &'static str) -> Vec<CommittedSubDag> {
        self.last_attempt_at = Instant::now();
        self.last_trigger = trigger;
        self.context
            .metrics
            .node_metrics
            .finalizer_v3_attempts
            .with_label_values(&[trigger])
            .inc();
        let _scope = monitored_scope("CommitFinalizer::process_commit");
        let _timer = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["CommitFinalizer::try_finalize_commits"])
            .start_timer();

        // Direct finalization applies these steps to every pending block B in a commit whose
        // leader is at round L:
        //
        // 1. The validator traverses local descendants of B through round L + 1.
        // 2. The first block on each authority chain whose causal history includes B casts a vote.
        //    A cutoff that covers B rejects all its transactions; otherwise, explicit rejects
        //    apply per transaction and the rest are accepted. Later blocks on the same authority
        //    chain do not vote again. Each side counts an authority once.
        // 3. DagState only traverses B above its current GC round. All descendants are newer than
        //    B, so GC cannot hide an earlier first vote while leaving B traversable.
        // 4. It combines cutoff reject voters with explicit reject voters from the tracker,
        //    counting each authority only once even if it appears in both sources.
        // 5. It accepts the transaction when accept stake reaches quorum. It rejects the
        //    transaction when reject stake reaches quorum. Otherwise, the transaction stays
        //    pending.
        //
        // DagState keeps the direct child links for local blocks.
        let direct_timer = self
            .context
            .metrics
            .node_metrics
            .finalizer_v3_phase_duration_seconds
            .with_label_values(&["direct"])
            .start_timer();
        for index in 0..self.pending_commits.len() {
            self.try_direct_finalize_commit(index);
        }

        let mut finalized_commits = self.pop_finalized_commits("direct");
        drop(direct_timer);
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
        // finalization, with a fixed GC bound to keep earlier first votes from being pruned before
        // they commit. It accepts at the certification threshold and rejects at a quorum of
        // committed cutoff or explicit rejects. The direct pass above has already applied local
        // reject quorums.
        //
        // A commit must leave the queue when the newest leader round is
        // INDIRECT_COMMIT_DEPTH rounds above its leader. At that point, any direct accept quorum
        // must leave an accept certificate in the committed prefix. The validator rejects every
        // transaction that is still pending after the certificate check.
        let indirect_timer = self
            .context
            .metrics
            .node_metrics
            .finalizer_v3_phase_duration_seconds
            .with_label_values(&["indirect"])
            .start_timer();
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
                let indirect_finalized_commits = self.pop_finalized_commits("indirect");
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
        drop(indirect_timer);

        self.report_finalization_latency(&finalized_commits);
        self.context
            .metrics
            .node_metrics
            .finalizer_buffered_commits
            .set(self.pending_commits.len() as i64);
        self.report_pending_status();

        finalized_commits
    }

    fn try_direct_finalize_commit(&mut self, commit_index: usize) {
        self.pending_commits[commit_index].pending_direct_vote = None;
        self.pending_commits[commit_index].attempts += 1;
        let leader_round = self.pending_commits[commit_index].commit.leader.round;
        let last_voting_round = leader_round.saturating_add(1);
        let pending_transactions = self.pending_commits[commit_index]
            .pending_transactions
            .clone();
        for (block_ref, transaction_indices) in pending_transactions {
            // First votes come from the earliest blocks on each authority chain that include
            // block_ref in their causal history. Only rounds through leader_round + 1 are eligible;
            // DagState returns no children for targets at or below its current GC round.
            let (first_votes, gc_round) = {
                let dag_state = self.dag_state.read();
                (
                    collect_first_votes(&*dag_state, block_ref, last_voting_round),
                    dag_state.gc_round(),
                )
            };
            let mut decisions =
                self.compute_direct_decisions(block_ref, &transaction_indices, &first_votes);
            if self.pending_commits[commit_index]
                .pending_direct_vote
                .is_none()
            {
                self.pending_commits[commit_index].pending_direct_vote =
                    decisions.pending.take().map(|vote| (gc_round, vote));
            }
            self.apply_decisions(
                commit_index,
                block_ref,
                decisions,
                "direct_finalize",
                "direct_reject",
            );
        }
    }

    fn report_gc_guarded_blocks(&self, commit_state: &CommitStateV3) {
        let leader_round = commit_state.commit.leader.round;
        let vote_evidence_gc_round =
            leader_round.saturating_sub(self.context.protocol_config.gc_depth());
        for block_ref in commit_state.pending_transactions.keys() {
            if block_ref.round > vote_evidence_gc_round {
                continue;
            }
            let hostname = &self.context.committee.authority(block_ref.author).hostname;
            self.context
                .metrics
                .node_metrics
                .finalizer_skipped_voting_blocks
                .with_label_values(&[hostname, "direct"])
                .inc();
            tracing::debug!(
                "Block {block_ref} is at or below vote GC round {vote_evidence_gc_round}. The finalizer will not count its accept votes."
            );
        }
    }

    fn compute_direct_decisions(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        first_votes: &[VotingBlock],
    ) -> TransactionDecisions {
        let explicit_reject_votes = self
            .transaction_vote_tracker
            .get_reject_vote_aggregators(&block_ref)
            .unwrap_or_else(|| {
                panic!(
                    "No vote info found for {block_ref}. It is incorrectly GC'ed or failed to be recovered after crash."
                )
            });

        let accept_votes: TransactionVotes<QuorumThreshold> =
            self.collect_accept_votes(block_ref, transaction_indices, first_votes);
        let reject_votes = self.collect_reject_votes(
            block_ref,
            transaction_indices,
            first_votes,
            explicit_reject_votes,
        );

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = accept_votes.for_transaction(*transaction_index);
            let accepted = transaction_accept_votes.reached_threshold(&self.context.committee);
            let transaction_reject_votes = reject_votes.for_transaction(*transaction_index);
            let rejected = transaction_reject_votes.reached_threshold(&self.context.committee);
            assert!(
                !(accepted && rejected),
                "Transaction {} in block {} cannot have both accept and reject quorums. Accept voters: {:?}, reject voters: {:?}",
                transaction_index,
                block_ref,
                transaction_accept_votes.authorities(),
                transaction_reject_votes.authorities(),
            );
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if rejected {
                decisions.rejected.push(*transaction_index);
            } else if decisions.pending.is_none() {
                decisions.pending = Some(PendingVoteStatus::new(
                    block_ref,
                    *transaction_index,
                    transaction_accept_votes,
                    transaction_reject_votes,
                    first_votes,
                ));
            }
        }
        decisions
    }

    fn try_indirect_finalize_first_commit(
        &mut self,
        committed_voting_graph: &CommittedBlockGraph,
        reject_remaining: bool,
    ) {
        self.pending_commits[0].pending_indirect_vote = None;
        let pending_transactions = self.pending_commits[0].pending_transactions.clone();
        let leader_round = self.pending_commits[0].commit.leader.round;
        let last_voting_round = leader_round.saturating_add(1);
        // The first depth-two anchor's predecessor has leader round at most L + 1. Linearizing
        // that anchor can prune through L + 1 - gc_depth. Since first votes are strictly newer
        // than their targets, target > L - gc_depth keeps all first-vote evidence available.
        // A signed cutoff cannot replace this bound: a later block can have a low cutoff but
        // omit a reject already cast by an ancestor that GC would skip during linearization.
        let vote_evidence_gc_round =
            leader_round.saturating_sub(self.context.protocol_config.gc_depth());

        for (block_ref, transaction_indices) in pending_transactions {
            // An accept voter is a descendant of the target block. Thus, it cannot commit before
            // the target block. For an eligible target, GC cannot remove an earlier first vote.
            // Therefore, the first committed descendant from each authority is its true first vote.
            let first_votes = if block_ref.round > vote_evidence_gc_round {
                collect_first_votes(committed_voting_graph, block_ref, last_voting_round)
            } else {
                vec![]
            };
            let mut decisions = self.compute_indirect_decisions(
                block_ref,
                &transaction_indices,
                &first_votes,
                reject_remaining,
            );
            if self.pending_commits[0].pending_indirect_vote.is_none() {
                self.pending_commits[0].pending_indirect_vote = decisions.pending.take();
            }
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
        first_votes: &[VotingBlock],
        reject_remaining: bool,
    ) -> TransactionDecisions {
        let accept_votes: TransactionVotes<CertificationThreshold> =
            self.collect_accept_votes(block_ref, transaction_indices, first_votes);
        // Rejection still needs a full quorum: Q + C > N + f ensures it intersects any accept
        // certificate in honest stake. Crash faults cannot supply conflicting first votes.
        let reject_votes =
            self.collect_reject_votes(block_ref, transaction_indices, first_votes, BTreeMap::new());

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = accept_votes.for_transaction(*transaction_index);
            let accepted = transaction_accept_votes.reached_threshold(&self.context.committee);
            let transaction_reject_votes = reject_votes.for_transaction(*transaction_index);
            let rejected = transaction_reject_votes.reached_threshold(&self.context.committee);
            assert!(
                !(accepted && rejected),
                "Transaction {} in block {} cannot have both an accept certificate and a reject quorum. Accept voters: {:?}, reject voters: {:?}",
                transaction_index,
                block_ref,
                transaction_accept_votes.authorities(),
                transaction_reject_votes.authorities(),
            );
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if rejected || reject_remaining {
                // A reject quorum decides immediately. At depth two, even without a reject
                // quorum, no accept certificate means that rejection is safe: any direct accept
                // quorum must leave an accept certificate in the committed prefix.
                decisions.rejected.push(*transaction_index);
            } else if decisions.pending.is_none() {
                decisions.pending = Some(PendingVoteStatus::new(
                    block_ref,
                    *transaction_index,
                    transaction_accept_votes,
                    transaction_reject_votes,
                    first_votes,
                ));
            }
        }
        decisions
    }

    fn collect_accept_votes<T: CommitteeThreshold>(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        first_votes: &[VotingBlock],
    ) -> TransactionVotes<T> {
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

        TransactionVotes {
            shared,
            by_transaction,
        }
    }

    fn collect_reject_votes(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        first_votes: &[VotingBlock],
        mut by_transaction: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    ) -> TransactionVotes<QuorumThreshold> {
        by_transaction.retain(|index, _| transaction_indices.contains(index));
        // A cutoff rejects every transaction in the target, so these voters share one aggregator.
        let mut shared = StakeAggregator::new();
        for voting_block in first_votes {
            let author = voting_block.block_ref.author;
            if !voting_block.can_accept(block_ref) {
                shared.add_unique(author, &self.context.committee);
            } else if let Some(rejects) = voting_block.explicit_rejects.get(&block_ref) {
                for index in rejects.intersection(transaction_indices) {
                    by_transaction
                        .entry(*index)
                        .or_default()
                        .add_unique(author, &self.context.committee);
                }
            }
        }
        // An equivocating authority may contribute both a cutoff and an explicit reject.
        // Union the voters rather than adding their stake totals.
        for reject_votes in by_transaction.values_mut() {
            for author in shared.authorities() {
                reject_votes.add_unique(*author, &self.context.committee);
            }
        }
        TransactionVotes {
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

    fn pop_finalized_commits(&mut self, release_path: &'static str) -> Vec<CommittedSubDag> {
        let mut finalized_commits = vec![];
        while self
            .pending_commits
            .front()
            .is_some_and(|state| state.pending_transactions.is_empty())
        {
            let commit_state = self.pending_commits.pop_front().unwrap();
            let now = Instant::now();
            let ready_at = commit_state
                .ready_at
                .expect("All transactions have a decision");
            let decision_wait = ready_at.duration_since(commit_state.received_at);
            let ordered_release_wait = now.duration_since(ready_at);
            let total_wait = now.duration_since(commit_state.received_at);
            for (stage, wait) in [
                ("decision", decision_wait),
                ("ordered_release", ordered_release_wait),
                ("total", total_wait),
            ] {
                self.context
                    .metrics
                    .node_metrics
                    .finalizer_v3_commit_wait_seconds
                    .with_label_values(&[stage])
                    .observe(wait.as_secs_f64());
            }
            if total_wait >= SLOW_FINALIZATION {
                tracing::info!(
                    commit_index = commit_state.commit.commit_ref.index,
                    leader_round = commit_state.commit.leader.round,
                    decided_with_local_blocks = commit_state.commit.decided_with_local_blocks,
                    release_path,
                    trigger = self.last_trigger,
                    attempts = commit_state.attempts,
                    decision_wait_ms = decision_wait.as_secs_f64() * 1000.0,
                    ordered_release_wait_ms = ordered_release_wait.as_secs_f64() * 1000.0,
                    total_wait_ms = total_wait.as_secs_f64() * 1000.0,
                    "V3 finalizer released a slow commit"
                );
            }
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

    fn report_pending_status(&mut self) {
        let metrics = &self.context.metrics.node_metrics;
        let pending_transactions: usize = self
            .pending_commits
            .iter()
            .flat_map(|state| state.pending_transactions.values())
            .map(BTreeSet::len)
            .sum();
        let ready_commits = self
            .pending_commits
            .iter()
            .filter(|state| state.pending_transactions.is_empty())
            .count();
        metrics
            .finalizer_v3_pending_transactions
            .set(pending_transactions as i64);
        metrics.finalizer_v3_ready_commits.set(ready_commits as i64);
        let buffered_commits = self.pending_commits.len();
        let newest_leader_round = self
            .pending_commits
            .back()
            .map(|state| state.commit.leader.round)
            .unwrap_or_default();
        let Some(first) = self.pending_commits.front_mut() else {
            metrics.finalizer_v3_oldest_pending_seconds.set(0.0);
            metrics.finalizer_v3_anchor_round_gap.set(0);
            return;
        };
        let now = Instant::now();
        let age = now.duration_since(first.received_at);
        let anchor_round_gap = first
            .commit
            .leader
            .round
            .saturating_add(INDIRECT_COMMIT_DEPTH)
            .saturating_sub(newest_leader_round);
        metrics
            .finalizer_v3_oldest_pending_seconds
            .set(age.as_secs_f64());
        metrics
            .finalizer_v3_anchor_round_gap
            .set(i64::from(anchor_round_gap));
        if first
            .last_status_log_at
            .is_some_and(|last| now.duration_since(last) < STATUS_INTERVAL)
        {
            return;
        }
        first.last_status_log_at = Some(now);
        let (local_gc_round, highest_accepted_round) = {
            let dag_state = self.dag_state.read();
            (dag_state.gc_round(), dag_state.highest_accepted_round())
        };
        tracing::info!(
            commit_index = first.commit.commit_ref.index,
            leader_round = first.commit.leader.round,
            decided_with_local_blocks = first.commit.decided_with_local_blocks,
            age_ms = age.as_secs_f64() * 1000.0,
            attempts = first.attempts,
            trigger = self.last_trigger,
            since_last_attempt_ms = now.duration_since(self.last_attempt_at).as_secs_f64() * 1000.0,
            buffered_commits,
            ready_commits,
            pending_transactions,
            first_pending_blocks = first.pending_transactions.len(),
            newest_leader_round,
            anchor_round_gap,
            local_gc_round,
            highest_accepted_round,
            last_voting_round = first.commit.leader.round.saturating_add(1),
            "V3 finalizer is waiting for transaction decisions"
        );
        if let Some((gc_round, vote)) = &first.pending_direct_vote
            && first
                .pending_transactions
                .get(&vote.block_ref)
                .is_some_and(|pending| pending.contains(&vote.transaction_index))
        {
            vote.report(
                first.commit.commit_ref.index,
                "direct",
                *gc_round,
                self.context.committee.quorum_threshold(),
                self.context.committee.quorum_threshold(),
            );
        }
        if let Some(vote) = &first.pending_indirect_vote {
            vote.report(
                first.commit.commit_ref.index,
                "indirect",
                first
                    .commit
                    .leader
                    .round
                    .saturating_sub(self.context.protocol_config.gc_depth()),
                self.context.committee.certification_threshold(),
                self.context.committee.quorum_threshold(),
            );
        }
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
    let mut to_visit: BTreeSet<_> = graph
        .children(&block_ref)
        .into_iter()
        .filter(|child| child.round <= last_voting_round)
        .collect();
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
        // Children are in later rounds. Reading them at the boundary adds no eligible votes.
        if current_ref.round == last_voting_round {
            continue;
        }
        to_visit.extend(
            graph
                .children(&current_ref)
                .into_iter()
                .filter(|child| child.round <= last_voting_round && !visited.contains(child)),
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
    if block_ref.round >= last_voting_round {
        return;
    }
    let mut to_visit: BTreeSet<_> = graph
        .children(&block_ref)
        .into_iter()
        .filter(|child| child.round <= last_voting_round && child.author == block_ref.author)
        .collect();
    let mut visited = BTreeSet::new();
    while let Some(current_ref) = to_visit.pop_first() {
        if current_ref.round > last_voting_round || !visited.insert(current_ref) {
            continue;
        }
        ignored.insert(current_ref);
        if current_ref.round == last_voting_round {
            continue;
        }
        to_visit.extend(graph.children(&current_ref).into_iter().filter(|child| {
            child.round <= last_voting_round
                && child.author == block_ref.author
                && !visited.contains(child)
        }));
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

struct TransactionVotes<T> {
    shared: StakeAggregator<T>,
    by_transaction: BTreeMap<TransactionIndex, StakeAggregator<T>>,
}

impl<T> TransactionVotes<T> {
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
    pending: Option<PendingVoteStatus>,
}

// Keep one pending transaction per vote check. The status timer must not traverse the DAG again.
struct PendingVoteStatus {
    block_ref: BlockRef,
    transaction_index: TransactionIndex,
    accept_stake: Stake,
    reject_stake: Stake,
    accept_voters: Vec<AuthorityIndex>,
    reject_voters: Vec<AuthorityIndex>,
    first_votes: Vec<(BlockRef, Round)>,
}

impl PendingVoteStatus {
    fn new<T: CommitteeThreshold>(
        block_ref: BlockRef,
        transaction_index: TransactionIndex,
        accept_votes: &StakeAggregator<T>,
        reject_votes: &StakeAggregator<QuorumThreshold>,
        first_votes: &[VotingBlock],
    ) -> Self {
        Self {
            block_ref,
            transaction_index,
            accept_stake: accept_votes.stake(),
            reject_stake: reject_votes.stake(),
            accept_voters: accept_votes.authorities().iter().copied().collect(),
            reject_voters: reject_votes.authorities().iter().copied().collect(),
            first_votes: first_votes
                .iter()
                .map(|vote| (vote.block_ref, vote.cutoff_round))
                .collect(),
        }
    }

    fn report(
        &self,
        commit_index: CommitIndex,
        path: &str,
        gc_round: Round,
        accept_threshold: Stake,
        reject_threshold: Stake,
    ) {
        tracing::info!(
            commit_index, path,
            block = %self.block_ref,
            transaction_index = self.transaction_index,
            vote_gc_round = gc_round,
            target_at_or_below_gc = self.block_ref.round <= gc_round,
            accept_stake = self.accept_stake,
            reject_stake = self.reject_stake,
            accept_threshold, reject_threshold,
            accept_stake_gap = accept_threshold.saturating_sub(self.accept_stake),
            reject_stake_gap = reject_threshold.saturating_sub(self.reject_stake),
            accept_voters = ?self.accept_voters,
            reject_voters = ?self.reject_voters,
            first_votes_and_cutoffs = ?self.first_votes,
            "V3 finalizer pending vote sample from the last attempt"
        );
    }
}

struct CommitStateV3 {
    commit: CommittedSubDag,
    pending_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    rejected_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    received_at: Instant,
    ready_at: Option<Instant>,
    attempts: u64,
    last_status_log_at: Option<Instant>,
    pending_direct_vote: Option<(Round, PendingVoteStatus)>,
    pending_indirect_vote: Option<PendingVoteStatus>,
}

impl CommitStateV3 {
    fn new(commit: CommittedSubDag) -> Self {
        let pending_transactions: BTreeMap<_, BTreeSet<_>> = commit
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
        let received_at = Instant::now();
        let ready_at = pending_transactions.is_empty().then_some(received_at);
        Self {
            commit,
            pending_transactions,
            rejected_transactions: BTreeMap::new(),
            received_at,
            ready_at,
            attempts: 0,
            last_status_log_at: None,
            pending_direct_vote: None,
            pending_indirect_vote: None,
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
        if self.pending_transactions.is_empty() {
            self.ready_at.get_or_insert_with(Instant::now);
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
            Self::new_with_protocol_config(|_| {})
        }

        fn with_gc_depth(gc_depth: Round) -> Self {
            Self::new_with_protocol_config(|context| {
                context.protocol_config.set_gc_depth_for_testing(gc_depth);
            })
        }

        fn new_with_protocol_config(configure: impl FnOnce(&mut Context)) -> Self {
            let fixture = Self::with_fault_budget(COMMITTEE_SIZE, 1, 0, configure);
            assert_eq!(fixture.context.committee.quorum_threshold(), 5);
            assert_eq!(fixture.context.committee.certification_threshold(), 3);
            fixture
        }

        fn with_fault_budget(
            committee_size: usize,
            malicious_stake: u64,
            crash_stake: u64,
            configure: impl FnOnce(&mut Context),
        ) -> Self {
            let (mut context, _) = Context::new_with_test_options(committee_size, false);
            configure(&mut context);
            let committee = Committee::new_v3(
                context.committee.epoch(),
                context.committee.authorities_slice().to_vec(),
                malicious_stake,
                crash_stake,
            );
            context = context.with_committee(committee);
            context.protocol_config.set_enable_v3_for_testing(true);
            let context = Arc::new(context);

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
            let blocks: Vec<_> = (0..self.context.committee.size() as u32)
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

        fn make_round_two_target(
            &self,
            num_target_transactions: usize,
        ) -> (VerifiedBlock, Vec<VerifiedBlock>) {
            let round_one_blocks = self.make_round_one_blocks(&[]);
            let round_one_refs: Vec<_> = round_one_blocks
                .iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..COMMITTEE_SIZE as u32)
                .map(|author| {
                    let own_authority = AuthorityIndex::new_for_test(author);
                    let mut ancestors = round_one_refs.clone();
                    ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
                    let transactions = if author == 0 {
                        vec![Transaction::new(vec![1]); num_target_transactions]
                    } else {
                        vec![]
                    };
                    VerifiedBlock::new_for_test(
                        TestBlock::new(2, author)
                            .set_ancestors(ancestors)
                            .set_transactions(transactions)
                            .build_v3(0),
                    )
                })
                .collect();
            self.add_blocks(&blocks);
            (blocks[0].clone(), blocks)
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
    async fn first_votes_stop_at_voting_window() {
        struct CountingGraph<'a> {
            dag: &'a DagState,
            last_voting_round: Round,
            boundary_reads: std::cell::Cell<usize>,
        }

        impl ReverseBlockGraph for CountingGraph<'_> {
            fn block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock> {
                self.dag.get_block(block_ref)
            }

            fn children(&self, block_ref: &BlockRef) -> Vec<BlockRef> {
                if block_ref.round >= self.last_voting_round {
                    self.boundary_reads.set(self.boundary_reads.get() + 1);
                }
                self.dag.get_block_children(block_ref).unwrap_or_default()
            }
        }

        let fixture = Fixture::with_fault_budget(127, 21, 0, |_| {});
        let targets = fixture.make_round_one_blocks(&[]);
        let mut parents: Vec<_> = targets.iter().map(|block| block.reference()).collect();
        let mut voting_refs = vec![];
        for round in 2..=4 {
            let blocks: Vec<_> = (0..127)
                .map(|author| fixture.make_graph_block(round, author, parents.clone(), vec![], 0))
                .collect();
            fixture.add_blocks(&blocks);
            parents = blocks.iter().map(|block| block.reference()).collect();
            if round == 2 {
                voting_refs = parents.clone();
            }
        }
        let dag = fixture.dag_state.read();
        for last_voting_round in [2, 3] {
            let graph = CountingGraph {
                dag: &dag,
                last_voting_round,
                boundary_reads: std::cell::Cell::new(0),
            };
            for target in &targets {
                let votes = collect_first_votes(&graph, target.reference(), last_voting_round);
                assert_eq!(
                    votes.iter().map(|vote| vote.block_ref).collect::<Vec<_>>(),
                    voting_refs,
                );
            }
            assert_eq!(graph.boundary_reads.get(), 0);
        }
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
    async fn direct_accepts_above_local_gc_round() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_two_blocks) = fixture.make_round_two_target(1);
        let round_two_refs: Vec<_> = round_two_blocks
            .iter()
            .map(|block| block.reference())
            .collect();
        let voters: Vec<_> = (0..5)
            .map(|author| fixture.make_graph_block(3, author, round_two_refs.clone(), vec![], 0))
            .collect();
        fixture.add_blocks(&voters);

        let first_leader = fixture.make_graph_block(
            4,
            0,
            voters
                .iter()
                .map(|voting_block| voting_block.reference())
                .collect(),
            vec![],
            0,
        );
        fixture.add_blocks(std::slice::from_ref(&first_leader));
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let commit = linearizer.handle_commit(vec![first_leader]).pop().unwrap();
        assert_eq!(fixture.dag_state.read().gc_round(), 1);
        assert_eq!(target.round(), fixture.dag_state.read().gc_round() + 1);

        // Linearization advances local GC before finalization. The target and its first votes
        // remain above that cutoff, so the finalizer can still use this accept quorum.
        let finalized = fixture.finalizer.process_commit(commit);

        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn gc_guard_counts_indirect_accept_evidence_above_commit_gc() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..3)
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
        fixture.add_blocks(&voters);
        let non_voters: Vec<_> = (3..5)
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
        fixture.add_blocks(&non_voters);

        let first_leader = fixture.make_anchor(
            voters
                .iter()
                .chain(&non_voters)
                .map(|voting_block| voting_block.reference())
                .collect(),
        );
        fixture.add_blocks(std::slice::from_ref(&first_leader));
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let first_commit = linearizer
            .handle_commit(vec![first_leader.clone()])
            .pop()
            .unwrap();
        assert_eq!(target.round(), fixture.dag_state.read().gc_round() + 1);
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());

        // These peers provide the anchor's parent quorum without casting additional accepts.
        let round_three_peers: Vec<_> = (1..5)
            .map(|author| {
                fixture.make_graph_block(
                    3,
                    author,
                    first_leader.ancestors().to_vec(),
                    vec![],
                    target.round(),
                )
            })
            .collect();
        fixture.add_blocks(&round_three_peers);
        let anchor_ancestors = std::iter::once(first_leader.reference())
            .chain(round_three_peers.iter().map(|block| block.reference()))
            .collect();
        let anchor = fixture.make_graph_block(4, 0, anchor_ancestors, vec![], 0);
        fixture.add_blocks(std::slice::from_ref(&anchor));
        let second_commit = linearizer.handle_commit(vec![anchor]).pop().unwrap();
        assert_eq!(target.round(), fixture.dag_state.read().gc_round());
        let finalized = fixture.finalizer.process_commit(second_commit);

        assert_eq!(finalized.len(), 2);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn gc_guard_rejects_target_at_commit_gc_round() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..5)
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
        fixture.add_blocks(&voters);

        let mut previous_round = voters;
        let mut leaders = vec![];
        for round in 3..=6 {
            let ancestors: Vec<_> = previous_round
                .iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..5)
                .map(|author| fixture.make_graph_block(round, author, ancestors.clone(), vec![], 0))
                .collect();
            fixture.add_blocks(&blocks);
            if round >= 4 {
                leaders.push(blocks[0].clone());
            }
            previous_round = blocks;
        }
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let first_commit = linearizer
            .handle_commit(vec![leaders[0].clone()])
            .pop()
            .unwrap();

        // At this boundary local traversal already hides the target's children. The indirect
        // guard must also exclude the committed accept quorum, whose first-vote history is no
        // longer guaranteed to survive until the depth-two decision.
        assert_eq!(target.round(), fixture.dag_state.read().gc_round());
        assert!(
            fixture
                .dag_state
                .read()
                .get_block_children(&target.reference())
                .is_none()
        );
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());

        let second_commit = linearizer
            .handle_commit(vec![leaders[1].clone()])
            .pop()
            .unwrap();
        assert!(fixture.finalizer.process_commit(second_commit).is_empty());
        let third_commit = linearizer
            .handle_commit(vec![leaders[2].clone()])
            .pop()
            .unwrap();
        let finalized = fixture.finalizer.process_commit(third_commit);

        assert_eq!(finalized.len(), 3);
        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
        let target_hostname = &fixture
            .context
            .committee
            .authority(target.author())
            .hostname;
        assert_eq!(
            fixture
                .context
                .metrics
                .node_metrics
                .finalizer_skipped_voting_blocks
                .with_label_values(&[target_hostname, "direct"])
                .get(),
            1
        );
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn gc_guard_prevents_later_vote_from_replacing_a_pruned_reject() {
        let mut fixture = Fixture::with_gc_depth(3);
        let round_one = fixture.make_round_one_blocks(&[0, 0, 0, 0, 0, 1]);
        let target = round_one[5].clone();
        let target_ref = target.reference();
        let reject = || {
            vec![BlockTransactionVotes {
                block_ref: target_ref,
                rejects: vec![0],
            }]
        };

        // Authority 4 rejects the target in round 2, then its branch is withheld from the
        // round-4 and round-5 leaders. Every block still has a quorum of previous-round parents.
        // Only authorities 3 and 5 accept; authorities 0..=2 first see and reject it in round 3.
        let mut rounds = vec![round_one];
        for round in 2..=5 {
            let previous = rounds.last().unwrap();
            let blocks: Vec<_> = (0..COMMITTEE_SIZE as u32)
                .filter(|author| round == 2 || *author != 4)
                .map(|author| {
                    let ancestors = previous
                        .iter()
                        .filter(|block| {
                            if round == 2 {
                                author >= 4 || block.author() != target.author()
                            } else {
                                block.author() != AuthorityIndex::new_for_test(4)
                            }
                        })
                        .map(|block| block.reference())
                        .collect();
                    let votes = if (round == 2 && author == 4) || (round == 3 && author < 3) {
                        reject()
                    } else {
                        vec![]
                    };
                    fixture.make_graph_block(round, author, ancestors, votes, 0)
                })
                .collect();
            fixture.add_blocks(&blocks);
            rounds.push(blocks);
        }
        let first_reject = &rounds[1][4];

        // This proposal was created before GC, so its signed cutoff remains zero. Its own
        // round-2 ancestor already included the target, so proposal logic does not repeat the
        // reject. Its other parents also provide a path back to the target.
        let mut later_ancestors: Vec<_> = rounds[2].iter().map(|block| block.reference()).collect();
        later_ancestors.push(first_reject.reference());
        let later_vote = fixture.make_graph_block(4, 4, later_ancestors, vec![], 0);
        fixture.add_blocks(std::slice::from_ref(&later_vote));
        let mut anchor_ancestors: Vec<_> =
            rounds[4].iter().map(|block| block.reference()).collect();
        anchor_ancestors.push(later_vote.reference());
        let depth_two_anchor = fixture.make_graph_block(6, 0, anchor_ancestors, vec![], 0);
        fixture.add_blocks(std::slice::from_ref(&depth_two_anchor));

        let transaction_indices = BTreeSet::from([0]);
        let complete_first_votes = collect_first_votes(&*fixture.dag_state.read(), target_ref, 5);
        let complete_accepts: TransactionVotes<CertificationThreshold> = fixture
            .finalizer
            .collect_accept_votes(target_ref, &transaction_indices, &complete_first_votes);
        assert_eq!(complete_accepts.for_transaction(0).stake(), 2);
        assert!(
            !complete_accepts
                .for_transaction(0)
                .reached_threshold(&fixture.context.committee)
        );

        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let first_commit = linearizer
            .handle_commit(vec![rounds[3][0].clone()])
            .pop()
            .unwrap();
        assert!(
            first_commit
                .blocks
                .iter()
                .any(|block| block.reference() == target_ref)
        );
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());
        let second_commit = linearizer
            .handle_commit(vec![rounds[4][0].clone()])
            .pop()
            .unwrap();
        assert!(fixture.finalizer.process_commit(second_commit).is_empty());
        assert_eq!(fixture.dag_state.read().gc_round(), first_reject.round());

        let third_commit = linearizer
            .handle_commit(vec![depth_two_anchor])
            .pop()
            .unwrap();
        let committed_graph = CommittedBlockGraph::new(
            fixture
                .finalizer
                .pending_commits
                .iter()
                .flat_map(|state| state.commit.blocks.iter().cloned())
                .chain(third_commit.blocks.iter().cloned()),
        );
        assert!(
            !committed_graph
                .blocks
                .contains_key(&first_reject.reference())
        );
        assert!(committed_graph.blocks.contains_key(&later_vote.reference()));
        assert_eq!(later_vote.transaction_votes_cutoff_round(), 0);
        assert!(VotingBlock::new(later_vote.clone()).can_accept(target_ref));

        // The real linearizer omitted the round-2 reject at GC, but retained the round-4
        // descendant. The per-block cutoff therefore permits a false accept certificate.
        let incomplete_first_votes = collect_first_votes(&committed_graph, target_ref, 5);
        assert!(
            incomplete_first_votes
                .iter()
                .any(|vote| vote.block_ref == later_vote.reference())
        );
        let incomplete_accepts: TransactionVotes<CertificationThreshold> = fixture
            .finalizer
            .collect_accept_votes(target_ref, &transaction_indices, &incomplete_first_votes);
        assert_eq!(incomplete_accepts.for_transaction(0).stake(), 3);
        assert!(
            incomplete_accepts
                .for_transaction(0)
                .reached_threshold(&fixture.context.committee)
        );

        let finalized = fixture.finalizer.process_commit(third_commit);
        assert_eq!(finalized.len(), 3);
        assert_eq!(
            finalized[0].rejected_transactions_by_block.get(&target_ref),
            Some(&vec![0])
        );
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn direct_rejects_all_transactions_with_a_cutoff_quorum() {
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

        assert_eq!(finalized.len(), 1);
        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0, 1])
        );
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn cutoff_and_explicit_rejects_combine_per_transaction() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(2);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                let rejects = match author {
                    2 => vec![0, 1],
                    3 | 4 => vec![0],
                    _ => vec![],
                };
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    rejects,
                    if author < 2 { 1 } else { 0 },
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        let graph = CommittedBlockGraph::new(voters);
        let first_votes = collect_first_votes(&graph, target.reference(), 2);
        let transaction_indices = BTreeSet::from([0, 1]);

        // Both transactions have two cutoff rejects. Explicit rejects bring only transaction 0
        // to quorum; transaction 1 has three rejects and two accepts, so it remains pending.
        for decisions in [
            fixture.finalizer.compute_direct_decisions(
                target.reference(),
                &transaction_indices,
                &first_votes,
            ),
            fixture.finalizer.compute_indirect_decisions(
                target.reference(),
                &transaction_indices,
                &first_votes,
                false,
            ),
        ] {
            assert!(decisions.accepted.is_empty());
            assert_eq!(decisions.rejected, vec![0]);
        }
        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        assert_eq!(
            fixture.finalizer.pending_commits[0]
                .pending_transactions
                .get(&target.reference()),
            Some(&BTreeSet::from([1]))
        );
    }

    #[tokio::test]
    async fn cutoff_and_explicit_rejects_do_not_double_count_an_equivocator() {
        let fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters: Vec<_> = (0..3)
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
        voters.extend((2..4).map(|author| {
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
        let transaction_indices = BTreeSet::from([0]);

        // Authority 2's cutoff and explicit reject are two branches of the same vote. Only the
        // later addition of authority 4 can bring the four distinct reject voters to quorum.
        for reaches_quorum in [false, true] {
            if reaches_quorum {
                let fifth_reject = fixture.make_voter(
                    4,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![0],
                    0,
                    None,
                );
                fixture.add_blocks(std::slice::from_ref(&fifth_reject));
                voters.push(fifth_reject);
            }
            let graph = CommittedBlockGraph::new(voters.iter().cloned());
            let first_votes = collect_first_votes(&graph, target.reference(), 2);
            for decisions in [
                fixture.finalizer.compute_direct_decisions(
                    target.reference(),
                    &transaction_indices,
                    &first_votes,
                ),
                fixture.finalizer.compute_indirect_decisions(
                    target.reference(),
                    &transaction_indices,
                    &first_votes,
                    false,
                ),
            ] {
                assert!(decisions.accepted.is_empty());
                assert_eq!(
                    decisions.rejected,
                    if reaches_quorum { vec![0] } else { vec![] }
                );
            }
        }
    }

    #[tokio::test]
    async fn cutoff_certificate_without_quorum_waits_until_depth_two() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    vec![],
                    if author < 3 { 1 } else { 0 },
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        let mut previous_round = voters;
        let mut leaders = vec![];
        for round in 3..=5 {
            let ancestors: Vec<_> = previous_round
                .iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..5)
                .map(|author| fixture.make_graph_block(round, author, ancestors.clone(), vec![], 0))
                .collect();
            fixture.add_blocks(&blocks);
            leaders.push(blocks[0].clone());
            previous_round = blocks;
        }

        // Three cutoff rejects reach certification stake, but rejection requires the full
        // five-authority quorum. Two accepts also fall short of an accept certificate.
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        for (depth, leader) in leaders.into_iter().enumerate() {
            let commit = linearizer.handle_commit(vec![leader]).pop().unwrap();
            let finalized = fixture.finalizer.process_commit(commit);
            if depth < 2 {
                assert!(finalized.is_empty());
            } else {
                assert_eq!(finalized.len(), 3);
                assert_eq!(
                    finalized[0]
                        .rejected_transactions_by_block
                        .get(&target.reference()),
                    Some(&vec![0])
                );
            }
        }
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn later_cutoffs_cannot_retract_first_accept_votes() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let first_accepts: Vec<_> = (0..5)
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
        fixture.add_blocks(&first_accepts);
        let ancestors: Vec<_> = first_accepts
            .iter()
            .map(|block| block.reference())
            .collect();
        let later_cutoffs: Vec<_> = (0..5)
            .map(|author| fixture.make_graph_block(3, author, ancestors.clone(), vec![], 1))
            .collect();
        fixture.add_blocks(&later_cutoffs);

        let graph = CommittedBlockGraph::new(first_accepts.iter().chain(&later_cutoffs).cloned());
        let first_votes = collect_first_votes(&graph, target.reference(), 3);
        assert!(first_votes.iter().all(|vote| vote.block_ref.round == 2));
        let indirect = fixture.finalizer.compute_indirect_decisions(
            target.reference(),
            &BTreeSet::from([0]),
            &first_votes,
            false,
        );
        assert_eq!(indirect.accepted, vec![0]);
        assert!(indirect.rejected.is_empty());

        let finalized = fixture.finalizer.process_commit(make_commit(
            1,
            &first_accepts[0],
            vec![target.clone(), first_accepts[0].clone()],
        ));
        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn indirect_rejects_a_cutoff_quorum_before_depth_two_after_local_gc() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    author < 4,
                    vec![],
                    1,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        let first_leader =
            fixture.make_anchor(voters.iter().map(|block| block.reference()).collect());
        fixture.add_blocks(std::slice::from_ref(&first_leader));
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let first_commit = linearizer
            .handle_commit(vec![first_leader.clone()])
            .pop()
            .unwrap();
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());

        // Authority 4's round-3 block is its first vote on the target. Linearizing the anchor
        // advances local GC before direct finalization can count this fifth cutoff reject.
        let round_three_peers: Vec<_> = (1..5)
            .map(|author| {
                fixture.make_graph_block(3, author, first_leader.ancestors().to_vec(), vec![], 1)
            })
            .collect();
        fixture.add_blocks(&round_three_peers);
        let ancestors = std::iter::once(first_leader.reference())
            .chain(round_three_peers.iter().map(|block| block.reference()))
            .collect();
        let anchor = fixture.make_graph_block(4, 0, ancestors, vec![], 0);
        fixture.add_blocks(std::slice::from_ref(&anchor));
        let second_commit = linearizer.handle_commit(vec![anchor]).pop().unwrap();
        assert_eq!(fixture.dag_state.read().gc_round(), target.round());
        assert!(
            fixture
                .dag_state
                .read()
                .get_block_children(&target.reference())
                .is_none()
        );
        let finalized = fixture.finalizer.process_commit(second_commit);
        assert_eq!(finalized.len(), 2);
        assert_eq!(
            finalized[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
        let statuses = &fixture
            .context
            .metrics
            .node_metrics
            .finalizer_transaction_status;
        assert_eq!(statuses.with_label_values(&["direct_reject"]).get(), 0);
        assert_eq!(statuses.with_label_values(&["indirect_reject"]).get(), 1);
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    #[should_panic(expected = "cannot have both")]
    async fn indirect_detects_conflicting_accept_and_cutoff_quorums() {
        let fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let mut voters: Vec<_> = (0..3)
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
                vec![],
                1,
                Some(author as u8),
            )
        }));
        let graph = CommittedBlockGraph::new(voters);
        let first_votes = collect_first_votes(&graph, target.reference(), 2);
        fixture.finalizer.compute_indirect_decisions(
            target.reference(),
            &BTreeSet::from([0]),
            &first_votes,
            false,
        );
    }

    #[tokio::test]
    async fn cutoff_votes_remain_safe_with_byzantine_and_crash_stake() {
        // Cover Byzantine-only, hybrid, crash-only, and one Byzantine authority with stake two.
        for (size, byzantine_stake, crash_stake, weighted) in [
            (6, 1, 0, false),
            (9, 1, 1, false),
            (4, 0, 1, false),
            (10, 2, 0, true),
        ] {
            for reject_quorum in [false, true] {
                // Separate observers can receive different forks from the Byzantine authority.
                for view in [0, 1, 2] {
                    let active_authorities = size - crash_stake as usize;
                    let byzantine_author = active_authorities - 1;
                    let fixture =
                        Fixture::with_fault_budget(size, byzantine_stake, crash_stake, |context| {
                            if weighted {
                                let mut authorities =
                                    context.committee.authorities_slice().to_vec();
                                authorities[byzantine_author].stake = 2;
                                context.committee =
                                    Committee::new(context.committee.epoch(), authorities);
                            }
                        });
                    let committee = &fixture.context.committee;
                    let quorum = 4 * byzantine_stake + 2 * crash_stake + 1;
                    let certificate = 2 * byzantine_stake + crash_stake + 1;
                    assert_eq!(committee.quorum_threshold(), quorum);
                    assert_eq!(committee.certification_threshold(), certificate);

                    // The target's author is either Byzantine or crashes before round two, so
                    // no honest proposer needs to emit an explicit reject for its own block.
                    let mut transaction_counts = vec![0; size];
                    transaction_counts[size - 1] = 1;
                    let round_one_blocks = fixture.make_round_one_blocks(&transaction_counts);
                    let target = &round_one_blocks[size - 1];
                    let round_one_refs: Vec<_> = round_one_blocks
                        .iter()
                        .map(|block| block.reference())
                        .collect();
                    let faulty_authorities = usize::from(byzantine_stake > 0);
                    let honest_accepts = if reject_quorum {
                        // Honest rejects plus Byzantine stake reach exactly Q.
                        committee.total_stake() - crash_stake - quorum
                    } else {
                        // Honest accepts plus Byzantine stake reach exactly C.
                        certificate - byzantine_stake
                    } as usize;
                    let mut voters: Vec<_> = (0..active_authorities - faulty_authorities)
                        .map(|author| {
                            let accepts = author < honest_accepts;
                            let explicit_reject = !accepts && author % 2 == 0;
                            fixture.make_voter(
                                author as u32,
                                &round_one_refs,
                                target.reference(),
                                true,
                                if explicit_reject { vec![0] } else { vec![] },
                                u32::from(!accepts && !explicit_reject),
                                None,
                            )
                        })
                        .collect();
                    // The crashed authority produced round one, then stopped before voting.
                    // The Byzantine authority can send an accept, rejects, or both to an observer.
                    if byzantine_stake > 0 && view != 1 {
                        voters.push(fixture.make_voter(
                            byzantine_author as u32,
                            &round_one_refs,
                            target.reference(),
                            true,
                            vec![],
                            0,
                            None,
                        ));
                    }
                    if byzantine_stake > 0 && view != 0 {
                        for marker in 1..=3 {
                            voters.push(fixture.make_voter(
                                byzantine_author as u32,
                                &round_one_refs,
                                target.reference(),
                                true,
                                vec![],
                                1,
                                Some(marker),
                            ));
                        }
                        voters.push(fixture.make_voter(
                            byzantine_author as u32,
                            &round_one_refs,
                            target.reference(),
                            true,
                            vec![0],
                            0,
                            Some(4),
                        ));
                    }
                    fixture.add_blocks(&voters);
                    let graph = CommittedBlockGraph::new(voters);
                    let first_votes = collect_first_votes(&graph, target.reference(), 2);
                    let transactions = BTreeSet::from([0]);
                    let direct = fixture.finalizer.compute_direct_decisions(
                        target.reference(),
                        &transactions,
                        &first_votes,
                    );
                    let indirect = fixture.finalizer.compute_indirect_decisions(
                        target.reference(),
                        &transactions,
                        &first_votes,
                        false,
                    );
                    assert!(direct.accepted.is_empty());
                    let should_reject = reject_quorum && (byzantine_stake == 0 || view != 0);
                    let expected_rejects = if should_reject { vec![0] } else { vec![] };
                    assert_eq!(direct.rejected, expected_rejects);
                    assert_eq!(indirect.rejected, expected_rejects);
                    let should_accept = !reject_quorum && (byzantine_stake == 0 || view != 1);
                    assert_eq!(
                        indirect.accepted,
                        if should_accept { vec![0] } else { vec![] }
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn high_cutoffs_without_a_causal_link_do_not_reject() {
        let fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (1..6)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    false,
                    vec![],
                    1,
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        let committed = CommittedBlockGraph::new(voters);
        let local_votes = collect_first_votes(&*fixture.dag_state.read(), target.reference(), 2);
        let committed_votes = collect_first_votes(&committed, target.reference(), 2);
        assert!(local_votes.is_empty());
        assert!(committed_votes.is_empty());
        let transactions = BTreeSet::from([0]);
        let direct = fixture.finalizer.compute_direct_decisions(
            target.reference(),
            &transactions,
            &local_votes,
        );
        let indirect = fixture.finalizer.compute_indirect_decisions(
            target.reference(),
            &transactions,
            &committed_votes,
            false,
        );
        assert!(direct.accepted.is_empty() && direct.rejected.is_empty());
        assert!(indirect.accepted.is_empty() && indirect.rejected.is_empty());
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

    #[tokio::test(start_paused = true)]
    async fn diagnostics_separate_vote_wait_from_ordered_release() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let later_leader = fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        fixture.add_blocks(std::slice::from_ref(&later_leader));
        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(1, &target, vec![target.clone()]))
                .is_empty()
        );
        tokio::time::advance(Duration::from_millis(100)).await;
        assert!(
            fixture
                .finalizer
                .process_commit(make_commit(2, &later_leader, vec![later_leader.clone()]))
                .is_empty()
        );

        let metrics = &fixture.context.metrics.node_metrics;
        assert_eq!(metrics.finalizer_v3_pending_transactions.get(), 1);
        assert_eq!(metrics.finalizer_v3_ready_commits.get(), 1);
        assert_eq!(metrics.finalizer_v3_anchor_round_gap.get(), 1);
        let (_, sample) = fixture.finalizer.pending_commits[0]
            .pending_direct_vote
            .as_ref()
            .unwrap();
        assert_eq!(sample.block_ref, target.reference());
        assert_eq!((sample.accept_stake, sample.reject_stake), (0, 0));

        tokio::time::advance(Duration::from_millis(400)).await;
        let voters: Vec<_> = (0..5)
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
        fixture.add_blocks(&voters);
        let finalized = fixture.finalizer.try_finalize_commits("block_update");
        assert_eq!(
            finalized
                .iter()
                .map(|commit| commit.commit_ref.index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        for (stage, expected) in [("decision", 0.5), ("ordered_release", 0.4), ("total", 0.9)] {
            let histogram = metrics
                .finalizer_v3_commit_wait_seconds
                .with_label_values(&[stage]);
            assert_eq!(histogram.get_sample_count(), 2);
            assert!((histogram.get_sample_sum() - expected).abs() < 1e-9);
        }
        assert_eq!(metrics.finalizer_v3_pending_transactions.get(), 0);
        assert_eq!(metrics.finalizer_v3_ready_commits.get(), 0);
        assert_eq!(metrics.finalizer_v3_oldest_pending_seconds.get(), 0.0);
        assert_eq!(metrics.finalizer_v3_anchor_round_gap.get(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn status_timer_reports_age_without_retrying_or_consuming_notifications() {
        let mut fixture = Fixture::new();
        let (target, round_one_refs) = fixture.make_round_one(1);
        let voters: Vec<_> = (0..5)
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
        let metrics = &fixture.context.metrics.node_metrics;
        let (commit_sender, mut commit_receiver) = unbounded_channel("finalizer_v3_status_output");
        fixture.finalizer.commit_sender = commit_sender;
        let (sender, receiver) = unbounded_channel("finalizer_v3_status_input");
        let (block_sender, block_updates) = watch::channel(());
        let run = fixture.finalizer.run(receiver, block_updates);
        tokio::pin!(run);
        sender
            .send(make_commit(1, &target, vec![target.clone()]))
            .unwrap();
        assert!(futures::poll!(&mut run).is_pending());

        // Evidence alone must not make the status timer run a finalization pass.
        fixture.dag_state.write().accept_blocks(voters.clone());
        fixture
            .transaction_vote_tracker
            .add_voted_blocks(voters.into_iter().map(|block| (block, vec![])).collect());
        tokio::time::advance(STATUS_INTERVAL).await;
        assert!(futures::poll!(&mut run).is_pending());
        assert_eq!(metrics.finalizer_v3_oldest_pending_seconds.get(), 1.0);
        assert_eq!(metrics.finalizer_v3_pending_transactions.get(), 1);
        assert!(commit_receiver.try_recv().is_err());
        assert_eq!(
            metrics
                .finalizer_v3_attempts
                .with_label_values(&["commit"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .finalizer_v3_attempts
                .with_label_values(&["block_update"])
                .get(),
            0
        );

        block_sender.send_replace(());
        assert!(futures::poll!(&mut run).is_pending());
        assert_eq!(commit_receiver.try_recv().unwrap().commit_ref.index, 1);
        assert_eq!(
            metrics
                .finalizer_v3_attempts
                .with_label_values(&["block_update"])
                .get(),
            1
        );
        assert_eq!(metrics.finalizer_v3_oldest_pending_seconds.get(), 0.0);
        drop(sender);
        assert!(futures::poll!(&mut run).is_ready());
    }

    #[tokio::test]
    async fn block_notifications_are_coalesced_and_rearmed() {
        let fixture = Fixture::new();
        let (target, _) = fixture.make_round_one(1);
        let passes = fixture
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["CommitFinalizer::try_finalize_commits"]);
        let (sender, receiver) = unbounded_channel("finalizer_v3_coalescing_test");
        let (block_sender, block_updates) = watch::channel(());
        let run = fixture.finalizer.run(receiver, block_updates);
        tokio::pin!(run);

        // Notifications while idle must neither run the finalizer nor leave an extra pass
        // behind the next commit, which already checks all available evidence.
        for _ in 0..10 {
            block_sender.send_replace(());
        }
        assert!(futures::poll!(&mut run).is_pending());
        assert_eq!(passes.get_sample_count(), 0);
        sender
            .send(make_commit(1, &target, vec![target.clone()]))
            .unwrap();
        assert!(futures::poll!(&mut run).is_pending());
        assert_eq!(passes.get_sample_count(), 1);

        for (batch, notifications) in [1, 10, 1].into_iter().enumerate() {
            for _ in 0..notifications {
                block_sender.send_replace(());
            }
            assert!(futures::poll!(&mut run).is_pending());
            assert_eq!(passes.get_sample_count(), batch as u64 + 2);
            assert!(futures::poll!(&mut run).is_pending());
            assert_eq!(passes.get_sample_count(), batch as u64 + 2);
        }

        // Closing notifications alone must not terminate or spin the commit receiver.
        drop(block_sender);
        assert!(futures::poll!(&mut run).is_pending());
        drop(sender);
        assert!(futures::poll!(&mut run).is_ready());
    }

    #[tokio::test]
    async fn block_notification_finalizes_pending_commits_in_order() {
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
        fixture.add_blocks(&voters[..4]);
        let later_leader = fixture.make_voter(
            5,
            &round_one_refs,
            target.reference(),
            false,
            vec![],
            0,
            None,
        );
        fixture.add_blocks(std::slice::from_ref(&later_leader));

        let (commit_sender, mut commit_receiver) = unbounded_channel("finalizer_v3_notify_test");
        let mut handle = CommitFinalizerHandle::start(
            fixture.context.clone(),
            fixture.dag_state.clone(),
            fixture.transaction_vote_tracker.clone(),
            commit_sender,
        );
        let first = make_commit(1, &target, vec![target.clone()]);
        let second = make_commit(2, &later_leader, vec![later_leader.clone()]);
        handle.send(first.clone()).unwrap();
        handle.send(second.clone()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while fixture
                .context
                .metrics
                .node_metrics
                .finalizer_buffered_commits
                .get()
                != 2
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Both commits must be buffered before the final vote arrives");
        assert!(commit_receiver.try_recv().is_err());

        fixture.add_blocks(&voters[4..]);
        handle.notify_new_blocks();
        for expected in [&first, &second] {
            let finalized = tokio::time::timeout(Duration::from_secs(1), commit_receiver.recv())
                .await
                .expect("New block evidence must finalize without another commit")
                .unwrap();
            assert_eq!(finalized.commit_ref, expected.commit_ref);
            if finalized.commit_ref == first.commit_ref {
                assert_eq!(
                    finalized
                        .rejected_transactions_by_block
                        .get(&target.reference()),
                    Some(&vec![1])
                );
            }
            assert_eq!(
                fixture
                    .store
                    .read_rejected_transactions(finalized.commit_ref)
                    .unwrap(),
                Some(finalized.rejected_transactions_by_block)
            );
        }
        assert_eq!(
            fixture.store.read_last_finalized_commit().unwrap(),
            Some(second.commit_ref)
        );
        handle.stop().await;
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

    #[tokio::test]
    async fn recovery_recomputes_partial_cutoff_rejections_after_local_gc() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(2);
        let voters: Vec<_> = (0..5)
            .map(|author| {
                fixture.make_voter(
                    author,
                    &round_one_refs,
                    target.reference(),
                    true,
                    match author {
                        2 => vec![0, 1],
                        3 | 4 => vec![0],
                        _ => vec![],
                    },
                    if author < 2 { 1 } else { 0 },
                    None,
                )
            })
            .collect();
        fixture.add_blocks(&voters);
        let mut previous_round = voters;
        let mut leaders = vec![];
        for round in 3..=5 {
            let ancestors: Vec<_> = previous_round
                .iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..5)
                .map(|author| fixture.make_graph_block(round, author, ancestors.clone(), vec![], 0))
                .collect();
            fixture.add_blocks(&blocks);
            leaders.push(blocks[0].clone());
            previous_round = blocks;
        }

        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        for leader in &leaders[..2] {
            let commit = linearizer
                .handle_commit(vec![leader.clone()])
                .pop()
                .unwrap();
            assert!(fixture.finalizer.process_commit(commit).is_empty());
        }
        let first_state = &fixture.finalizer.pending_commits[0];
        assert_eq!(
            first_state.rejected_transactions.get(&target.reference()),
            Some(&BTreeSet::from([0]))
        );
        assert_eq!(
            first_state.pending_transactions.get(&target.reference()),
            Some(&BTreeSet::from([1]))
        );
        let first_commit_ref = first_state.commit.commit_ref;
        assert_eq!(fixture.dag_state.read().gc_round(), target.round());
        fixture.dag_state.write().flush();
        assert!(
            fixture
                .store
                .read_rejected_transactions(first_commit_ref)
                .unwrap()
                .is_none()
        );

        // Only commits and blocks are durable at this crash point. The partial transaction-0
        // rejection exists only in the finalizer, and local GC already hides the target.
        // First compute the uninterrupted result without flushing past that durable image.
        let last_commit = linearizer
            .handle_commit(vec![leaders[2].clone()])
            .pop()
            .unwrap();
        let expected = fixture.finalizer.process_commit(last_commit);
        assert_eq!(expected.len(), 3);
        assert_eq!(
            expected[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0, 1])
        );
        let context = fixture.context.clone();
        let store = fixture.store.clone();
        drop(linearizer);
        drop(fixture);

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        assert_eq!(dag_state.read().gc_round(), target.round());
        assert!(
            dag_state
                .read()
                .get_block_children(&target.reference())
                .is_none()
        );
        let tracker = TransactionVoteTracker::new(
            context.clone(),
            Arc::new(NoopBlockVerifier),
            dag_state.clone(),
        );
        assert!(tracker.get_reject_votes(&target.reference()).is_none());
        let indirect_rejects = context
            .metrics
            .node_metrics
            .finalizer_transaction_status
            .with_label_values(&["indirect_reject"]);
        let rejects_before_recovery = indirect_rejects.get();
        let (consumer, mut receiver) = crate::commit_consumer::CommitConsumerArgs::new(0, 0);
        let mut observer = crate::commit_observer::CommitObserver::new(
            context,
            consumer,
            dag_state,
            tracker.clone(),
        )
        .await;
        assert_eq!(
            tracker.get_reject_votes(&target.reference()),
            Some(vec![(0, 3), (1, 1)])
        );
        // Replay must reconstruct transaction 0's cutoff quorum before the depth-two fallback
        // exists. Transaction 1 must still keep the commit pending at this point.
        tokio::time::timeout(Duration::from_secs(5), async {
            while indirect_rejects.get() == rejects_before_recovery {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Recovery must reestablish the partial rejection at depth one");
        assert_eq!(indirect_rejects.get(), rejects_before_recovery + 1);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        observer
            .handle_committed_leaders(vec![leaders[2].clone()], true)
            .unwrap();

        for expected_commit in &expected {
            let replayed = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                .await
                .expect("Recovery must finalize after the depth-two anchor")
                .unwrap();
            assert_eq!(replayed.commit_ref, expected_commit.commit_ref);
            assert_eq!(
                replayed.rejected_transactions_by_block,
                expected_commit.rejected_transactions_by_block
            );
        }
        observer.stop().await;
        assert_eq!(
            store.read_rejected_transactions(first_commit_ref).unwrap(),
            Some(expected[0].rejected_transactions_by_block.clone())
        );
        assert_eq!(
            store.read_last_finalized_commit().unwrap(),
            Some(expected.last().unwrap().commit_ref)
        );
    }

    #[tokio::test]
    async fn recovery_preserves_persisted_cutoff_rejection_without_local_votes() {
        let mut fixture = Fixture::with_gc_depth(3);
        let (target, round_one_refs) = fixture.make_round_one(1);
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
        let first_voter_ref = voters[0].reference();
        let mut previous_round = voters;
        let mut leaders = vec![];
        for round in 3..=5 {
            let ancestors: Vec<_> = previous_round
                .iter()
                .map(|block| block.reference())
                .collect();
            let blocks: Vec<_> = (0..5)
                .map(|author| fixture.make_graph_block(round, author, ancestors.clone(), vec![], 0))
                .collect();
            fixture.add_blocks(&blocks);
            leaders.push(blocks[0].clone());
            previous_round = blocks;
        }
        let mut linearizer =
            crate::linearizer::Linearizer::new(fixture.context.clone(), fixture.dag_state.clone());
        let mut expected = vec![];
        for leader in leaders {
            let commit = linearizer.handle_commit(vec![leader]).pop().unwrap();
            let finalized = fixture.finalizer.process_commit(commit);
            assert_eq!(finalized.len(), 1);
            persist_finalized_commits(
                &fixture.dag_state,
                &fixture.transaction_vote_tracker,
                &finalized,
                true,
            );
            expected.extend(finalized);
        }
        assert_eq!(
            expected[0]
                .rejected_transactions_by_block
                .get(&target.reference()),
            Some(&vec![0])
        );
        let context = fixture.context.clone();
        let store = fixture.store.clone();
        drop(linearizer);
        drop(fixture);

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        assert_eq!(dag_state.read().gc_round(), first_voter_ref.round);
        assert!(
            dag_state
                .read()
                .get_block_children(&first_voter_ref)
                .is_none()
        );
        let tracker = TransactionVoteTracker::new(
            context.clone(),
            Arc::new(NoopBlockVerifier),
            dag_state.clone(),
        );
        let (consumer, mut receiver) = crate::commit_consumer::CommitConsumerArgs::new(0, 0);
        let mut observer = crate::commit_observer::CommitObserver::new(
            context,
            consumer,
            dag_state,
            tracker.clone(),
        )
        .await;

        // Real recovery loads the persisted rejection marker and bypasses voting. Neither the
        // local first-vote traversal nor the fresh tracker can reconstruct this old decision.
        assert!(tracker.get_reject_votes(&target.reference()).is_none());
        for expected_commit in &expected {
            let replayed = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                .await
                .expect("Persisted cutoff decisions must be replayed without voting")
                .unwrap();
            assert!(replayed.recovered_rejected_transactions);
            assert_eq!(replayed.commit_ref, expected_commit.commit_ref);
            assert_eq!(
                replayed.rejected_transactions_by_block,
                expected_commit.rejected_transactions_by_block
            );
        }
        observer.stop().await;
        assert_eq!(
            store
                .read_rejected_transactions(expected[0].commit_ref)
                .unwrap(),
            Some(expected[0].rejected_transactions_by_block.clone())
        );
        assert_eq!(
            store.read_last_finalized_commit().unwrap(),
            Some(expected.last().unwrap().commit_ref)
        );
    }
}

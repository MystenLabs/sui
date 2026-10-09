// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_common::ZipDebugEqIteratorExt;
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
// Limit the target bits and retained vote data for large commits.
const VOTE_TARGETS_PER_BATCH: usize = 256;

/// Finalizes whether transactions in committed sub-DAGs are accepted or rejected under Mysticeti v3.
///
/// Consensus determines committed blocks and transactions, and sends committed sub-DAGs in commit order.
/// The finalizer buffers these commits, determines each transaction's outcome from voting evidence,
/// and releases finalized commits in order.
///
/// For a given target block, a voting block is the first block on an authority chain whose causal
/// history includes the target. Later blocks on that chain do not vote again on the target.
/// A voting block rejects every transaction in the target if the target's round is at or below
/// its cutoff. Otherwise, it rejects the transactions listed explicitly and accepts the rest.
///
/// Direct finalization uses evidence in DagState and requires a quorum to accept or reject.
/// Indirect finalization uses evidence in committed blocks and accepts at the certification threshold
/// or rejects at quorum. Once a later commit's leader is at least two rounds ahead of the target's
/// commit leader, transactions in the target block without an eligible accept certificate are rejected.
///
/// Pending commits are retried when new commits or blocks arrive.
pub(crate) struct CommitFinalizerV3 {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    transaction_vote_tracker: TransactionVoteTracker,
    commit_sender: UnboundedSender<CommittedSubDag>,

    last_processed_commit: Option<CommitIndex>,
    pending_commits: VecDeque<CommitStateV3>,
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
                commit = receiver.recv() => {
                    let shutting_down = commit.is_none();
                    (commit, shutting_down)
                }
                Ok(()) = block_updates.changed(), if !self.pending_commits.is_empty() => {
                    (None, false)
                }
                _ = status_interval.tick(), if !self.pending_commits.is_empty() => {
                    // Update queue metrics without retrying finalization or consuming block updates.
                    self.report_pending_status();
                    continue;
                }
            };
            // A commit also retries pending work, so consume all preceding block notifications
            // before any kind of pass. Updates arriving during the pass remain unobserved and will trigger another pass,
            // avoiding lost wakeups.
            block_updates.borrow_and_update();

            let _scope = monitored_scope("CommitFinalizer::loop");

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

            if !finalized_commits.is_empty() {
                let persist_started = Instant::now();
                persist_finalized_commits(
                    &self.dag_state,
                    &self.transaction_vote_tracker,
                    &finalized_commits,
                    !already_finalized,
                );
                let elapsed = persist_started.elapsed();
                self.context
                    .metrics
                    .node_metrics
                    .finalizer_v3_phase_duration_seconds
                    .with_label_values(&["persist"])
                    .observe(elapsed.as_secs_f64());
                if elapsed >= SLOW_FINALIZATION {
                    tracing::debug!(
                        first_commit = finalized_commits.first().unwrap().commit_ref.index,
                        last_commit = finalized_commits.last().unwrap().commit_ref.index,
                        persist_ms = elapsed.as_secs_f64() * 1000.0,
                        "V3 finalizer storage write was slow"
                    );
                }
                for commit in finalized_commits {
                    if let Err(error) = self.commit_sender.send(commit) {
                        tracing::warn!(
                            "Failed to send to commit handler, probably due to shutdown: {error:?}"
                        );
                        return;
                    }
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
        let _scope = monitored_scope("CommitFinalizer::process_commit");
        if let Some(last_processed_commit) = self.last_processed_commit {
            assert_eq!(
                last_processed_commit + 1,
                committed_sub_dag.commit_ref.index
            );
        }
        self.last_processed_commit = Some(committed_sub_dag.commit_ref.index);
        let commit_state = CommitStateV3::new(committed_sub_dag);
        self.report_skipped_blocks_in_commit(&commit_state);
        self.pending_commits.push_back(commit_state);

        self.try_finalize_commits("commit")
    }

    // Reports blocks whose vote evidence is skipped during indirect finalization because their
    // round is at or below the commit's vote GC round.
    // These blocks usually come from slow validators.
    fn report_skipped_blocks_in_commit(&self, commit_state: &CommitStateV3) {
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
                "Block {block_ref} is at or below vote GC round {vote_evidence_gc_round}. Indirect finalization will not count its votes."
            );
        }
    }

    fn try_finalize_commits(&mut self, trigger: &'static str) -> Vec<CommittedSubDag> {
        self.last_trigger = trigger;
        self.context
            .metrics
            .node_metrics
            .finalizer_v3_attempts
            .with_label_values(&[trigger])
            .inc();
        let _timer = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["CommitFinalizer::try_finalize_commits"])
            .start_timer();

        // Run direct finalization pass. See comments above try_direct_finalize_commit() for details.
        {
            let _direct_timer = self
                .context
                .metrics
                .node_metrics
                .finalizer_v3_phase_duration_seconds
                .with_label_values(&["direct"])
                .start_timer();
            for index in 0..self.pending_commits.len() {
                self.try_direct_finalize_commit(index);
            }
        }

        let mut finalized_commits = self.pop_finalized_commits("direct");
        self.context
            .metrics
            .node_metrics
            .finalizer_output_commits
            .with_label_values(&["direct"])
            .inc_by(finalized_commits.len() as u64);

        // Run indirect finalization pass. See comments above try_indirect_finalize_first_commit() for details.
        {
            let _indirect_timer = self
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
                    self.try_indirect_finalize_first_commit(&committed_voting_graph);
                    let indirect_finalized_commits = self.pop_finalized_commits("indirect");
                    if indirect_finalized_commits.is_empty() {
                        // No future commits can be indirectly finalized at this point.
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
        }

        self.report_finalization_latency(&finalized_commits);
        self.context
            .metrics
            .node_metrics
            .finalizer_buffered_commits
            .set(self.pending_commits.len() as i64);
        self.report_pending_status();

        finalized_commits
    }

    /// Direct finalization applies these steps to every pending block B in a commit whose
    /// leader is at round L:
    ///
    /// 1. The validator traverses local descendants of B through round L + 1. DagState keeps
    ///    the direct child links for local blocks.
    /// 2. Each voting block (the first block on an authority chain whose causal history
    ///    includes B) casts a vote. If B's round is at or below a voting block's cutoff, that
    ///    block votes to reject every transaction in B. Otherwise, explicit rejects apply per
    ///    transaction and the rest are accepted. Later blocks on the same authority chain do
    ///    not vote again. Each side (accept vs reject) counts an authority once.
    /// 3. DagState only traverses B above the current GC round. All descendant blocks are newer
    ///    than B, so GC cannot hide a voting block while leaving B traversable.
    /// 4. The finalizer combines cutoff reject voters from the voting blocks with explicit
    ///    reject voters from the transaction vote tracker, counting each authority once even
    ///    if it appears in both sources.
    /// 5. Direct finalization accepts the transaction when accept stake reaches quorum, and
    ///    rejects it when reject stake reaches quorum. Otherwise, the transaction stays pending.
    fn try_direct_finalize_commit(&mut self, commit_index: usize) {
        self.pending_commits[commit_index].attempts += 1;
        let leader_round = self.pending_commits[commit_index].commit.leader.round;
        let last_voting_round = leader_round.saturating_add(1);
        let pending_transactions: Vec<_> = self.pending_commits[commit_index]
            .pending_transactions
            .iter()
            .map(|(block_ref, indices)| (*block_ref, indices.clone()))
            .collect();
        if pending_transactions.is_empty() {
            return;
        }
        for batch in pending_transactions.chunks(VOTE_TARGETS_PER_BATCH) {
            let mut votes_by_target = {
                let dag_state = self.dag_state.read();
                collect_voting_blocks_for_batch(
                    &*dag_state,
                    batch
                        .iter()
                        .map(|(target, _)| *target)
                        // Skip pruned or uncached targets: their child links may be unavailable.
                        .filter(|target| dag_state.get_block_children(target).is_some()),
                    last_voting_round,
                )
            };
            for (block_ref, transaction_indices) in batch {
                let voting_blocks = votes_by_target.remove(block_ref).unwrap_or_default();
                let explicit_reject_votes = self
                    .transaction_vote_tracker
                    .get_reject_vote_aggregators(block_ref)
                    .unwrap_or_else(|| {
                        panic!(
                            "No vote info found for {block_ref}. It is incorrectly GC'ed or failed to be recovered after crash."
                        )
                    });
                let decisions = self.compute_decisions::<QuorumThreshold>(
                    *block_ref,
                    transaction_indices,
                    &voting_blocks,
                    explicit_reject_votes,
                    false,
                );
                self.apply_decisions(
                    commit_index,
                    *block_ref,
                    decisions,
                    "direct_finalize",
                    "direct_reject",
                );
            }
        }
    }

    /// Indirect finalization checks the earliest pending commit once a later commit is available.
    /// The direct finalization pass has already applied any decisions supported by local evidence.
    ///
    /// For each pending block B in a commit whose leader is at round L, it selects committed voting
    /// blocks from B's descendants through round L + 1. It counts these voting blocks only when B's
    /// round is greater than L - gc_depth, so GC cannot hide a voting block and make a later block
    /// appear to cast that chain's vote.
    ///
    /// It accepts a transaction when committed accept stake reaches the certification threshold,
    /// and rejects it when committed reject stake reaches quorum. Reject votes can come from
    /// cutoffs or explicit rejects. Each side counts an authority once.
    ///
    /// When the newest commit's leader is at least INDIRECT_COMMIT_DEPTH rounds ahead of this
    /// commit's leader, any direct accept quorum must leave an accept certificate in the committed
    /// prefix. After checking for certificates, this method rejects every remaining pending
    /// transaction, allowing the caller to remove the finalized commit from the queue.
    fn try_indirect_finalize_first_commit(&mut self, committed_voting_graph: &CommittedBlockGraph) {
        let pending_transactions: Vec<_> = self.pending_commits[0]
            .pending_transactions
            .iter()
            .map(|(block_ref, indices)| (*block_ref, indices.clone()))
            .collect();
        let leader_round = self.pending_commits[0].commit.leader.round;
        let anchor_round = self.pending_commits.back().unwrap().commit.leader.round;
        let reject_remaining = leader_round.saturating_add(INDIRECT_COMMIT_DEPTH) <= anchor_round;
        let last_voting_round = leader_round.saturating_add(1);
        // This cutoff round excludes vote evidence for targets at or below it because of GC,
        // in addition to the per block cutoff.
        //
        // Let L be this commit's leader round. It must be finalized when the first later commit
        // whose leader is at least two rounds ahead is processed. The commit immediately before
        // that later commit has a leader at round L + 1 or earlier.
        //
        // The linearizer uses the previous commit's GC round when collecting blocks for the
        // later commit, so it can skip blocks through round L + 1 - gc_depth. Requiring the
        // target's round to be greater than L - gc_depth keeps its strictly newer voting blocks
        // above that GC round.
        //
        // A voting block's cutoff alone is not enough: if GC skips a voting block that rejected
        // a transaction, a later block on the same authority chain can appear to accept it.
        // That later block may have a cutoff below the target's round and need not repeat the
        // earlier explicit reject.
        let vote_evidence_gc_round =
            leader_round.saturating_sub(self.context.protocol_config.gc_depth());

        for batch in pending_transactions.chunks(VOTE_TARGETS_PER_BATCH) {
            // A voting block is a descendant of the target, so it cannot commit before the target.
            // For an eligible target, GC cannot remove any voting blocks.
            let mut votes_by_target = collect_voting_blocks_for_batch(
                committed_voting_graph,
                batch
                    .iter()
                    .map(|(target, _)| *target)
                    .filter(|target| target.round > vote_evidence_gc_round),
                last_voting_round,
            );
            // Decide all blocks in the batch. Even GC-filtered target blocks still get a decision if the
            // indirect finalization depth limit is reached.
            for (block_ref, transaction_indices) in batch {
                let voting_blocks = votes_by_target.remove(block_ref).unwrap_or_default();
                let decisions = self.compute_decisions::<CertificationThreshold>(
                    *block_ref,
                    transaction_indices,
                    &voting_blocks,
                    BTreeMap::new(),
                    reject_remaining,
                );
                self.apply_decisions(
                    0,
                    *block_ref,
                    decisions,
                    "indirect_finalize",
                    "indirect_reject",
                );
            }
        }
    }

    fn compute_decisions<T: CommitteeThreshold>(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        voting_blocks: &[Arc<VotingBlock>],
        additional_reject_votes: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
        reject_remaining: bool,
    ) -> TransactionDecisions {
        let accept_votes: TransactionVotes<T> =
            self.collect_accept_votes(block_ref, transaction_indices, voting_blocks);
        // Rejection needs a full quorum: Q + C > N + f ensures it intersects any accept quorum or
        // committed accept certificate. Crash faults cannot produce conflicting votes.
        let reject_votes = self.collect_reject_votes(
            block_ref,
            transaction_indices,
            voting_blocks,
            additional_reject_votes,
        );

        let mut decisions = TransactionDecisions::default();
        for transaction_index in transaction_indices {
            let transaction_accept_votes = accept_votes.for_transaction(*transaction_index);
            let accepted = transaction_accept_votes.reached_threshold(&self.context.committee);
            let transaction_reject_votes = reject_votes.for_transaction(*transaction_index);
            let rejected = transaction_reject_votes.reached_threshold(&self.context.committee);
            assert!(
                !(accepted && rejected),
                "Transaction {} in block {} cannot meet both acceptance and rejection thresholds. Accept voters: {:?}, reject voters: {:?}",
                transaction_index,
                block_ref,
                transaction_accept_votes.authorities(),
                transaction_reject_votes.authorities(),
            );
            if accepted {
                decisions.accepted.push(*transaction_index);
            } else if rejected || reject_remaining {
                // A reject quorum decides immediately. During indirect finalization, once the newest
                // commit's leader is at least two rounds ahead of this commit's leader, rejection is
                // also safe without a reject quorum if no eligible accept certificate is found: any
                // direct accept quorum must leave an accept certificate in the committed prefix.
                decisions.rejected.push(*transaction_index);
            }
        }
        decisions
    }

    fn collect_accept_votes<T: CommitteeThreshold>(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
        voting_blocks: &[Arc<VotingBlock>],
    ) -> TransactionVotes<T> {
        // Most transactions have no explicit reject. Use one shared aggregator for them, and
        // create transaction-specific aggregators only when a voting block contains a reject.
        let mut shared = StakeAggregator::<T>::new();
        let mut transactions_with_explicit_rejects = BTreeSet::new();
        for voting_block in voting_blocks {
            if voting_block.rejects_by_cutoff(block_ref) {
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
        for voting_block in voting_blocks {
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
        voting_blocks: &[Arc<VotingBlock>],
        mut by_transaction: BTreeMap<TransactionIndex, StakeAggregator<QuorumThreshold>>,
    ) -> TransactionVotes<QuorumThreshold> {
        by_transaction.retain(|index, _| transaction_indices.contains(index));
        // A cutoff covering the target rejects every transaction, so these voters share one aggregator.
        let mut shared = StakeAggregator::new();
        for voting_block in voting_blocks {
            let author = voting_block.block_ref.author;
            if voting_block.rejects_by_cutoff(block_ref) {
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
        // Merge cutoff and explicit reject voters, counting each authority only once even
        // if it contributes both kinds of rejection.
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
                tracing::debug!(
                    commit_index = commit_state.commit.commit_ref.index,
                    leader_round = commit_state.commit.leader.round,
                    decided_with_local_blocks = commit_state.commit.decided_with_local_blocks,
                    release_path,
                    trigger = self.last_trigger,
                    attempts = commit_state.attempts,
                    decision_wait_ms = decision_wait.as_secs_f64() * 1000.0,
                    ordered_release_wait_ms = ordered_release_wait.as_secs_f64() * 1000.0,
                    total_wait_ms = total_wait.as_secs_f64() * 1000.0,
                    "Slow finalization in commit finalizer V3"
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

    fn report_pending_status(&self) {
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
        let oldest_pending_seconds = self
            .pending_commits
            .front()
            .map_or(0.0, |first| first.received_at.elapsed().as_secs_f64());
        metrics
            .finalizer_v3_oldest_pending_seconds
            .set(oldest_pending_seconds);
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

// Child links (reverse DAG) for committed blocks, used to find voting blocks for indirect finalization.
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

// Collect one batch at a time so callers can release its votes before collecting the next batch.
fn collect_voting_blocks_for_batch(
    graph: &impl ReverseBlockGraph,
    targets: impl Iterator<Item = BlockRef>,
    last_voting_round: Round,
) -> BTreeMap<BlockRef, Vec<Arc<VotingBlock>>> {
    let targets: Vec<_> = targets.collect();
    let votes = collect_voting_blocks_for_targets(graph, &targets, last_voting_round);
    targets.into_iter().zip_debug_eq(votes).collect()
}

/// Collects voting blocks for all targets in a single round-ordered pass.
/// Target blocks must be unique and within the batch limit. The caller must exclude pruned child links.
fn collect_voting_blocks_for_targets(
    graph: &impl ReverseBlockGraph,
    targets: &[BlockRef],
    last_voting_round: Round,
) -> Vec<Vec<Arc<VotingBlock>>> {
    // Target sets for a visited block.
    // Each field is a bitmap where bit i represents targets[i].
    struct TargetSets {
        // Targets reachable through ancestors, plus this block if it is a target.
        including_self: Vec<u64>,
        // Targets reachable through ancestors only, excluding this block itself.
        excluding_self: Vec<u64>,
    }

    assert!(targets.len() <= VOTE_TARGETS_PER_BATCH);
    let target_indices: BTreeMap<_, _> = targets
        .iter()
        .enumerate()
        .map(|(index, target)| (*target, index))
        .collect();
    // Use bitmaps packed into u64 words to track target reachability, since there can be many targets.
    const BITS_PER_WORD: usize = u64::BITS as usize;
    let word_count = targets.len().div_ceil(BITS_PER_WORD);
    let mut voting_blocks = vec![vec![]; targets.len()];
    let mut target_sets: BTreeMap<BlockRef, TargetSets> = BTreeMap::new();
    let mut to_visit: BTreeSet<_> = targets
        .iter()
        .copied()
        .filter(|target| target.round <= last_voting_round)
        .collect();
    while let Some(current_ref) = to_visit.pop_first() {
        let block = graph
            .block(&current_ref)
            .unwrap_or_else(|| panic!("No block data found for voting block {current_ref}"));
        // Bitmap packed into u64 words: bit i marks targets[i] reachable through any parent of the block.
        let mut inherited = vec![0u64; word_count];
        // Bitmap packed into u64 words: bit i marks targets[i] already in block's same-authority parent's
        // causal history, excluding that parent itself. In this case, the block should not vote for the target.
        let mut own_authority_history = vec![0u64; word_count];
        for ancestor in block.ancestors() {
            let Some(target_set) = target_sets.get(ancestor) else {
                continue;
            };
            for (word, ancestor_word) in inherited
                .iter_mut()
                .zip_debug_eq(&target_set.including_self)
            {
                // Merge targets reachable through this parent, including the parent if it is a target.
                *word |= ancestor_word;
            }
            if ancestor.author == current_ref.author {
                for (word, ancestor_word) in own_authority_history
                    .iter_mut()
                    .zip_debug_eq(&target_set.excluding_self)
                {
                    // Merge targets already voted on along this authority chain.
                    *word |= ancestor_word;
                }
            }
        }
        // A voting block sees the target through its parents, but its same-authority parents
        // do not see the target through their own ancestors. A target cannot vote on itself.
        let mut voting_block = None;
        for (word_index, (inherited_word, own_word)) in inherited
            .iter()
            .zip_debug_eq(&own_authority_history)
            .enumerate()
        {
            // Keep targets this block sees that its authority chain has not already voted on.
            let mut voters = inherited_word & !own_word;
            while voters != 0 {
                // Use the lowest set bit in `voters` to determine the next target index.
                let target_index = word_index * BITS_PER_WORD + voters.trailing_zeros() as usize;
                let vote =
                    voting_block.get_or_insert_with(|| Arc::new(VotingBlock::new(block.clone())));
                voting_blocks[target_index].push(vote.clone());
                // Clear the lowest set bit, which represents the target just processed.
                voters &= voters - 1;
            }
        }
        let mut including_self = inherited.clone();
        if let Some(index) = target_indices.get(&current_ref) {
            // Set this block's target bit. Division selects the word, modulo selects the bit.
            including_self[index / BITS_PER_WORD] |= 1 << (index % BITS_PER_WORD);
        }
        target_sets.insert(
            current_ref,
            TargetSets {
                including_self,
                excluding_self: inherited,
            },
        );
        // Do not traverse beyond the last voting round.
        if current_ref.round < last_voting_round {
            // Children blocks must be in later rounds. Round order ensures none has been visited yet.
            to_visit.extend(graph.children(&current_ref).into_iter().filter(|child| {
                assert!(child.round > current_ref.round);
                child.round <= last_voting_round
            }));
        }
    }
    voting_blocks
}

// A block's cutoff round and explicit rejects, used to determine its votes on target transactions.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
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

    fn rejects_by_cutoff(&self, block_ref: BlockRef) -> bool {
        block_ref.round <= self.cutoff_round
    }

    fn select_accepted_transactions(
        &self,
        block_ref: BlockRef,
        transaction_indices: &BTreeSet<TransactionIndex>,
    ) -> Vec<TransactionIndex> {
        if self.rejects_by_cutoff(block_ref) {
            return vec![];
        }
        let Some(rejects) = self.explicit_rejects.get(&block_ref) else {
            return transaction_indices.iter().copied().collect();
        };
        transaction_indices.difference(rejects).copied().collect()
    }
}

// Votes for transactions in one target block. If a transaction has a specific entry, it overrides the shared set.
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

// Accepted and rejected transaction indices for one target block.
#[derive(Default)]
struct TransactionDecisions {
    accepted: Vec<TransactionIndex>,
    rejected: Vec<TransactionIndex>,
}

// Tracks pending and rejected transactions, timing, and attempts for a commit awaiting release.
struct CommitStateV3 {
    commit: CommittedSubDag,
    pending_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    rejected_transactions: BTreeMap<BlockRef, BTreeSet<TransactionIndex>>,
    received_at: Instant,
    ready_at: Option<Instant>,
    attempts: u64,
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
    use std::ops::{Range, RangeInclusive};

    use consensus_config::{AuthorityIndex, Committee, Stake};
    use tokio::sync::mpsc::error::TryRecvError;

    use crate::{
        Transaction,
        block::{BlockTransactionVotes, TestBlock, genesis_blocks},
        block_verifier::NoopBlockVerifier,
        commit::{CommitDigest, CommitRef},
        commit_consumer::CommitConsumerArgs,
        commit_observer::CommitObserver,
        linearizer::Linearizer,
        storage::{Store, mem_store::MemStore},
    };

    use super::*;

    const COMMITTEE_SIZE: usize = 6;

    impl CommitFinalizerV3 {
        fn is_empty(&self) -> bool {
            self.pending_commits.is_empty()
        }
    }

    /// Accepted and rejected transactions.
    type Decided = (Vec<TransactionIndex>, Vec<TransactionIndex>);

    type MakeVotes = fn(&RoundOne) -> Vec<VerifiedBlock>;

    /// How a round-two block votes on the round-one target.
    #[derive(Clone)]
    enum Vote {
        Accept,
        Reject(Vec<TransactionIndex>),
        /// A cutoff that covers the target rejects all of its transactions.
        Cutoff,
        /// The block does not include the target in its causal history.
        Absent,
    }

    // Test environment for creating, committing, and finalizing blocks with in-memory storage.
    struct Fixture {
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        store: Arc<MemStore>,
        transaction_vote_tracker: TransactionVoteTracker,
        finalizer: CommitFinalizerV3,
        linearizer: Linearizer,
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
            let transaction_vote_tracker = noop_tracker(&context, &dag_state);
            let (commit_sender, _commit_receiver) = unbounded_channel("finalizer_v3_test");
            let finalizer = CommitFinalizerV3::new(
                context.clone(),
                dag_state.clone(),
                transaction_vote_tracker.clone(),
                commit_sender,
            );
            let linearizer = Linearizer::new(context.clone(), dag_state.clone());

            Self {
                context,
                dag_state,
                store,
                transaction_vote_tracker,
                finalizer,
                linearizer,
            }
        }

        /// Adds round-one blocks with a target from authority 0.
        fn round_one(&self, num_target_transactions: usize) -> RoundOne {
            RoundOne::new(&self.round_one_blocks(&[num_target_transactions]), 0)
        }

        fn round_one_blocks(&self, transaction_counts: &[usize]) -> Vec<VerifiedBlock> {
            let genesis = genesis_blocks(&self.context);
            self.add_full_round(1, &refs(&genesis), transaction_counts)
        }

        /// Adds a block from every authority that links to `ancestors`. Authority i proposes
        /// `transaction_counts[i]` transactions.
        fn add_full_round(
            &self,
            round: Round,
            ancestors: &[BlockRef],
            transaction_counts: &[usize],
        ) -> Vec<VerifiedBlock> {
            let blocks: Vec<_> = (0..self.context.committee.size() as u32)
                .map(|author| {
                    let count = transaction_counts
                        .get(author as usize)
                        .copied()
                        .unwrap_or_default();
                    VerifiedBlock::new_for_test(
                        test_block(round, author, ancestors.to_vec())
                            .set_transactions(vec![Transaction::new(vec![1]); count])
                            .build_v3(0),
                    )
                })
                .collect();
            self.add_blocks(&blocks);
            blocks
        }

        /// Adds blocks from authorities 0..5 in each round, linking all blocks of the previous
        /// round. Returns authority 0's block in each round.
        fn add_rounds(
            &self,
            previous: &[VerifiedBlock],
            rounds: RangeInclusive<Round>,
        ) -> Vec<VerifiedBlock> {
            let mut ancestors = refs(previous);
            let mut leaders = vec![];
            for round in rounds {
                let blocks: Vec<_> = (0..5)
                    .map(|author| block(round, author, ancestors.clone()))
                    .collect();
                self.add_blocks(&blocks);
                ancestors = refs(&blocks);
                leaders.push(blocks[0].clone());
            }
            leaders
        }

        fn add_blocks<'a>(&self, blocks: impl IntoIterator<Item = &'a VerifiedBlock>) {
            let blocks: Vec<_> = blocks.into_iter().cloned().collect();
            self.dag_state.write().accept_blocks(blocks.clone());
            self.transaction_vote_tracker
                .add_voted_blocks(blocks.into_iter().map(|block| (block, vec![])).collect());
        }

        fn linearize(&mut self, leader: &VerifiedBlock) -> CommittedSubDag {
            self.linearizer
                .handle_commit(vec![leader.clone()])
                .pop()
                .unwrap()
        }

        fn process<'a>(
            &mut self,
            index: CommitIndex,
            leader: &VerifiedBlock,
            blocks: impl IntoIterator<Item = &'a VerifiedBlock>,
        ) -> Vec<CommittedSubDag> {
            self.finalizer
                .process_commit(make_commit(index, leader, blocks))
        }

        fn process_leader(&mut self, leader: &VerifiedBlock) -> Vec<CommittedSubDag> {
            let commit = self.linearize(leader);
            self.finalizer.process_commit(commit)
        }

        fn pending(&self, block: &VerifiedBlock) -> Option<&BTreeSet<TransactionIndex>> {
            self.finalizer.pending_commits[0]
                .pending_transactions
                .get(&block.reference())
        }

        /// Returns the direct decisions from local voting blocks and the indirect decisions from
        /// voting blocks among `committed`, on every transaction in `target`.
        fn decide(
            &self,
            target: &VerifiedBlock,
            committed: &[VerifiedBlock],
            last_voting_round: Round,
        ) -> [Decided; 2] {
            let target_ref = target.reference();
            let transactions: BTreeSet<_> =
                (0..target.transactions().len() as TransactionIndex).collect();
            let local_votes = collect_voting_blocks_for_targets(
                &*self.dag_state.read(),
                &[target_ref],
                last_voting_round,
            )
            .pop()
            .unwrap();
            let graph = CommittedBlockGraph::new(
                committed
                    .iter()
                    .cloned()
                    .chain(std::iter::once(target.clone())),
            );
            let committed_votes =
                collect_voting_blocks_for_targets(&graph, &[target_ref], last_voting_round)
                    .pop()
                    .unwrap();
            let direct = self.finalizer.compute_decisions::<QuorumThreshold>(
                target_ref,
                &transactions,
                &local_votes,
                self.transaction_vote_tracker
                    .get_reject_vote_aggregators(&target_ref)
                    .unwrap(),
                false,
            );
            let indirect = self.finalizer.compute_decisions::<CertificationThreshold>(
                target_ref,
                &transactions,
                &committed_votes,
                BTreeMap::new(),
                false,
            );
            [
                (direct.accepted, direct.rejected),
                (indirect.accepted, indirect.rejected),
            ]
        }

        fn start_handle(
            &self,
            tracker: TransactionVoteTracker,
        ) -> (CommitFinalizerHandle, UnboundedReceiver<CommittedSubDag>) {
            let (commit_sender, commit_receiver) = unbounded_channel("finalizer_v3_output_test");
            let handle = CommitFinalizerHandle::start(
                self.context.clone(),
                self.dag_state.clone(),
                tracker,
                commit_sender,
            );
            (handle, commit_receiver)
        }

        /// Drops all in-memory state, as in a crash, and reloads the DAG from the store.
        fn restart(self) -> (Arc<Context>, Arc<MemStore>, Arc<RwLock<DagState>>) {
            let context = self.context.clone();
            let store = self.store.clone();
            drop(self);
            let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
            (context, store, dag_state)
        }
    }

    /// Round-one blocks, one of which is the target whose transactions are finalized.
    struct RoundOne {
        target: VerifiedBlock,
        refs: Vec<BlockRef>,
    }

    impl RoundOne {
        fn new(blocks: &[VerifiedBlock], target: usize) -> Self {
            Self {
                target: blocks[target].clone(),
                refs: refs(blocks),
            }
        }

        fn unlinked_refs(&self) -> Vec<BlockRef> {
            let target_ref = self.target.reference();
            self.refs
                .iter()
                .copied()
                .filter(|block_ref| *block_ref != target_ref)
                .collect()
        }

        fn vote(&self, author: u32, vote: Vote) -> VerifiedBlock {
            self.build(author, vote, vec![])
        }

        fn votes(&self, authors: Range<u32>, vote: Vote) -> Vec<VerifiedBlock> {
            authors
                .map(|author| self.vote(author, vote.clone()))
                .collect()
        }

        /// A `marker` transaction makes the block differ from the author's other round-two blocks.
        fn fork(&self, author: u32, vote: Vote, marker: u8) -> VerifiedBlock {
            self.build(author, vote, vec![Transaction::new(vec![marker])])
        }

        fn build(&self, author: u32, vote: Vote, transactions: Vec<Transaction>) -> VerifiedBlock {
            let (ancestors, rejects, cutoff_round) = match vote {
                Vote::Accept => (self.refs.clone(), vec![], 0),
                Vote::Reject(rejects) => (self.refs.clone(), rejects, 0),
                Vote::Cutoff => (self.refs.clone(), vec![], self.target.round()),
                Vote::Absent => (self.unlinked_refs(), vec![], 0),
            };
            let transaction_votes = if rejects.is_empty() {
                vec![]
            } else {
                vec![reject_votes(&self.target, rejects)]
            };
            VerifiedBlock::new_for_test(
                test_block(2, author, ancestors)
                    .set_transactions(transactions)
                    .set_transaction_votes(transaction_votes)
                    .build_v3(cutoff_round),
            )
        }
    }

    /// Orders the author's own ancestor first, as a proposer does.
    fn test_block(round: Round, author: u32, mut ancestors: Vec<BlockRef>) -> TestBlock {
        let own_authority = AuthorityIndex::new_for_test(author);
        ancestors.sort_by_key(|block_ref| block_ref.author != own_authority);
        TestBlock::new(round, author).set_ancestors(ancestors)
    }

    fn block(round: Round, author: u32, ancestors: Vec<BlockRef>) -> VerifiedBlock {
        vote_block(round, author, ancestors, vec![], 0)
    }

    fn vote_block(
        round: Round,
        author: u32,
        ancestors: Vec<BlockRef>,
        transaction_votes: Vec<BlockTransactionVotes>,
        cutoff_round: Round,
    ) -> VerifiedBlock {
        VerifiedBlock::new_for_test(
            test_block(round, author, ancestors)
                .set_transaction_votes(transaction_votes)
                .build_v3(cutoff_round),
        )
    }

    fn reject_votes(
        target: &VerifiedBlock,
        rejects: Vec<TransactionIndex>,
    ) -> BlockTransactionVotes {
        BlockTransactionVotes {
            block_ref: target.reference(),
            rejects,
        }
    }

    fn noop_tracker(
        context: &Arc<Context>,
        dag_state: &Arc<RwLock<DagState>>,
    ) -> TransactionVoteTracker {
        TransactionVoteTracker::new(
            context.clone(),
            Arc::new(NoopBlockVerifier),
            dag_state.clone(),
        )
    }

    fn refs(blocks: &[VerifiedBlock]) -> Vec<BlockRef> {
        blocks.iter().map(|block| block.reference()).collect()
    }

    fn make_commit<'a>(
        index: CommitIndex,
        leader: &VerifiedBlock,
        blocks: impl IntoIterator<Item = &'a VerifiedBlock>,
    ) -> CommittedSubDag {
        CommittedSubDag::new(
            leader.reference(),
            blocks.into_iter().cloned().collect(),
            0,
            CommitRef::new(index, CommitDigest::default()),
        )
    }

    fn rejected<'a>(
        commit: &'a CommittedSubDag,
        block: &VerifiedBlock,
    ) -> Option<&'a Vec<TransactionIndex>> {
        commit
            .rejected_transactions_by_block
            .get(&block.reference())
    }

    /// Returns the accept stake on transaction 0 and whether it forms a certificate.
    fn accept_certificate(
        finalizer: &CommitFinalizerV3,
        target: BlockRef,
        voting_blocks: &[Arc<VotingBlock>],
    ) -> (Stake, bool) {
        let votes: TransactionVotes<CertificationThreshold> =
            finalizer.collect_accept_votes(target, &BTreeSet::from([0]), voting_blocks);
        let votes = votes.for_transaction(0);
        (
            votes.stake(),
            votes.reached_threshold(&finalizer.context.committee),
        )
    }

    // Independent per-target reference for checking the batched traversal in tests.
    fn collect_voting_blocks(
        graph: &impl ReverseBlockGraph,
        block_ref: BlockRef,
        last_voting_round: Round,
    ) -> Vec<Arc<VotingBlock>> {
        let mut to_visit: BTreeSet<_> = graph
            .children(&block_ref)
            .into_iter()
            .filter(|child| child.round <= last_voting_round)
            .collect();
        let mut visited = BTreeSet::new();
        let mut ignored = BTreeSet::new();
        let mut voting_blocks = vec![];

        // BlockRef ordering visits lower rounds first. A reachable block votes unless an earlier block
        // on its own-authority chain has voted. The traversal still follows ignored blocks because
        // their descendants from other authorities can be voting blocks for the target.
        while let Some(current_ref) = to_visit.pop_first() {
            if current_ref.round > last_voting_round || !visited.insert(current_ref) {
                continue;
            }
            if !ignored.contains(&current_ref) {
                let current_block = graph.block(&current_ref).unwrap_or_else(|| {
                    panic!("No block data found for voting block {current_ref}")
                });
                voting_blocks.push(Arc::new(VotingBlock::new(current_block)));
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

        voting_blocks
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

    /// Counts child reads of blocks at or above `counted_round`.
    struct CountingGraph<'a> {
        dag: &'a DagState,
        counted_round: Round,
        reads: std::cell::Cell<usize>,
    }

    impl<'a> CountingGraph<'a> {
        fn new(dag: &'a DagState, counted_round: Round) -> Self {
            Self {
                dag,
                counted_round,
                reads: Default::default(),
            }
        }
    }

    impl ReverseBlockGraph for CountingGraph<'_> {
        fn block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock> {
            self.dag.get_block(block_ref)
        }

        fn children(&self, block_ref: &BlockRef) -> Vec<BlockRef> {
            if block_ref.round >= self.counted_round {
                self.reads.set(self.reads.get() + 1);
            }
            self.dag.get_block_children(block_ref).unwrap_or_default()
        }
    }

    async fn recv(receiver: &mut UnboundedReceiver<CommittedSubDag>) -> CommittedSubDag {
        tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .expect("Timed out waiting for a finalized commit")
            .expect("The output channel must stay open")
    }

    async fn wait_until(message: &str, mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect(message);
    }

    async fn start_observer(
        context: &Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        tracker: &TransactionVoteTracker,
    ) -> (CommitObserver, UnboundedReceiver<CommittedSubDag>) {
        let (consumer, receiver) = CommitConsumerArgs::new(0, 0);
        let observer =
            CommitObserver::new(context.clone(), consumer, dag_state, tracker.clone()).await;
        (observer, receiver)
    }

    /// Checks that recovery replays and persists `expected`, and returns the replayed commits.
    async fn assert_replayed(
        receiver: &mut UnboundedReceiver<CommittedSubDag>,
        mut observer: CommitObserver,
        store: &MemStore,
        expected: &[CommittedSubDag],
    ) -> Vec<CommittedSubDag> {
        let mut replayed = vec![];
        for expected_commit in expected {
            let commit = recv(receiver).await;
            assert_eq!(commit.commit_ref, expected_commit.commit_ref);
            assert_eq!(
                commit.rejected_transactions_by_block,
                expected_commit.rejected_transactions_by_block
            );
            replayed.push(commit);
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
        replayed
    }

    #[tokio::test]
    async fn voting_block_traversals_stay_in_window_and_agree() {
        let fixture = Fixture::with_fault_budget(127, 21, 0, |_| {});
        let mut rounds = vec![fixture.round_one_blocks(&[])];
        for round in 2..=4 {
            let parents = refs(rounds.last().unwrap());
            rounds.push(fixture.add_full_round(round, &parents, &[]));
        }
        let dag = fixture.dag_state.read();

        // Per-target traversal must not read children at or beyond the last voting round.
        for last_voting_round in [2, 3] {
            let graph = CountingGraph::new(&dag, last_voting_round);
            for target in &rounds[0] {
                let votes = collect_voting_blocks(&graph, target.reference(), last_voting_round);
                assert_eq!(
                    votes.iter().map(|vote| vote.block_ref).collect::<Vec<_>>(),
                    refs(&rounds[1]),
                );
            }
            assert_eq!(graph.reads.get(), 0);
        }

        // The shared traversal reads each child list in rounds one and two once.
        let targets = refs(&[rounds[0].clone(), rounds[1].clone()].concat());
        let graph = CountingGraph::new(&dag, 0);
        let expected: Vec<Vec<_>> = targets
            .iter()
            .map(|target| collect_voting_blocks(&graph, *target, 3))
            .collect();
        let per_target_reads = graph.reads.replace(0);
        assert_eq!(
            collect_voting_blocks_for_targets(&graph, &targets, 3),
            expected
        );
        assert_eq!(graph.reads.get(), 254);
        assert!(graph.reads.get() < per_target_reads);

        use rand::{Rng, SeedableRng, rngs::StdRng};
        let fixture = Fixture::new();
        let mut rng = StdRng::seed_from_u64(0xF1_2A_11_2E);
        for case in 0..64 {
            let genesis = genesis_blocks(&fixture.context);
            let mut history: Vec<Vec<_>> = genesis
                .iter()
                .map(|block| vec![block.reference()])
                .collect();
            let mut blocks = vec![];
            for round in 1..=7 {
                let mut round_blocks = vec![];
                for author in 0..COMMITTEE_SIZE {
                    let fork_count = if rng.gen_bool(0.25) { 2 } else { 1 };
                    for fork in 0..fork_count {
                        let mut ancestors =
                            vec![history[author][rng.gen_range(0..history[author].len())]];
                        for (other_author, own_blocks) in history.iter().enumerate() {
                            if other_author != author && rng.gen_bool(0.7) {
                                ancestors.push(own_blocks[rng.gen_range(0..own_blocks.len())]);
                            }
                        }
                        let transaction_votes = ancestors
                            .iter()
                            .find(|ancestor| ancestor.round > 0)
                            .filter(|_| rng.gen_bool(0.3))
                            .map(|ancestor| BlockTransactionVotes {
                                block_ref: *ancestor,
                                rejects: vec![0],
                            })
                            .into_iter()
                            .collect();
                        round_blocks.push(VerifiedBlock::new_for_test(
                            TestBlock::new(round, author as u32)
                                .set_ancestors(ancestors)
                                .set_transaction_votes(transaction_votes)
                                .set_transactions(vec![Transaction::new(vec![fork])])
                                .build_v3(rng.gen_range(0..round)),
                        ));
                    }
                }
                for block in &round_blocks {
                    history[block.author().value()].push(block.reference());
                }
                blocks.extend(round_blocks);
            }
            let gc_round = rng.gen_range(0..=2);
            blocks.retain(|block| block.round() > gc_round);
            let targets: Vec<_> = blocks
                .iter()
                .filter(|_| rng.gen_bool(0.4))
                .map(|block| block.reference())
                .collect();
            let graph = CommittedBlockGraph::new(blocks);
            for last_voting_round in 2..=8 {
                let expected: Vec<Vec<_>> = targets
                    .iter()
                    .map(|target| collect_voting_blocks(&graph, *target, last_voting_round))
                    .collect();
                assert_eq!(
                    collect_voting_blocks_for_targets(&graph, &targets, last_voting_round),
                    expected,
                    "case {case}, GC round {gc_round}, voting round {last_voting_round}",
                );
            }
        }
    }

    #[tokio::test]
    async fn shared_direct_votes_use_bounded_batches() {
        let mut fixture = Fixture::with_fault_budget(127, 21, 0, |_| {});
        let mut blocks = fixture.round_one_blocks(&[1; 127]);
        let mut leader = None;
        for round in 2..=5 {
            blocks = fixture.add_full_round(round, &refs(&blocks), &[1; 127]);
            if round == 4 {
                leader = Some(blocks[0].clone());
            }
        }
        let commit = fixture.linearize(&leader.unwrap());
        let target_count = commit.blocks.len();
        assert!(target_count > VOTE_TARGETS_PER_BATCH);
        let finalized = fixture.finalizer.process_commit(commit);
        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert_eq!(
            fixture
                .context
                .metrics
                .node_metrics
                .finalizer_transaction_status
                .with_label_values(&["direct_finalize"])
                .get(),
            target_count as u64,
        );
    }

    #[tokio::test]
    async fn indirect_batches_accept_eligible_targets_and_reject_gc_filtered_targets() {
        let mut fixture = Fixture::with_fault_budget(127, 21, 0, |context| {
            context.protocol_config.set_gc_depth_for_testing(4);
        });
        let mut rounds = vec![fixture.round_one_blocks(&[1; 127])];
        for round in 2..=8 {
            let transaction_counts: &[usize] = if round <= 4 { &[1; 127] } else { &[] };
            rounds.push(fixture.add_full_round(
                round,
                &refs(rounds.last().unwrap()),
                transaction_counts,
            ));
        }
        let first_commit = fixture.linearize(&rounds[4][0]);
        let vote_evidence_gc_round = first_commit.leader.round - 4;
        let targets: Vec<_> = first_commit
            .blocks
            .iter()
            .filter(|block| !block.transactions().is_empty())
            .map(|block| block.reference())
            .collect();
        let eligible_count = targets
            .iter()
            .filter(|target| target.round > vote_evidence_gc_round)
            .count();
        let filtered_count = targets.len() - eligible_count;
        assert_eq!(filtered_count, 127);
        assert!(eligible_count > VOTE_TARGETS_PER_BATCH);

        // Consensus advances while the finalizer is delayed. Local GC hides every target's
        // child links, so only committed evidence can accept the eligible targets.
        let later_commit = fixture.linearize(&rounds[7][0]);
        assert!(targets.iter().all(|target| {
            fixture
                .dag_state
                .read()
                .get_block_children(target)
                .is_none()
        }));
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());
        assert_eq!(
            fixture.finalizer.pending_commits[0]
                .pending_transactions
                .len(),
            targets.len()
        );
        let finalized = fixture.finalizer.process_commit(later_commit);

        assert_eq!(finalized.len(), 2);
        let rejects = &finalized[0].rejected_transactions_by_block;
        assert_eq!(rejects.len(), filtered_count);
        for target in targets {
            if target.round <= vote_evidence_gc_round {
                // Filtering out vote evidence must not skip the depth-based rejection.
                assert_eq!(rejects.get(&target), Some(&vec![0]));
            } else {
                // An accept certificate takes precedence over the depth-based fallback.
                assert!(!rejects.contains_key(&target));
            }
        }
        let statuses = &fixture
            .context
            .metrics
            .node_metrics
            .finalizer_transaction_status;
        for (label, expected) in [
            ("direct_finalize", 0),
            ("direct_reject", 0),
            ("indirect_finalize", eligible_count),
            ("indirect_reject", filtered_count),
        ] {
            assert_eq!(
                statuses.with_label_values(&[label]).get(),
                expected as u64,
                "{label}"
            );
        }
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn voting_block_decisions() {
        // Each case gives the target's transaction count, the round-two voting blocks, and the
        // expected (accepted, rejected) transactions under the direct and indirect rules. With
        // Q = 5 and C = 3, the direct rule accepts at Q and the indirect rule accepts at C.
        let cases: [(&str, usize, MakeVotes, Decided, Decided); 10] = [
            (
                "explicit reject quorum",
                2,
                |r| r.votes(0..5, Vote::Reject(vec![1])),
                (vec![0], vec![1]),
                (vec![0], vec![1]),
            ),
            (
                "explicit rejects below quorum",
                1,
                |r| {
                    let mut votes = r.votes(0..5, Vote::Accept);
                    votes.push(r.fork(4, Vote::Reject(vec![0]), 4));
                    votes.push(r.vote(5, Vote::Reject(vec![0])));
                    votes
                },
                (vec![0], vec![]),
                (vec![0], vec![]),
            ),
            (
                "cutoff quorum rejects every transaction",
                2,
                |r| r.votes(0..5, Vote::Cutoff),
                (vec![], vec![0, 1]),
                (vec![], vec![0, 1]),
            ),
            (
                "next-round blocks that do not link the target do not vote",
                1,
                |r| r.votes(1..6, Vote::Absent),
                (vec![], vec![]),
                (vec![], vec![]),
            ),
            (
                "an equivocating accept voter counts once",
                1,
                |r| {
                    [
                        r.votes(0..4, Vote::Accept),
                        vec![r.fork(1, Vote::Accept, 9)],
                    ]
                    .concat()
                },
                (vec![], vec![]),
                (vec![0], vec![]),
            ),
            (
                // Both transactions have two cutoff rejects. Explicit rejects bring only
                // transaction 0 to quorum; transaction 1 has three rejects and two accepts.
                "cutoff and explicit rejects combine per transaction",
                2,
                |r| {
                    [
                        r.votes(0..2, Vote::Cutoff),
                        vec![r.vote(2, Vote::Reject(vec![0, 1]))],
                        r.votes(3..5, Vote::Reject(vec![0])),
                    ]
                    .concat()
                },
                (vec![], vec![0]),
                (vec![], vec![0]),
            ),
            (
                // Authority 2's cutoff and explicit reject are two branches of the same vote.
                "an equivocator's cutoff and explicit reject count once",
                1,
                |r| {
                    [
                        r.votes(0..3, Vote::Cutoff),
                        vec![r.fork(2, Vote::Reject(vec![0]), 2)],
                        r.votes(3..4, Vote::Reject(vec![0])),
                    ]
                    .concat()
                },
                (vec![], vec![]),
                (vec![], vec![]),
            ),
            (
                "a fifth distinct reject voter reaches quorum with an equivocator",
                1,
                |r| {
                    [
                        r.votes(0..3, Vote::Cutoff),
                        vec![r.fork(2, Vote::Reject(vec![0]), 2)],
                        r.votes(3..5, Vote::Reject(vec![0])),
                    ]
                    .concat()
                },
                (vec![], vec![0]),
                (vec![], vec![0]),
            ),
            (
                "high cutoffs without a causal link do not reject",
                1,
                |r| {
                    (1..6)
                        .map(|author| vote_block(2, author, r.unlinked_refs(), vec![], 1))
                        .collect()
                },
                (vec![], vec![]),
                (vec![], vec![]),
            ),
            (
                // Round-three cutoffs are later blocks on authority chains that already voted.
                "later cutoffs cannot retract first accept votes",
                1,
                |r| {
                    let accepts = r.votes(0..5, Vote::Accept);
                    let cutoffs: Vec<_> = (0..5)
                        .map(|author| vote_block(3, author, refs(&accepts), vec![], 1))
                        .collect();
                    [accepts, cutoffs].concat()
                },
                (vec![0], vec![]),
                (vec![0], vec![]),
            ),
        ];
        for (name, num_transactions, make_votes, direct, indirect) in cases {
            let fixture = Fixture::new();
            let round_one = fixture.round_one(num_transactions);
            let voters = make_votes(&round_one);
            fixture.add_blocks(&voters);
            assert_eq!(
                fixture.decide(&round_one.target, &voters, 3),
                [direct, indirect],
                "{name}"
            );
        }
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
                    let byzantine_author = active_authorities as u32 - 1;
                    let fixture =
                        Fixture::with_fault_budget(size, byzantine_stake, crash_stake, |context| {
                            if weighted {
                                let mut authorities =
                                    context.committee.authorities_slice().to_vec();
                                authorities[byzantine_author as usize].stake = 2;
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
                    let round_one =
                        RoundOne::new(&fixture.round_one_blocks(&transaction_counts), size - 1);
                    let faulty_authorities = u32::from(byzantine_stake > 0);
                    let honest_accepts = if reject_quorum {
                        // Honest rejects plus Byzantine stake reach exactly Q.
                        committee.total_stake() - crash_stake - quorum
                    } else {
                        // Honest accepts plus Byzantine stake reach exactly C.
                        certificate - byzantine_stake
                    } as u32;
                    let mut voters: Vec<_> = (0..active_authorities as u32 - faulty_authorities)
                        .map(|author| {
                            let vote = if author < honest_accepts {
                                Vote::Accept
                            } else if author % 2 == 0 {
                                Vote::Reject(vec![0])
                            } else {
                                Vote::Cutoff
                            };
                            round_one.vote(author, vote)
                        })
                        .collect();
                    // The crashed authority produced round one, then stopped before voting.
                    // The Byzantine authority can send an accept, rejects, or both to an observer.
                    if byzantine_stake > 0 && view != 1 {
                        voters.push(round_one.vote(byzantine_author, Vote::Accept));
                    }
                    if byzantine_stake > 0 && view != 0 {
                        for marker in 1..=3 {
                            voters.push(round_one.fork(byzantine_author, Vote::Cutoff, marker));
                        }
                        voters.push(round_one.fork(byzantine_author, Vote::Reject(vec![0]), 4));
                    }
                    fixture.add_blocks(&voters);
                    let [direct, indirect] = fixture.decide(&round_one.target, &voters, 2);
                    let should_reject = reject_quorum && (byzantine_stake == 0 || view != 0);
                    let should_accept = !reject_quorum && (byzantine_stake == 0 || view != 1);
                    let rejects = if should_reject { vec![0] } else { vec![] };
                    let accepts = if should_accept { vec![0] } else { vec![] };
                    assert_eq!(direct, (vec![], rejects.clone()));
                    assert_eq!(indirect, (accepts, rejects));
                }
            }
        }
    }

    #[tokio::test]
    #[should_panic(expected = "cannot meet both acceptance and rejection thresholds")]
    async fn direct_detects_more_than_f_equivocating_stake() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        let rejects: Vec<_> = (1..6)
            .map(|author| round_one.fork(author, Vote::Reject(vec![0]), author as u8))
            .collect();
        fixture.add_blocks(&[round_one.votes(0..5, Vote::Accept), rejects].concat());
        assert_eq!(
            fixture
                .transaction_vote_tracker
                .get_reject_votes(&target.reference()),
            Some(vec![(0, 5)])
        );

        fixture.process(1, target, [target]);
    }

    #[tokio::test]
    #[should_panic(expected = "cannot meet both acceptance and rejection thresholds")]
    async fn indirect_detects_conflicting_accept_and_cutoff_quorums() {
        let fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let cutoffs: Vec<_> = (1..6)
            .map(|author| round_one.fork(author, Vote::Cutoff, author as u8))
            .collect();
        // The votes stay out of the local DAG, so only the indirect rule sees them.
        let committed = [round_one.votes(0..3, Vote::Accept), cutoffs].concat();
        fixture.decide(&round_one.target, &committed, 2);
    }

    #[tokio::test]
    async fn direct_counts_voting_blocks_through_leader_round_plus_one() {
        for (last_vote_round, finalizes) in [(3, true), (4, false)] {
            let mut fixture = Fixture::new();
            let round_one = fixture.round_one(1);
            let target = &round_one.target;
            let leader = round_one.vote(0, Vote::Accept);
            let non_voters = round_one.votes(1..6, Vote::Absent);
            fixture.add_blocks([&leader]);
            fixture.add_blocks(&non_voters);

            // The other accepts first observe the target through weak links. Their round-two
            // parents do not include the target in their causal history.
            let weak_link = [refs(&non_voters), vec![target.reference()]].concat();
            let mut later_accepts: Vec<_> = (1..4)
                .map(|author| block(3, author, weak_link.clone()))
                .collect();
            later_accepts.push(block(
                last_vote_round,
                4,
                vec![non_voters[3].reference(), target.reference()],
            ));
            fixture.add_blocks(&later_accepts);

            // A synced commit uses the same local votes.
            let mut commit = make_commit(1, &leader, [target, &leader]);
            commit.decided_with_local_blocks = false;
            let finalized = fixture.finalizer.process_commit(commit);

            if finalizes {
                assert_eq!(finalized.len(), 1);
                assert!(finalized[0].rejected_transactions_by_block.is_empty());
            } else {
                assert!(finalized.is_empty());
                assert_eq!(fixture.pending(target), Some(&BTreeSet::from([0])));
            }
        }
    }

    #[tokio::test]
    async fn direct_ignores_later_blocks_on_the_same_authority_chain() {
        for vote in [Vote::Cutoff, Vote::Reject(vec![0])] {
            let mut fixture = Fixture::new();
            let round_one = fixture.round_one(1);
            let target = &round_one.target;
            let accepts = round_one.votes(0..4, Vote::Accept);
            let voting_block = round_one.vote(4, vote);
            fixture.add_blocks(&accepts);
            fixture.add_blocks([&voting_block]);

            // The later block would accept the transaction if it could vote again. Its earlier
            // block has already consumed this authority chain's vote with a cutoff or a reject.
            fixture.add_blocks(&[block(3, 4, vec![voting_block.reference()])]);

            let leader = &accepts[0];
            assert!(fixture.process(1, leader, [target, leader]).is_empty());
            assert_eq!(fixture.pending(target), Some(&BTreeSet::from([0])));
        }
    }

    #[tokio::test]
    async fn direct_retries_when_new_local_votes_arrive() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        fixture.add_blocks(&round_one.votes(0..4, Vote::Accept));
        assert!(fixture.process(1, target, [target]).is_empty());

        let later_leader = round_one.vote(5, Vote::Absent);
        fixture.add_blocks([&round_one.vote(4, Vote::Accept), &later_leader]);

        // The second commit contains no accept certificate for the target. Only the retried direct
        // rule can use the new local vote and reach quorum.
        let finalized = fixture.process(2, &later_leader, [&later_leader]);

        assert_eq!(finalized.len(), 2);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn direct_votes_only_on_targets_above_local_gc_round() {
        let mut fixture = Fixture::with_gc_depth(3);
        let pruned_target = fixture.round_one(1);
        let mut rounds = vec![fixture.add_full_round(2, &pruned_target.refs, &[1])];
        for round in 3..=5 {
            let parents = refs(rounds.last().unwrap());
            rounds.push(fixture.add_full_round(round, &parents, &[]));
        }
        let target = &rounds[0][0];
        let commit = fixture.linearize(&rounds[2][0]);
        assert_eq!(fixture.dag_state.read().gc_round(), 1);
        assert_eq!(target.round(), fixture.dag_state.read().gc_round() + 1);

        // Linearization advances local GC before finalization. The round-two target and its
        // voting blocks remain above that cutoff, so the finalizer can still use this accept
        // quorum. The pruned target must not seed the shared traversal, and stays pending.
        assert!(fixture.finalizer.process_commit(commit).is_empty());

        assert_eq!(fixture.pending(target), None);
        assert_eq!(
            fixture.pending(&pruned_target.target),
            Some(&BTreeSet::from([0]))
        );
        assert!(
            fixture.finalizer.pending_commits[0]
                .rejected_transactions
                .is_empty()
        );
    }

    #[tokio::test]
    async fn direct_and_indirect_finalize_different_blocks_in_one_commit() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one_blocks(&[1, 1]);
        let (target_a, target_b) = (&round_one[0], &round_one[1]);
        let voters: Vec<_> = (0..COMMITTEE_SIZE as u32)
            .map(|author| {
                let mut transaction_votes = vec![];
                if author == 5 {
                    transaction_votes.push(reject_votes(target_a, vec![0]));
                }
                if author >= 3 {
                    transaction_votes.push(reject_votes(target_b, vec![0]));
                }
                vote_block(2, author, refs(&round_one), transaction_votes, 0)
            })
            .collect();
        fixture.add_blocks(&voters);

        assert!(
            fixture
                .process(1, target_a, [target_a, target_b])
                .is_empty()
        );
        assert_eq!(fixture.pending(target_a), None);
        assert_eq!(fixture.pending(target_b), Some(&BTreeSet::from([0])));

        let finalized = fixture.process(2, &voters[0], &voters);

        assert_eq!(finalized.len(), 2);
        assert!(finalized[0].rejected_transactions_by_block.is_empty());
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn indirect_accepts_only_with_an_accept_certificate() {
        for (num_accepts, expected_rejects) in [(3, None), (2, Some(vec![0]))] {
            let mut fixture = Fixture::new();
            let round_one = fixture.round_one(1);
            let target = &round_one.target;
            let accepts = round_one.votes(0..num_accepts, Vote::Accept);
            let non_voters = round_one.votes(num_accepts..5, Vote::Absent);
            let anchor = block(3, 0, refs(&[accepts.clone(), non_voters.clone()].concat()));
            fixture.add_blocks(&accepts);
            fixture.add_blocks(&non_voters);
            fixture.add_blocks([&anchor]);

            assert!(fixture.process(1, target, [target]).is_empty());
            // The non-voters can belong to earlier finalized commits. Only an accept certificate
            // must appear after the target block. The anchor is at depth two.
            let finalized = fixture.process(2, &anchor, accepts.iter().chain([&anchor]));

            assert_eq!(finalized.len(), 2);
            assert_eq!(rejected(&finalized[0], target).cloned(), expected_rejects);
        }
    }

    #[tokio::test]
    async fn indirect_accepts_mixed_round_voting_blocks_after_one_later_commit() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        let round_two_vote = round_one.vote(0, Vote::Accept);
        let round_three_vote = block(3, 1, vec![round_one.refs[1], round_two_vote.reference()]);
        let round_four_vote = block(4, 2, vec![round_one.refs[2], round_three_vote.reference()]);
        fixture.add_blocks([&round_two_vote]);
        fixture.add_blocks([&round_three_vote]);
        fixture.add_blocks([&round_four_vote]);

        assert!(
            fixture
                .process(
                    1,
                    &round_three_vote,
                    [target, &round_two_vote, &round_three_vote],
                )
                .is_empty()
        );
        let finalized = fixture.process(2, &round_four_vote, [&round_four_vote]);

        assert_eq!(finalized.len(), 2);
        assert_eq!(rejected(&finalized[0], target), None);
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn indirect_accepts_at_depth_two_with_one_equivocating_voter() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        let accepts = round_one.votes(0..3, Vote::Accept);
        let non_voters = round_one.votes(3..5, Vote::Absent);
        let voters = [accepts.clone(), non_voters].concat();
        let anchor = block(3, 0, refs(&voters));
        // Authority 2 also proposes a non-voting fork that no commit includes.
        fixture.add_blocks(&voters);
        fixture.add_blocks([&round_one.fork(2, Vote::Absent, 2), &anchor]);

        assert!(fixture.process(1, target, [target]).is_empty());
        assert!(
            fixture.process(2, &accepts[0], [&accepts[0]]).is_empty(),
            "Depth one must not make an indirect decision"
        );
        let finalized = fixture.process(
            3,
            &anchor,
            &[&voters[1..], std::slice::from_ref(&anchor)].concat(),
        );

        assert_eq!(finalized.len(), 3);
        assert_eq!(rejected(&finalized[0], target), None);
    }

    #[tokio::test]
    async fn indirect_uses_votes_from_all_committed_leaders() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        let accepts = round_one.votes(0..3, Vote::Accept);
        // Authority 2 equivocates with an accept and a non-vote.
        let non_voters = round_one.votes(2..6, Vote::Absent);
        fixture.add_blocks(&accepts);
        fixture.add_blocks(&non_voters);

        // The named leader links one accept. Another committed leader links the certificate.
        let named_leader = block(
            3,
            0,
            [vec![accepts[0].reference()], refs(&non_voters)].concat(),
        );
        let other_leader = block(3, 1, [refs(&accepts), refs(&non_voters[1..3])].concat());
        fixture.add_blocks([&named_leader, &other_leader]);

        assert!(fixture.process(1, target, [target]).is_empty());
        let committed = [
            accepts,
            non_voters,
            vec![other_leader, named_leader.clone()],
        ]
        .concat();
        let finalized = fixture.process(2, &named_leader, &committed);

        assert_eq!(finalized.len(), 2);
        assert_eq!(rejected(&finalized[0], target), None);
    }

    #[tokio::test]
    async fn indirect_decides_from_committed_votes_after_local_gc() {
        let cases: [(MakeVotes, Option<Vec<TransactionIndex>>, &str); 2] = [
            // Three accepts form a certificate. The voting blocks from authorities 3 and 4
            // are in round three and reject through their cutoffs, so they add no accepts.
            (
                |r| [r.votes(0..3, Vote::Accept), r.votes(3..5, Vote::Absent)].concat(),
                None,
                "indirect_finalize",
            ),
            // Authority 4's round-three cutoff is the fifth cutoff reject.
            (
                |r| [r.votes(0..4, Vote::Cutoff), r.votes(4..5, Vote::Absent)].concat(),
                Some(vec![0]),
                "indirect_reject",
            ),
        ];
        for (make_votes, expected_rejects, status) in cases {
            let mut fixture = Fixture::with_gc_depth(3);
            let round_one = fixture.round_one(1);
            let target = &round_one.target;
            let voters = make_votes(&round_one);
            fixture.add_blocks(&voters);
            let first_leader = block(3, 0, refs(&voters));
            fixture.add_blocks([&first_leader]);
            let first_commit = fixture.linearize(&first_leader);
            assert_eq!(target.round(), fixture.dag_state.read().gc_round() + 1);
            assert!(fixture.finalizer.process_commit(first_commit).is_empty());

            // These peers complete the anchor's parent quorum. Linearizing the anchor advances
            // local GC to the target before direct finalization can count their votes.
            let peers: Vec<_> = (1..5)
                .map(|author| vote_block(3, author, refs(&voters), vec![], target.round()))
                .collect();
            fixture.add_blocks(&peers);
            let anchor = block(4, 0, refs(&[vec![first_leader], peers].concat()));
            fixture.add_blocks([&anchor]);
            let second_commit = fixture.linearize(&anchor);
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
            assert_eq!(rejected(&finalized[0], target).cloned(), expected_rejects);
            let statuses = &fixture
                .context
                .metrics
                .node_metrics
                .finalizer_transaction_status;
            for label in ["direct_finalize", "direct_reject", status] {
                let expected = u64::from(label == status);
                assert_eq!(
                    statuses.with_label_values(&[label]).get(),
                    expected,
                    "{label}"
                );
            }
            assert!(fixture.finalizer.is_empty());
        }
    }

    #[tokio::test]
    async fn depth_two_rejects_without_an_eligible_accept_certificate() {
        let cases: [(&str, MakeVotes, Round, bool); 2] = [
            // At the commit GC round, local traversal already hides the target's children. The
            // indirect guard must also exclude the committed accept quorum, whose voting
            // blocks are no longer guaranteed to survive until the depth-two decision.
            (
                "accept quorum at the commit GC round",
                |r| r.votes(0..5, Vote::Accept),
                4,
                true,
            ),
            // Three cutoff rejects reach certification stake, but rejection requires the full
            // five-authority quorum. Two accepts also fall short of an accept certificate.
            (
                "cutoff certificate without a reject quorum",
                |r| [r.votes(0..3, Vote::Cutoff), r.votes(3..5, Vote::Accept)].concat(),
                3,
                false,
            ),
        ];
        for (name, make_votes, first_leader_round, guarded) in cases {
            let mut fixture = Fixture::with_gc_depth(3);
            let round_one = fixture.round_one(1);
            let target = &round_one.target;
            let voters = make_votes(&round_one);
            fixture.add_blocks(&voters);
            let leaders = fixture.add_rounds(&voters, 3..=first_leader_round + 2);
            let leaders = &leaders[(first_leader_round - 3) as usize..];

            for (depth, leader) in leaders.iter().enumerate() {
                let finalized = fixture.process_leader(leader);
                if depth == 0 {
                    let children = fixture
                        .dag_state
                        .read()
                        .get_block_children(&target.reference());
                    assert_eq!(children.is_none(), guarded, "{name}");
                }
                if depth < 2 {
                    assert!(finalized.is_empty(), "{name}");
                } else {
                    assert_eq!(finalized.len(), 3, "{name}");
                    assert_eq!(rejected(&finalized[0], target), Some(&vec![0]), "{name}");
                }
            }
            let hostname = &fixture
                .context
                .committee
                .authority(target.author())
                .hostname;
            let skipped = fixture
                .context
                .metrics
                .node_metrics
                .finalizer_skipped_voting_blocks
                .with_label_values(&[hostname, "direct"])
                .get();
            assert_eq!(skipped, u64::from(guarded), "{name}");
            assert!(fixture.finalizer.is_empty(), "{name}");
        }
    }

    #[tokio::test]
    async fn gc_guard_prevents_later_vote_from_replacing_a_pruned_reject() {
        let mut fixture = Fixture::with_gc_depth(3);
        let round_one = fixture.round_one_blocks(&[0, 0, 0, 0, 0, 1]);
        let target = round_one[5].clone();
        let target_ref = target.reference();

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
                        vec![reject_votes(&target, vec![0])]
                    } else {
                        vec![]
                    };
                    vote_block(round, author, ancestors, votes, 0)
                })
                .collect();
            fixture.add_blocks(&blocks);
            rounds.push(blocks);
        }
        let first_reject = &rounds[1][4];

        // This proposal was created before GC, so its signed cutoff remains zero. Its own
        // round-2 ancestor already included the target, so proposal logic does not repeat the
        // reject. Its other parents also provide a path back to the target.
        let later_vote = block(
            4,
            4,
            [refs(&rounds[2]), vec![first_reject.reference()]].concat(),
        );
        fixture.add_blocks([&later_vote]);
        let depth_two_anchor = block(
            6,
            0,
            [refs(&rounds[4]), vec![later_vote.reference()]].concat(),
        );
        fixture.add_blocks([&depth_two_anchor]);

        let complete_voting_blocks =
            collect_voting_blocks(&*fixture.dag_state.read(), target_ref, 5);
        assert_eq!(
            accept_certificate(&fixture.finalizer, target_ref, &complete_voting_blocks),
            (2, false)
        );

        let first_commit = fixture.linearize(&rounds[3][0]);
        assert!(
            first_commit
                .blocks
                .iter()
                .any(|block| block.reference() == target_ref)
        );
        assert!(fixture.finalizer.process_commit(first_commit).is_empty());
        assert!(fixture.process_leader(&rounds[4][0]).is_empty());
        assert_eq!(fixture.dag_state.read().gc_round(), first_reject.round());

        let third_commit = fixture.linearize(&depth_two_anchor);
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
        assert!(!VotingBlock::new(later_vote.clone()).rejects_by_cutoff(target_ref));

        // The real linearizer omitted the round-2 reject at GC, but retained the round-4
        // descendant. The per-block cutoff therefore permits a false accept certificate.
        let incomplete_voting_blocks = collect_voting_blocks(&committed_graph, target_ref, 5);
        assert!(
            incomplete_voting_blocks
                .iter()
                .any(|vote| vote.block_ref == later_vote.reference())
        );
        assert_eq!(
            accept_certificate(&fixture.finalizer, target_ref, &incomplete_voting_blocks),
            (3, true)
        );

        let finalized = fixture.finalizer.process_commit(third_commit);
        assert_eq!(finalized.len(), 3);
        assert_eq!(rejected(&finalized[0], &target), Some(&vec![0]));
        assert!(fixture.finalizer.is_empty());
    }

    #[tokio::test]
    async fn block_notifications_are_coalesced_and_rearmed() {
        let fixture = Fixture::new();
        let target = fixture.round_one(1).target;
        let attempts = &fixture.context.metrics.node_metrics.finalizer_v3_attempts;
        let commit_passes = attempts.with_label_values(&["commit"]);
        let block_passes = attempts.with_label_values(&["block_update"]);
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
        assert_eq!(commit_passes.get(), 0);
        assert_eq!(block_passes.get(), 0);
        sender.send(make_commit(1, &target, [&target])).unwrap();
        assert!(futures::poll!(&mut run).is_pending());
        assert_eq!(commit_passes.get(), 1);
        assert_eq!(block_passes.get(), 0);

        for (batch, notifications) in [1, 10, 1].into_iter().enumerate() {
            for _ in 0..notifications {
                block_sender.send_replace(());
            }
            assert!(futures::poll!(&mut run).is_pending());
            assert_eq!(block_passes.get(), batch as u64 + 1);
            assert!(futures::poll!(&mut run).is_pending());
            assert_eq!(block_passes.get(), batch as u64 + 1);
        }
        assert_eq!(commit_passes.get(), 1);

        // Closing notifications alone must not terminate or spin the commit receiver.
        drop(block_sender);
        assert!(futures::poll!(&mut run).is_pending());
        drop(sender);
        assert!(futures::poll!(&mut run).is_ready());
    }

    #[tokio::test(start_paused = true)]
    async fn status_timer_reports_waits_without_retrying_finalization() {
        let mut fixture = Fixture::new();
        let round_one = fixture.round_one(1);
        let target = &round_one.target;
        let voters = round_one.votes(0..5, Vote::Accept);
        let later_leader = round_one.vote(5, Vote::Absent);
        fixture.add_blocks([&later_leader]);
        let (commit_sender, mut commit_receiver) = unbounded_channel("finalizer_v3_status_output");
        fixture.finalizer.commit_sender = commit_sender;
        let (sender, receiver) = unbounded_channel("finalizer_v3_status_input");
        let (block_sender, block_updates) = watch::channel(());
        let run = fixture.finalizer.run(receiver, block_updates);
        tokio::pin!(run);
        sender.send(make_commit(1, target, [target])).unwrap();
        assert!(futures::poll!(&mut run).is_pending());
        tokio::time::advance(Duration::from_millis(100)).await;
        sender
            .send(make_commit(2, &later_leader, [&later_leader]))
            .unwrap();
        assert!(futures::poll!(&mut run).is_pending());

        // Evidence alone must not make the status timer run a finalization pass.
        fixture.dag_state.write().accept_blocks(voters.clone());
        fixture
            .transaction_vote_tracker
            .add_voted_blocks(voters.into_iter().map(|block| (block, vec![])).collect());
        tokio::time::advance(STATUS_INTERVAL - Duration::from_millis(100)).await;
        assert!(futures::poll!(&mut run).is_pending());
        let metrics = &fixture.context.metrics.node_metrics;
        let attempts = |trigger: &str| {
            metrics
                .finalizer_v3_attempts
                .with_label_values(&[trigger])
                .get()
        };
        assert_eq!(metrics.finalizer_v3_oldest_pending_seconds.get(), 1.0);
        assert_eq!(metrics.finalizer_v3_pending_transactions.get(), 1);
        assert_eq!(metrics.finalizer_v3_ready_commits.get(), 1);
        assert_eq!(metrics.finalizer_buffered_commits.get(), 2);
        assert!(commit_receiver.try_recv().is_err());
        assert_eq!(attempts("commit"), 2);
        assert_eq!(attempts("block_update"), 0);

        block_sender.send_replace(());
        assert!(futures::poll!(&mut run).is_pending());
        for index in [1, 2] {
            assert_eq!(commit_receiver.try_recv().unwrap().commit_ref.index, index);
        }
        assert!(commit_receiver.try_recv().is_err());
        assert_eq!(attempts("block_update"), 1);
        for (stage, expected) in [("decision", 1.0), ("ordered_release", 0.9), ("total", 1.9)] {
            let histogram = metrics
                .finalizer_v3_commit_wait_seconds
                .with_label_values(&[stage]);
            assert_eq!(histogram.get_sample_count(), 2);
            assert!((histogram.get_sample_sum() - expected).abs() < 1e-9);
        }
        assert_eq!(metrics.finalizer_v3_pending_transactions.get(), 0);
        assert_eq!(metrics.finalizer_v3_ready_commits.get(), 0);
        assert_eq!(metrics.finalizer_buffered_commits.get(), 0);
        assert_eq!(metrics.finalizer_v3_oldest_pending_seconds.get(), 0.0);
        drop(sender);
        assert!(futures::poll!(&mut run).is_ready());
    }

    #[tokio::test]
    async fn handle_finalizes_and_persists_commits_in_order() {
        for votes_available_initially in [true, false] {
            let fixture = Fixture::new();
            let round_one = fixture.round_one(2);
            let target = &round_one.target;
            let voters = round_one.votes(0..5, Vote::Reject(vec![1]));
            let later_leader = round_one.vote(5, Vote::Absent);
            let initial_votes = if votes_available_initially { 5 } else { 4 };
            fixture.add_blocks(&voters[..initial_votes]);
            fixture.add_blocks([&later_leader]);

            // The protocol config selects CommitFinalizerV3.
            let (mut handle, mut receiver) =
                fixture.start_handle(fixture.transaction_vote_tracker.clone());
            let first = make_commit(1, target, [target]);
            let second = make_commit(2, &later_leader, [&later_leader]);
            handle.send(first.clone()).unwrap();
            if !votes_available_initially {
                handle.send(second.clone()).unwrap();
                let buffered = &fixture
                    .context
                    .metrics
                    .node_metrics
                    .finalizer_buffered_commits;
                wait_until(
                    "Both commits must be buffered before the final vote arrives",
                    || buffered.get() == 2,
                )
                .await;
                assert!(receiver.try_recv().is_err());

                // New block evidence finalizes both commits without another commit.
                fixture.add_blocks(&voters[4..]);
                handle.notify_new_blocks();
            }
            // Initially available votes must finalize the first commit without a notification
            // or a later commit triggering a retry.
            for (expected, rejects) in [
                (&first, BTreeMap::from([(target.reference(), vec![1])])),
                (&second, BTreeMap::new()),
            ] {
                let finalized = recv(&mut receiver).await;
                assert_eq!(finalized.commit_ref, expected.commit_ref);
                assert_eq!(finalized.rejected_transactions_by_block, rejects);
                assert_eq!(
                    fixture
                        .store
                        .read_rejected_transactions(finalized.commit_ref)
                        .unwrap(),
                    Some(rejects)
                );
                if votes_available_initially && expected.commit_ref == first.commit_ref {
                    handle.send(second.clone()).unwrap();
                }
            }
            // A commit without transactions finalizes as soon as it arrives.
            let third = make_commit(3, &voters[0], [&voters[0]]);
            handle.send(third.clone()).unwrap();
            assert_eq!(recv(&mut receiver).await.commit_ref, third.commit_ref);
            assert_eq!(
                fixture.store.read_last_finalized_commit().unwrap(),
                Some(third.commit_ref)
            );
            handle.stop().await;
        }
    }

    #[tokio::test]
    async fn recovery_recomputes_partial_cutoff_rejections_after_local_gc() {
        let mut fixture = Fixture::with_gc_depth(3);
        let round_one = fixture.round_one(2);
        let target = round_one.target.clone();
        let voters = [
            round_one.votes(0..2, Vote::Cutoff),
            vec![round_one.vote(2, Vote::Reject(vec![0, 1]))],
            round_one.votes(3..5, Vote::Reject(vec![0])),
        ]
        .concat();
        fixture.add_blocks(&voters);
        let leaders = fixture.add_rounds(&voters, 3..=5);
        for leader in &leaders[..2] {
            assert!(fixture.process_leader(leader).is_empty());
        }
        let first_state = &fixture.finalizer.pending_commits[0];
        assert_eq!(
            first_state.rejected_transactions.get(&target.reference()),
            Some(&BTreeSet::from([0]))
        );
        assert_eq!(fixture.pending(&target), Some(&BTreeSet::from([1])));
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
        let expected = fixture.process_leader(&leaders[2]);
        assert_eq!(expected.len(), 3);
        assert_eq!(rejected(&expected[0], &target), Some(&vec![0, 1]));

        let (context, store, dag_state) = fixture.restart();
        let tracker = noop_tracker(&context, &dag_state);
        assert_eq!(dag_state.read().gc_round(), target.round());
        assert!(
            dag_state
                .read()
                .get_block_children(&target.reference())
                .is_none()
        );
        assert!(tracker.get_reject_votes(&target.reference()).is_none());
        let indirect_rejects = context
            .metrics
            .node_metrics
            .finalizer_transaction_status
            .with_label_values(&["indirect_reject"]);
        let rejects_before_recovery = indirect_rejects.get();
        let (mut observer, mut receiver) = start_observer(&context, dag_state, &tracker).await;
        assert_eq!(
            tracker.get_reject_votes(&target.reference()),
            Some(vec![(0, 3), (1, 1)])
        );
        // Replay must reconstruct transaction 0's cutoff quorum before the depth-two fallback
        // exists. Transaction 1 must still keep the commit pending at this point.
        wait_until(
            "Recovery must reestablish the partial rejection at depth one",
            || indirect_rejects.get() != rejects_before_recovery,
        )
        .await;
        assert_eq!(indirect_rejects.get(), rejects_before_recovery + 1);
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
        observer
            .handle_committed_leaders(vec![leaders[2].clone()], true)
            .unwrap();

        assert_replayed(&mut receiver, observer, &store, &expected).await;
    }

    #[tokio::test]
    async fn recovery_notifies_finalizer_after_restoring_reject_votes() {
        use crate::block_verifier::BlockVerifier;

        // Pauses voting on one block during recovery until the test allows it to resume.
        struct GatedVerifier {
            block_ref: BlockRef,
            paused: parking_lot::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
            resume: parking_lot::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
        }

        impl BlockVerifier for GatedVerifier {
            fn verify_and_vote(
                &self,
                block: crate::block::SignedBlock,
                serialized_block: bytes::Bytes,
            ) -> crate::error::ConsensusResult<(VerifiedBlock, Vec<TransactionIndex>)> {
                NoopBlockVerifier.verify_and_vote(block, serialized_block)
            }

            fn vote(
                &self,
                block: &VerifiedBlock,
            ) -> crate::error::ConsensusResult<Vec<TransactionIndex>> {
                if block.reference() == self.block_ref {
                    self.paused.lock().take().unwrap().send(()).unwrap();
                    self.resume.lock().take().unwrap().blocking_recv().unwrap();
                }
                Ok(vec![])
            }
        }

        let mut fixture = Fixture::new();
        let round_one = fixture.round_one_blocks(&[]);
        let round_two = fixture.add_full_round(2, &refs(&round_one), &[]);
        let round_three = fixture.add_full_round(3, &refs(&round_two), &[1]);
        let target = &round_three[0];
        // Authorities 0..=2 link and reject the target in round four. The others do not link it.
        let round_four: Vec<_> = (0..COMMITTEE_SIZE as u32)
            .map(|author| {
                let ancestors = round_three
                    .iter()
                    .filter(|block| author < 3 || block.reference() != target.reference())
                    .map(|block| block.reference())
                    .collect();
                let votes = if author < 3 {
                    vec![reject_votes(target, vec![0])]
                } else {
                    vec![]
                };
                vote_block(4, author, ancestors, votes, 0)
            })
            .collect();
        fixture.add_blocks(&round_four);
        // These voting blocks are outside the direct traversal window. Only tracker recovery
        // supplies them to the finalizer, so the DAG alone cannot complete the reject quorum.
        let late_rejects: Vec<_> = (3..5)
            .map(|author| {
                let votes = vec![reject_votes(target, vec![0])];
                vote_block(5, author, refs(&round_four[..5]), votes, 0)
            })
            .collect();
        fixture.add_blocks(&late_rejects);
        let committed = fixture.linearize(target);
        fixture.dag_state.write().flush();
        assert!(
            fixture
                .store
                .read_rejected_transactions(committed.commit_ref)
                .unwrap()
                .is_none()
        );

        let (context, _, dag_state) = fixture.restart();
        let (paused_tx, paused_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let tracker = TransactionVoteTracker::new(
            context.clone(),
            Arc::new(GatedVerifier {
                block_ref: late_rejects[1].reference(),
                paused: parking_lot::Mutex::new(Some(paused_tx)),
                resume: parking_lot::Mutex::new(Some(resume_rx)),
            }),
            dag_state.clone(),
        );
        let (consumer, mut receiver) = CommitConsumerArgs::new(0, 0);
        let observer_task = tokio::spawn({
            let context = context.clone();
            let tracker = tracker.clone();
            async move { CommitObserver::new(context, consumer, dag_state, tracker).await }
        });

        // Hold the last reject vote until the finalizer has tried the recovered commit.
        tokio::time::timeout(Duration::from_secs(5), paused_rx)
            .await
            .unwrap()
            .unwrap();
        let buffered = &context.metrics.node_metrics.finalizer_buffered_commits;
        wait_until("The recovered commit must be buffered", || {
            buffered.get() == 1
        })
        .await;
        assert!(receiver.try_recv().is_err());
        resume_tx.send(()).unwrap();
        let mut observer = tokio::time::timeout(Duration::from_secs(5), observer_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            tracker.get_reject_votes(&target.reference()),
            Some(vec![(0, 5)])
        );

        // Recovered reject votes must wake the finalizer without new blocks.
        let result = recv(&mut receiver).await;
        assert_eq!(result.commit_ref, committed.commit_ref);
        assert_eq!(rejected(&result, target), Some(&vec![0]));
        observer.stop().await;
    }

    #[tokio::test]
    async fn recovery_preserves_persisted_cutoff_rejection_without_local_votes() {
        let mut fixture = Fixture::with_gc_depth(3);
        let round_one = fixture.round_one(1);
        let target = round_one.target.clone();
        let voters = round_one.votes(0..5, Vote::Cutoff);
        fixture.add_blocks(&voters);
        let mut expected = vec![];
        for leader in fixture.add_rounds(&voters, 3..=5) {
            let finalized = fixture.process_leader(&leader);
            assert_eq!(finalized.len(), 1);
            persist_finalized_commits(
                &fixture.dag_state,
                &fixture.transaction_vote_tracker,
                &finalized,
                true,
            );
            expected.extend(finalized);
        }
        assert_eq!(rejected(&expected[0], &target), Some(&vec![0]));

        let (context, store, dag_state) = fixture.restart();
        let tracker = noop_tracker(&context, &dag_state);
        assert_eq!(dag_state.read().gc_round(), voters[0].round());
        assert!(
            dag_state
                .read()
                .get_block_children(&voters[0].reference())
                .is_none()
        );
        let (observer, mut receiver) = start_observer(&context, dag_state, &tracker).await;

        // Real recovery loads the persisted rejection marker and bypasses voting. Neither the
        // local traversal of voting blocks nor the fresh tracker can reconstruct this old decision.
        assert!(tracker.get_reject_votes(&target.reference()).is_none());
        let replayed = assert_replayed(&mut receiver, observer, &store, &expected).await;
        assert!(
            replayed
                .iter()
                .all(|commit| commit.recovered_rejected_transactions)
        );
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use parking_lot::RwLock;
use rand::{SeedableRng, prelude::SliceRandom, rngs::StdRng};

use crate::{
    CommitIndex, CommitRef, CommittedSubDag, DagState,
    block::{BlockAPI, VerifiedBlock},
    commit::{CommitRange, load_committed_subdag_from_store},
    context::Context,
    leader_scoring::ReputationScores,
};

// Max pending commits kept for scoring.
const MAX_PENDING_COMMITS: usize = 3;

/// Incremental, sliding-window leader scorer over recent commits.
///
/// On each new commit C, score contributions are computed and added to the
/// running totals. When the running window is full, the oldest commit is evicted
/// and its contributions are subtracted from the running totals.
///
/// Scoring rule for commit C:
///
/// When `C >= 4`, C-1, C-2 and C-3 exist as pending commits.
/// Let `r` be the leader round of commit C-3.
///
/// For each authority A: consider A's block at round `r+1` (voting round) across commits
/// from commit C-2 to commit with leader round > `r+1`.
/// If there are multiple A blocks in this slot, `score[A] = 0`.
///
/// Collect the set of distinct authorities P, such that A's voting block votes to
/// (has an ancestor link to) P's round `r` block (leader block) in commit C-3.
/// Then `voted_for_stake[A] = sum_{P in distinct set} stake(P)`.
///
/// Collect the set of distinct authorities Q, such that a Q's round `r+2`
/// block certifies (has an ancestor link to) A's voting block in round `r+1`,
/// across commits from C-2 to commit with leader round > `r+2`.
/// Then `certified_by_stake[A] = sum_{Q in distinct set} stake(Q)`.
///
/// The overall score for A is `score[A] = voted_for_stake[A] * certified_by_stake[A]`.
pub(crate) struct LeaderScheduleV3 {
    context: Arc<Context>,
    // Total scores per authority in the running window, indexed by AuthorityIndex.
    // Each scored commit contributes `voted_for_stake[A] * certified_by_stake[A]`
    // (stake² units) per authority A, summed across the running window.
    total_scores_per_authority: Vec<u64>,
    // Total number of leader-round blocks across the running window.
    // Used for metrics.
    total_num_leaders: usize,
    // Running sum of `Σ stake(leader_authors)` across the scored commits in
    // the window. The normalized-score metric multiplies this by
    // `committee.total_stake()` at report time to match the stake² units of
    // `total_scores_per_authority`.
    total_leader_stakes: u64,

    // Holds at most the last three committed subdags, for computing leader scores.
    // Score entries are produced once 3 commits have accumulated; on each
    // subsequent `add_commit`, the oldest is evicted and a new commit appended.
    pending_commits: VecDeque<CommittedSubDag>,
    // One entry per scored commit in the running window.
    // Used to subtract a commit's contributions from `total_scores_per_authority`
    // on eviction. Bounded by `leader_schedule_window_size`.
    scores_entries: VecDeque<ScoresEntry>,
    // Leader schedule for the current commit interval. The commit index and
    // minimum round move every commit; leader selection changes only at
    // configured interval boundaries.
    current_schedule: NextCommitLeaderSchedule,
}

impl LeaderScheduleV3 {
    /// Constructs a `LeaderScheduleV3`.
    /// Replays persisted commits from storage if available, to ensure
    /// the running window and current scores are recovered exactly.
    pub(crate) fn from_store(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> LeaderScheduleV3 {
        let committee_size = context.committee.size();
        let mut leader_schedule = LeaderScheduleV3::new(context.clone(), vec![0; committee_size]);

        let last_commit_index = dag_state.read().last_commit_index();
        let replay_count = LeaderScheduleV3::replay_length(&context);
        // Replay maximum replay_count commits, and starting from commit 1 at minimum.
        let replay_start = last_commit_index.saturating_sub(replay_count) + 1;
        if replay_start > last_commit_index {
            // No commits to replay, return the new schedule with empty state.
            return leader_schedule;
        }

        let store = dag_state.read().store();
        let commits = store
            .scan_commits((replay_start..=last_commit_index).into())
            .expect("Failed to scan commits from storage");
        for commit in commits {
            let subdag = load_committed_subdag_from_store(store.as_ref(), commit);
            leader_schedule.add_commit(subdag);
        }
        leader_schedule
    }

    pub(crate) fn new(context: Arc<Context>, total_scores_per_authority: Vec<u64>) -> Self {
        assert_eq!(total_scores_per_authority.len(), context.committee.size());
        let window_size = context.protocol_config.leader_schedule_window_size() as usize;
        let mut schedule = Self {
            context,
            total_scores_per_authority,
            total_num_leaders: 0,
            total_leader_stakes: 0,
            pending_commits: VecDeque::with_capacity(MAX_PENDING_COMMITS),
            scores_entries: VecDeque::with_capacity(window_size),
            current_schedule: NextCommitLeaderSchedule::default(),
        };
        schedule.current_schedule = schedule.compute_next_commit_leader_schedule();
        schedule
    }

    /// Index of the next commit to be added — one past the last commit in
    /// `pending_commits`, or 1 if none have been added yet.
    pub(crate) fn next_commit_index(&self) -> CommitIndex {
        self.pending_commits
            .back()
            .map(|c| c.commit_ref.index)
            .unwrap_or(0)
            + 1
    }

    /// Minimum round the next commit's leader can be at — one past the leader
    /// round of the last commit in `pending_commits`, or 1 if none have been
    /// added yet.
    pub(crate) fn min_next_leader_round(&self) -> Round {
        self.pending_commits
            .back()
            .map(|c| c.leader.round)
            .unwrap_or(0)
            + 1
    }

    /// Number of recent committed sub-dags that must be replayed on startup
    /// to reconstruct the full state: `leader_schedule_window_size` for the
    /// scoring window, the pending commits held before scoring begins, and
    /// `interval - 1` more so the replay spans back to the most recent rotation
    /// boundary with the full scoring window already populated.
    fn replay_length(context: &Context) -> u32 {
        context.protocol_config.leader_schedule_window_size()
            + MAX_PENDING_COMMITS as u32
            + context.protocol_config.leader_schedule_update_interval()
            - 1
    }

    /// Returns the current leader schedule. See `current_schedule` for what
    /// is refreshed every commit vs. only on rotation boundaries.
    pub(crate) fn next_commit_leader_schedule(&self) -> NextCommitLeaderSchedule {
        self.current_schedule.clone()
    }

    pub(crate) fn compute_next_commit_leader_schedule(&self) -> NextCommitLeaderSchedule {
        let allowed_leaders = self.select_allowed_leaders_with_fixed_config();
        let num_leaders = allowed_leaders.len();
        NextCommitLeaderSchedule {
            next_commit_index: self.next_commit_index(),
            min_next_leader_round: self.min_next_leader_round(),
            num_leaders,
            allowed_leaders,
        }
    }

    /// Returns the set of authorities allowed to be leaders, based on current
    /// running scores and threshold in config.
    ///
    /// Authorities are ranked by score descending. Lowest-score authorities are
    /// excluded while their cumulative stake remains within `stake_threshold`%
    /// of total stake.
    pub(crate) fn select_allowed_leaders_with_fixed_config(&self) -> Vec<AuthorityIndex> {
        // Create a vector of authorities and their scores.
        let mut by_score: Vec<(AuthorityIndex, u64)> = self
            .total_scores_per_authority
            .iter()
            .enumerate()
            .map(|(i, score)| {
                let authority_index = self
                    .context
                    .committee
                    .to_authority_index(i)
                    .expect("Should be a valid AuthorityIndex");
                (authority_index, *score)
            })
            .collect();

        // Shuffle deterministically.
        let seed_bytes = self
            .pending_commits
            .back()
            .map(|c| c.commit_ref.digest.into_inner())
            .unwrap_or_else(|| {
                // No pending commits yet — derive a per-epoch seed so the
                // initialization shuffle differs across epochs instead of
                // always defaulting to all-zeros.
                let mut seed = [0u8; 32];
                seed[..8].copy_from_slice(&self.context.epoch_start_timestamp_ms.to_le_bytes());
                seed[8..16].copy_from_slice(&self.context.committee.epoch().to_le_bytes());
                seed
            });
        let mut rng = StdRng::from_seed(seed_bytes);
        by_score.shuffle(&mut rng);

        // Use stable sort to produce deterministic order of authorities with the same score.
        by_score.sort_by(|a, b| b.1.cmp(&a.1));

        let mut accumulated_bad_stake = 0u64;
        let cutoff = (self.context.protocol_config.bad_nodes_stake_threshold()
            * self.context.committee.total_stake())
            / 100;
        while let Some((idx, _)) = by_score.last() {
            let stake = self.context.committee.stake(*idx);
            if accumulated_bad_stake + stake > cutoff {
                break;
            }
            accumulated_bad_stake += stake;
            by_score.pop();
        }

        by_score.into_iter().map(|(idx, _)| idx).collect()
    }

    /// On each `add_commit(c)`, once 3 commits have accumulated, a score entry
    /// corresponding to the front of `pending_commits` (the C-3-equivalent)
    /// is produced; then C is appended to `pending_commits` and the front is
    /// evicted. Evicts the oldest entry from the running window if full.
    ///
    /// Architectural contract: consecutive commits must have strictly
    /// increasing leader rounds. Under the invariant, 3 commits are sufficient
    /// to satisfy both the r+1 voting-block scan and the r+2 certifying-block
    /// scan (some commit in [C-2, C-1, c] satisfies each `>` bound).
    pub(crate) fn add_commit(&mut self, c: CommittedSubDag) {
        // Ensure commits are added in order. On the very first call there is
        // no prior commit to check against; once we have one, every subsequent
        // commit must be consecutive.
        if let Some(last) = self.pending_commits.back() {
            assert_eq!(c.commit_ref.index, last.commit_ref.index + 1);
        }

        // Need C-3, C-2, C-1 in pending before producing a score entry.
        // Until then, this is warmup (new instance, new epoch, or recovery).
        if self.pending_commits.len() < MAX_PENDING_COMMITS {
            self.pending_commits.push_back(c);
            self.refresh_current_schedule();
            self.report_metrics();
            return;
        }

        let c_minus_3 = &self.pending_commits[0];
        let c_minus_2 = &self.pending_commits[1];
        let c_minus_1 = &self.pending_commits[2];
        let leader_round = c_minus_3.leader.round;
        let vote_round = leader_round + 1;
        let certify_round = leader_round + 2;

        // Scoring logic assumes strictly increasing leader rounds across consecutive commits.
        assert!(
            c_minus_3.leader.round < c_minus_2.leader.round
                && c_minus_2.leader.round < c_minus_1.leader.round
                && c_minus_1.leader.round < c.leader.round,
            "consecutive commits must have strictly increasing leader rounds; \
             got {}, {}, {}, {}",
            c_minus_3.leader.round,
            c_minus_2.leader.round,
            c_minus_1.leader.round,
            c.leader.round,
        );

        // Every block in leader round is a leader block. A commit may
        // contain one or multiple leader blocks.
        let leader_refs: BTreeSet<BlockRef> = c_minus_3
            .blocks
            .iter()
            .filter(|b| b.round() == leader_round)
            .map(|b| b.reference())
            .collect();

        // A failure here points at an malformed CommittedSubDag.
        assert!(!leader_refs.is_empty());
        assert!(
            leader_refs.contains(&c_minus_3.leader),
            "C-3's leader {} missing from its own leader blocks {:?}",
            c_minus_3.leader,
            leader_refs,
        );

        // Walk [C-2, C-1, C] up to and including the first commit whose
        // leader round exceeds each bound. The cut off is chosen to exclude
        // blocks that are produced late relative to the current highest round.
        let voting_commits =
            Self::scan_commits_until_leader_round_above(&self.pending_commits, &c, vote_round);
        let certifying_commits =
            Self::scan_commits_until_leader_round_above(&self.pending_commits, &c, certify_round);

        // Group round r+1 voting blocks by author.
        let mut voting_blocks_by_author: BTreeMap<AuthorityIndex, Vec<&VerifiedBlock>> =
            BTreeMap::new();
        for cmt in &voting_commits {
            for b in &cmt.blocks {
                if b.round() == vote_round {
                    voting_blocks_by_author
                        .entry(b.author())
                        .or_default()
                        .push(b);
                }
            }
        }
        let mut certifying_blocks: Vec<&VerifiedBlock> = Vec::new();
        for cmt in &certifying_commits {
            for b in &cmt.blocks {
                if b.round() == certify_round {
                    certifying_blocks.push(b);
                }
            }
        }

        let mut score_contributions = vec![0u64; self.context.committee.size()];
        for (a, voting_blocks) in &voting_blocks_by_author {
            // Equivocation: more than one r+1 block from A → score[A] = 0.
            if voting_blocks.len() != 1 {
                continue;
            }
            let voting_block = voting_blocks[0];

            // voted_for_stake[A]: distinct authorities whose leader block
            // in C-3 is voted by A's voting block.
            let mut leader_authorities = BTreeSet::<AuthorityIndex>::new();
            for anc in voting_block.ancestors() {
                if leader_refs.contains(anc) {
                    leader_authorities.insert(anc.author);
                }
            }
            let voted_for_stake: u64 = leader_authorities
                .into_iter()
                .map(|i| self.context.committee.stake(i))
                .sum();
            // Product would be 0 anyway; skip the certifying-block scan.
            if voted_for_stake == 0 {
                continue;
            }

            // certified_by_stake[A]: distinct authorities whose round r+2 block
            // certifies A's voting block (has it as an ancestor).
            let voting_block_ref = voting_block.reference();
            let mut certifying_authorities = BTreeSet::<AuthorityIndex>::new();
            for certifying_block in &certifying_blocks {
                if certifying_block.ancestors().contains(&voting_block_ref) {
                    certifying_authorities.insert(certifying_block.author());
                }
            }
            let certified_by_stake: u64 = certifying_authorities
                .into_iter()
                .map(|i| self.context.committee.stake(i))
                .sum();

            score_contributions[a.value()] =
                voted_for_stake.checked_mul(certified_by_stake).unwrap();
        }

        let num_leaders = leader_refs.len();
        let leader_stakes: u64 = leader_refs
            .iter()
            .map(|block_ref| self.context.committee.stake(block_ref.author))
            .sum();

        while self.scores_entries.len() >= self.window_size() {
            let evicted = self.scores_entries.pop_front().expect("non empty");
            tracing::trace!(
                "LeaderScheduleV3 evicting scored commit {} from running window",
                evicted.commit_ref
            );
            for (i, contribution) in evicted.score_contributions.iter().enumerate() {
                self.total_scores_per_authority[i] = self.total_scores_per_authority[i]
                    .checked_sub(*contribution)
                    .unwrap();
            }
            self.total_num_leaders = self
                .total_num_leaders
                .checked_sub(evicted.num_leaders)
                .unwrap();
            self.total_leader_stakes = self
                .total_leader_stakes
                .checked_sub(evicted.leader_stakes)
                .unwrap();
        }

        for (i, delta) in score_contributions.iter().enumerate() {
            self.total_scores_per_authority[i] = self.total_scores_per_authority[i]
                .checked_add(*delta)
                .unwrap();
        }
        self.total_num_leaders = self.total_num_leaders.checked_add(num_leaders).unwrap();
        self.total_leader_stakes = self.total_leader_stakes.checked_add(leader_stakes).unwrap();
        self.scores_entries.push_back(ScoresEntry {
            commit_ref: c_minus_3.commit_ref,
            score_contributions,
            num_leaders,
            leader_stakes,
        });

        // Rotate pending commits: drop C-3, keep C-2 and C-1, append c.
        self.pending_commits.pop_front();
        self.pending_commits.push_back(c);

        self.refresh_current_schedule();
        self.report_metrics();
    }

    fn refresh_current_schedule(&mut self) {
        let interval = self
            .context
            .protocol_config
            .leader_schedule_update_interval() as CommitIndex;
        let on_boundary = self
            .next_commit_index()
            .saturating_sub(1)
            .is_multiple_of(interval);
        if on_boundary {
            // Boundary: full recompute, including allowed_leaders.
            self.current_schedule = self.compute_next_commit_leader_schedule();
        } else {
            // Off-boundary: only the per-commit fields move; skip the
            // expensive allowed-leaders selection.
            self.current_schedule.next_commit_index = self.next_commit_index();
            self.current_schedule.min_next_leader_round = self.min_next_leader_round();
        }
    }

    /// Walks commits [C-2, C-1, C] in order.
    /// Returning every commit up to and **including** the first one whose
    /// leader round is strictly greater than `upper`.
    fn scan_commits_until_leader_round_above<'a>(
        pending: &'a VecDeque<CommittedSubDag>,
        c: &'a CommittedSubDag,
        upper: Round,
    ) -> Vec<&'a CommittedSubDag> {
        let mut out = Vec::new();
        for cmt in pending.iter().skip(1).chain(std::iter::once(c)) {
            out.push(cmt);
            if cmt.leader.round > upper {
                break;
            }
        }
        assert!(
            out.last()
                .map(|cmt| cmt.leader.round > upper)
                .unwrap_or(false),
            "scan must end on a commit with leader.round > {upper}; \
             strictly-increasing-leader-rounds invariant violated upstream",
        );
        out
    }

    /// Snapshot of the running per-authority scores.
    pub(crate) fn current_reputation_scores(&self) -> ReputationScores {
        let interval_size = self
            .context
            .protocol_config
            .leader_schedule_update_interval();
        let interval_num = (self.current_schedule.next_commit_index - 1) / interval_size;
        let commit_range = CommitRange::new(
            (interval_num * interval_size + 1)..=(interval_num + 1) * interval_size,
        );
        ReputationScores::new(commit_range, self.total_scores_per_authority.clone())
    }

    fn report_metrics(&self) {
        let metrics = &self.context.metrics.node_metrics;
        for (i, score) in self.total_scores_per_authority.iter().enumerate() {
            let authority_index = self
                .context
                .committee
                .to_authority_index(i)
                .expect("Should be a valid AuthorityIndex");
            let hostname = &self.context.committee.authority(authority_index).hostname;
            if hostname.is_empty() {
                continue;
            }
            metrics
                .leader_schedule_total_scores
                .with_label_values(&[hostname])
                .set(*score as i64);
            // Multiply at report time so the field stays raw `Σ stake(leaders)`
            // and the metric formula divides stake² (numerator) by stake²
            // (denominator) — i.e. a fraction with no stake dimension.
            let denominator = self
                .total_leader_stakes
                .checked_mul(self.context.committee.total_stake())
                .unwrap();
            let normalized = if denominator == 0 {
                0.0
            } else {
                *score as f64 / denominator as f64
            };
            metrics
                .leader_schedule_normalized_scores
                .with_label_values(&[hostname])
                .set(normalized);
        }
        if let Some(last) = self.scores_entries.back() {
            metrics
                .leader_schedule_last_num_leaders
                .set(last.num_leaders as i64);
            metrics
                .leader_schedule_average_num_leaders
                .set(self.average_num_leaders());
        }
    }

    fn window_size(&self) -> usize {
        self.context.protocol_config.leader_schedule_window_size() as usize
    }

    /// Average number of leader-round blocks per scored commit in the current
    /// running window. Returns 0.0 when no commits have been scored yet.
    fn average_num_leaders(&self) -> f64 {
        if self.scores_entries.is_empty() {
            0.0
        } else {
            self.total_num_leaders as f64 / self.scores_entries.len() as f64
        }
    }

    /// Running sum of `Σ stake(leader_authors)` across the scored commits in
    /// the current window.
    #[cfg(test)]
    pub(crate) fn total_leader_stakes(&self) -> u64 {
        self.total_leader_stakes
    }

    #[cfg(test)]
    pub(crate) fn total_scores_per_authority(&self) -> &[u64] {
        &self.total_scores_per_authority
    }

    #[cfg(test)]
    pub(crate) fn scores_entries_len(&self) -> usize {
        self.scores_entries.len()
    }

    #[cfg(test)]
    pub(crate) fn pending_commits_len(&self) -> usize {
        self.pending_commits.len()
    }
}

/// This is used by the commit rule to figure out the next commit leaders.
#[derive(Clone, Debug, Default)]
pub(crate) struct NextCommitLeaderSchedule {
    // Index of the next commit.
    pub(crate) next_commit_index: CommitIndex,
    // Informs the committer about the minimum round of the next commit leader.
    pub(crate) min_next_leader_round: Round,
    // Number of leaders to select per round.
    pub(crate) num_leaders: usize,
    // Authorities allowed to be leaders.
    pub(crate) allowed_leaders: Vec<AuthorityIndex>,
}

struct ScoresEntry {
    // CommitRef of the oldest pending commit at the time the entry was produced.
    commit_ref: CommitRef,
    // Per-authority incremental score indexed by AuthorityIndex.
    // Retained so eviction can subtract from `total_scores_per_authority`.
    score_contributions: Vec<u64>,
    // Number of leader blocks in the commit.
    // Retained so eviction can subtract from `total_num_leaders`.
    num_leaders: usize,
    // `Σ stake(leader_authors)` from this commit's leaders.
    // Retained so eviction can subtract from `total_leader_stakes`.
    leader_stakes: u64,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use consensus_config::{AuthorityIndex, local_committee_and_keys};
    use consensus_types::block::{BlockRef, Round};

    use super::*;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        commit::CommitDigest,
        context::Context,
    };

    fn setup(committee_size: usize) -> Arc<Context> {
        setup_with_window_size(committee_size, None)
    }

    fn setup_with_window_size(committee_size: usize, window_size: Option<u32>) -> Arc<Context> {
        let (mut context, _) = Context::new_for_test(committee_size);
        if let Some(len) = window_size {
            context
                .protocol_config
                .set_leader_schedule_window_size_for_testing(len);
        }
        Arc::new(context)
    }

    /// Builds a VerifiedBlock at (round, author) with the given ancestor refs.
    fn make_block(round: Round, author: u32, ancestors: Vec<BlockRef>) -> VerifiedBlock {
        VerifiedBlock::new_for_test(
            TestBlock::new(round, author)
                .set_ancestors(ancestors)
                .build(),
        )
    }

    fn make_commit(
        index: CommitIndex,
        leader: BlockRef,
        blocks: Vec<VerifiedBlock>,
    ) -> CommittedSubDag {
        CommittedSubDag::new(
            leader,
            blocks,
            /* timestamp_ms */ 0,
            CommitRef {
                index,
                digest: CommitDigest::MIN,
            },
        )
    }

    fn make_uniform_chain_commits(last_index: CommitIndex) -> Vec<CommittedSubDag> {
        assert!(last_index > 0);
        let mut blocks_by_round: Vec<Vec<VerifiedBlock>> = Vec::new();
        blocks_by_round.push(vec![make_block(1, 0, vec![]), make_block(1, 1, vec![])]);
        for round in 2..=last_index {
            let prev = &blocks_by_round[(round - 2) as usize];
            let ancestors: Vec<_> = prev.iter().map(|b| b.reference()).collect();
            blocks_by_round.push(vec![
                make_block(round, 0, ancestors.clone()),
                make_block(round, 1, ancestors),
            ]);
        }

        (1..=last_index)
            .map(|i| {
                let blocks = blocks_by_round[(i - 1) as usize].clone();
                let leader = blocks[0].reference();
                make_commit(i, leader, blocks)
            })
            .collect()
    }

    fn assert_current_schedule_matches_computed(schedule: &LeaderScheduleV3) {
        let current = schedule.next_commit_leader_schedule();
        let computed = schedule.compute_next_commit_leader_schedule();
        assert_eq!(current.next_commit_index, computed.next_commit_index);
        assert_eq!(
            current.min_next_leader_round,
            computed.min_next_leader_round
        );
        assert_eq!(current.num_leaders, computed.num_leaders);
        assert_eq!(current.allowed_leaders, computed.allowed_leaders);
    }

    fn assert_current_reputation_range(
        schedule: &LeaderScheduleV3,
        start: CommitIndex,
        end: CommitIndex,
    ) {
        let scores = schedule.current_reputation_scores();
        assert_eq!(scores.commit_range, CommitRange::new(start..=end));
        assert_eq!(
            scores.scores_per_authority.as_slice(),
            schedule.total_scores_per_authority()
        );
    }

    #[tokio::test]
    async fn test_new_state_is_zero() {
        let context = setup(4);
        let schedule = LeaderScheduleV3::new(context, vec![0; 4]);
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entries_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 0);
        assert_eq!(schedule.average_num_leaders(), 0.0);
        assert_eq!(schedule.total_leader_stakes(), 0);
    }

    #[tokio::test]
    async fn test_first_three_commits_score_zero() {
        // Scoring logic needs C-3, C-2, C-1 in pending before producing the
        // first score entry, so the first three commits are warmup.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context, vec![0; 4]);

        let leader1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, leader1.reference(), vec![leader1.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entries_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 1);

        let leader2 = make_block(2, 1, vec![leader1.reference()]);
        schedule.add_commit(make_commit(2, leader2.reference(), vec![leader2.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entries_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 2);

        let leader3 = make_block(3, 2, vec![leader2.reference()]);
        schedule.add_commit(make_commit(3, leader3.reference(), vec![leader3.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entries_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 3);
    }

    #[tokio::test]
    async fn test_fourth_commit_scores_from_c_minus_3() {
        // 4 commits with strictly increasing leader rounds (1, 2, 3, 4).
        // C1 (the C-3-equivalent when scoring on C4) carries multiple round-1
        // leader blocks, exercising "leader_refs = all round-r blocks in C-3".
        // Voting blocks live at round 2 in C2, certifying blocks at round 3 in
        // C3, and C4 acts as the > r+2 sentinel.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        // C1: leader = L0_1 (auth 0); blocks include L1_1 too, so leader_refs
        // for C-3 = {L0_1, L1_1}.
        let l0_1 = make_block(1, 0, vec![]);
        let l1_1 = make_block(1, 1, vec![]);
        schedule.add_commit(make_commit(
            1,
            l0_1.reference(),
            vec![l0_1.clone(), l1_1.clone()],
        ));

        // C2: round-2 voting blocks for authorities 0 and 2.
        //   v0 (auth 0) links L0_1 only.
        //   v2 (auth 2) links L0_1 and L1_1.
        let v0 = make_block(2, 0, vec![l0_1.reference()]);
        let v2 = make_block(2, 2, vec![l0_1.reference(), l1_1.reference()]);
        schedule.add_commit(make_commit(2, v0.reference(), vec![v0.clone(), v2.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);

        // C3: round-3 certifying block q1 (auth 1) certifies v0 and v2.
        let q1 = make_block(3, 1, vec![v0.reference(), v2.reference()]);
        schedule.add_commit(make_commit(3, q1.reference(), vec![q1.clone()]));

        // C4: leader at round 4. Add a round-3 block q3 (auth 3) certifying v0
        // only — exercises "C4 (the inclusive r+2 sentinel) is searched too".
        let q3 = make_block(3, 3, vec![v0.reference()]);
        let lead4 = make_block(4, 0, vec![q1.reference()]);
        schedule.add_commit(make_commit(
            4,
            lead4.reference(),
            vec![lead4.clone(), q3.clone()],
        ));

        // Expected:
        //   A=0: voting={v0}, voted_for_stake = s0 (links L0_1).
        //        certifiers: q1 has v0 ancestor → qs={1}; q3 has v0 ancestor → qs={1,3}.
        //        certified_by_stake = s1 + s3.   score[0] = s0 * (s1 + s3).
        //   A=2: voting={v2}, voted_for_stake = s0 + s1 (links L0_1, L1_1).
        //        certifiers: q1 has v2 → qs={1}; q3 doesn't → qs stays {1}.
        //        certified_by_stake = s1.        score[2] = (s0 + s1) * s1.
        //   A=1, A=3: no round-2 voting block → 0.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let s3 = context.committee.stake(AuthorityIndex::new_for_test(3));
        assert_eq!(
            schedule.total_scores_per_authority(),
            &[s0 * (s1 + s3), 0, (s0 + s1) * s1, 0]
        );
        assert_eq!(schedule.scores_entries_len(), 1);
        assert_eq!(schedule.pending_commits_len(), 3);
        // C-3 (commit 1) has 2 leader-round blocks (auths 0 and 1) → average
        // num_leaders=2, single scored entry contributes (s0 + s1) to the
        // running total leader-stake.
        assert_eq!(schedule.average_num_leaders(), 2.0);
        assert_eq!(schedule.total_leader_stakes(), s0 + s1);
    }

    #[tokio::test]
    async fn test_eviction_subtracts_contributions() {
        // Uniform chain: rounds 1..=8, two authorities (0 and 1) producing
        // blocks at each round and linking to both of the previous round's
        // blocks. With `window_size=3` and the new rule, the first score
        // entry lands when commit 4 is added, the window fills at commit 6,
        // and commit 7 evicts the earliest entry. Because every scored commit
        // contributes an identical delta, post-eviction totals equal
        // pre-eviction totals — proving eviction is subtracting correctly.
        let context = setup_with_window_size(4, Some(3));
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let mut blocks_by_round: Vec<Vec<VerifiedBlock>> = Vec::new();
        blocks_by_round.push(vec![make_block(1, 0, vec![]), make_block(1, 1, vec![])]);
        for round in 2..=7u32 {
            let prev = &blocks_by_round[(round - 2) as usize];
            let ancestors: Vec<_> = prev.iter().map(|b| b.reference()).collect();
            blocks_by_round.push(vec![
                make_block(round, 0, ancestors.clone()),
                make_block(round, 1, ancestors),
            ]);
        }

        let commit = |i: CommitIndex, brs: &[Vec<VerifiedBlock>]| -> CommittedSubDag {
            let blocks = brs[(i - 1) as usize].clone();
            let leader = blocks[0].reference();
            make_commit(i, leader, blocks)
        };

        for i in 1..=6 {
            schedule.add_commit(commit(i, &blocks_by_round));
        }
        let after_6 = schedule.total_scores_per_authority().to_vec();
        // Window full: scored entries for C1, C2, C3.
        assert_eq!(schedule.scores_entries_len(), 3);

        // Per-commit contribution to authority A in this uniform chain:
        //   voted_for_stake[A]    = s0 + s1 (A's r+1 voting block links both leaders),
        //   certified_by_stake[A] = s0 + s1 (both r+2 blocks certify A's voting block).
        // → per_commit = (s0 + s1)^2 for authorities 0 and 1; 0 for 2 and 3.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let per_commit = (s0 + s1) * (s0 + s1);
        assert_eq!(after_6, vec![3 * per_commit, 3 * per_commit, 0, 0]);

        schedule.add_commit(commit(7, &blocks_by_round));
        assert_eq!(schedule.scores_entries_len(), 3);
        // C1's entry was evicted and C4's entry added; identical deltas leave
        // totals unchanged. Had eviction not subtracted, totals would be
        // 4 * per_commit.
        assert_eq!(schedule.total_scores_per_authority().to_vec(), after_6);

        // Each scored C-3 has exactly 2 leader-round blocks from auths 0/1;
        // average_num_leaders=2.0 and the running leader-stake total is
        // 3 entries × (s0 + s1) per commit.
        assert_eq!(schedule.average_num_leaders(), 2.0);
        assert_eq!(schedule.total_leader_stakes(), 3 * (s0 + s1));
    }

    #[tokio::test]
    async fn test_select_returns_next_commit_index_and_requested_count() {
        // With the default threshold disabled, no authority is "bad", so the
        // full randomized order is available and `num_leaders` equals the
        // committee size. With no pending commits, the returned CommitIndex
        // is the default 1.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(0);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context, vec![5, 10, 0, 2]);
        let next = schedule.compute_next_commit_leader_schedule();
        assert_eq!(next.next_commit_index, 1);
        // No pending commits -> min_next_leader_round defaults to 1.
        assert_eq!(next.min_next_leader_round, 1);
        // Threshold is 0, so every authority is allowed.
        assert_eq!(next.allowed_leaders.len(), 4);
        assert_eq!(next.num_leaders, next.allowed_leaders.len());
        // The allowed-leaders list is a permutation of distinct authorities.
        let mut sorted = next.allowed_leaders.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), next.allowed_leaders.len());
    }

    #[tokio::test]
    async fn test_select_allowed_leaders_threshold_cases() {
        struct Case {
            name: &'static str,
            stakes: Vec<u64>,
            scores: Vec<u64>,
            threshold: u64,
            expected_len: usize,
            excluded: Vec<u32>,
            exact_allowed: Option<Vec<u32>>,
        }

        let cases = [
            Case {
                name: "filters single lowest equal-stake authority",
                stakes: vec![1; 4],
                scores: vec![100, 0, 100, 100],
                threshold: 30,
                expected_len: 3,
                excluded: vec![1],
                exact_allowed: None,
            },
            Case {
                name: "does not exceed stake threshold",
                stakes: vec![2500; 4],
                scores: vec![10, 0, 1, 2],
                threshold: 30,
                expected_len: 3,
                excluded: vec![1],
                exact_allowed: None,
            },
            Case {
                name: "only highest score remains",
                stakes: vec![1; 4],
                scores: vec![10, 0, 1, 2],
                threshold: 80,
                expected_len: 1,
                excluded: vec![1, 2, 3],
                exact_allowed: Some(vec![0]),
            },
        ];

        for case in cases {
            let (mut ctx, _) = Context::new_for_test(case.stakes.len());
            let (committee, _) = local_committee_and_keys(0, case.stakes);
            ctx = ctx.with_committee(committee);
            ctx.protocol_config
                .set_bad_nodes_stake_threshold_for_testing(case.threshold);
            let context = Arc::new(ctx);
            let schedule = LeaderScheduleV3::new(context, case.scores);

            let allowed = schedule.select_allowed_leaders_with_fixed_config();
            assert_eq!(allowed.len(), case.expected_len, "{}", case.name);
            for excluded in case.excluded {
                assert!(
                    !allowed.contains(&AuthorityIndex::new_for_test(excluded)),
                    "{}: unexpectedly allowed authority {excluded}",
                    case.name
                );
            }
            if let Some(exact_allowed) = case.exact_allowed {
                let expected = exact_allowed
                    .into_iter()
                    .map(AuthorityIndex::new_for_test)
                    .collect::<Vec<_>>();
                assert_eq!(allowed, expected, "{}", case.name);
            }
        }
    }

    #[tokio::test]
    async fn test_select_allowed_leaders_seed_varies_per_epoch_at_init() {
        use std::collections::HashSet;

        // At epoch start every authority's score is zero. The bad-nodes 30% cutoff
        // with equal stakes filters exactly one of the tied authorities; which one
        // depends on the shuffle seed. The same authority shouldn't be permanently filtered.
        let zero_scores = vec![0u64; 4];
        let mut allowed_sets: HashSet<Vec<AuthorityIndex>> = HashSet::new();
        for &(epoch, ts) in &[(0u64, 0u64), (1, 1_000), (2, 2_000), (5, 5_000)] {
            let (committee, _) = local_committee_and_keys(epoch, vec![1, 1, 1, 1]);
            let (mut ctx, _) = Context::new_for_test(4);
            ctx = ctx
                .with_committee(committee)
                .with_epoch_start_timestamp_ms(ts);
            ctx.protocol_config
                .set_bad_nodes_stake_threshold_for_testing(30);
            let context = Arc::new(ctx);
            let schedule = LeaderScheduleV3::new(context, zero_scores.clone());
            let allowed = schedule.select_allowed_leaders_with_fixed_config();
            // Sanity: 30% cutoff filters exactly one of four equal-stake authorities.
            assert_eq!(allowed.len(), 3);
            allowed_sets.insert(allowed);
        }
        assert!(
            allowed_sets.len() >= 2,
            "expected per-epoch seed variance to produce at least 2 distinct \
             allowed-leader outputs across (epoch, timestamp) pairs, \
             got {}: {:?}",
            allowed_sets.len(),
            allowed_sets
        );
    }

    #[tokio::test]
    async fn test_replay_length() {
        // Replay covers scored entries, pending commits, and enough additional
        // history to reconstruct the current schedule at the last rotation.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_window_size_for_testing(5);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(12);
        let context = Arc::new(ctx);
        assert_eq!(LeaderScheduleV3::replay_length(&context), 19);

        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_window_size_for_testing(5);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(0);
        let context = Arc::new(ctx);
        assert_eq!(LeaderScheduleV3::replay_length(&context), 8);
    }

    #[tokio::test]
    async fn test_current_reputation_scores_uses_effective_interval_one_for_zero() {
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(0);
        let context = Arc::new(ctx);
        let mut schedule = LeaderScheduleV3::new(context, vec![1, 2, 3, 4]);
        let commits = make_uniform_chain_commits(2);

        assert_current_reputation_range(&schedule, 1, 1);
        schedule.add_commit(commits[0].clone());
        assert_current_reputation_range(&schedule, 2, 2);
        schedule.add_commit(commits[1].clone());
        assert_current_reputation_range(&schedule, 3, 3);
    }

    #[tokio::test]
    async fn test_current_reputation_scores_interval_boundaries() {
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(12);
        let context = Arc::new(ctx);
        let mut schedule = LeaderScheduleV3::new(context, vec![1, 2, 3, 4]);
        let commits = make_uniform_chain_commits(12);

        assert_eq!(schedule.next_commit_leader_schedule().next_commit_index, 1);
        assert_current_reputation_range(&schedule, 1, 12);

        for commit in commits.iter().take(11) {
            schedule.add_commit(commit.clone());
        }
        assert_eq!(schedule.next_commit_leader_schedule().next_commit_index, 12);
        assert_current_reputation_range(&schedule, 1, 12);

        schedule.add_commit(commits[11].clone());
        assert_eq!(schedule.next_commit_leader_schedule().next_commit_index, 13);
        assert_current_reputation_range(&schedule, 13, 24);
    }

    #[tokio::test]
    async fn test_recovery_replay_produces_same_state_as_live() {
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_window_size_for_testing(5);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(12);
        let context = Arc::new(ctx);

        let commits = make_uniform_chain_commits(23);

        // Live path: feed all commits into a fresh schedule starting at index 1.
        let mut live = LeaderScheduleV3::new(context.clone(), vec![0; 4]);
        for commit in &commits {
            live.add_commit(commit.clone());
        }
        assert_eq!(live.scores_entries_len(), 5);
        assert_eq!(live.pending_commits_len(), 3);

        let replay_count = LeaderScheduleV3::replay_length(&context);
        assert_eq!(replay_count, 19);
        let last_commit_index = commits.last().unwrap().commit_ref.index;
        let replay_start = last_commit_index.saturating_sub(replay_count) + 1;
        assert_eq!(replay_start, 5);
        let mut replayed = LeaderScheduleV3::new(context.clone(), vec![0; 4]);
        for commit in commits.iter().skip((replay_start - 1) as usize).cloned() {
            replayed.add_commit(commit);
        }

        // The two schedules must be indistinguishable from the outside.
        assert_eq!(
            live.total_scores_per_authority(),
            replayed.total_scores_per_authority()
        );
        assert_eq!(live.scores_entries_len(), replayed.scores_entries_len());
        assert_eq!(live.pending_commits_len(), replayed.pending_commits_len());
        assert_eq!(live.average_num_leaders(), replayed.average_num_leaders());
        assert_eq!(live.total_leader_stakes(), replayed.total_leader_stakes());
        // Selection is also seeded off the same pending commit, so the
        // authority ordering must match.
        let live_next = live.next_commit_leader_schedule();
        let replay_next = replayed.next_commit_leader_schedule();
        assert_eq!(live_next.next_commit_index, replay_next.next_commit_index);
        assert_eq!(
            live_next.min_next_leader_round,
            replay_next.min_next_leader_round
        );
        assert_eq!(live_next.num_leaders, replay_next.num_leaders);
        assert_eq!(live_next.allowed_leaders, replay_next.allowed_leaders);
        assert_eq!(live_next.next_commit_index, 24);
    }

    #[tokio::test]
    async fn test_select_is_deterministic_for_same_commit_index() {
        // Same state + same next_commit_index produces the same ordering on
        // every call.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_num_leaders_per_round_for_testing(Some(4));
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(0);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context, vec![1, 2, 3, 4]);
        let first = schedule.compute_next_commit_leader_schedule();
        let second = schedule.compute_next_commit_leader_schedule();
        assert_eq!(first.allowed_leaders, second.allowed_leaders);
    }

    #[tokio::test]
    async fn test_schedule_rotates_on_interval_boundary() {
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(12);
        let context = Arc::new(ctx);
        let mut schedule = LeaderScheduleV3::new(context, vec![0; 4]);
        let commits = make_uniform_chain_commits(12);

        let initial = schedule.next_commit_leader_schedule();
        assert_eq!(initial.next_commit_index, 1);
        for commit in commits.iter().take(11) {
            schedule.add_commit(commit.clone());
        }
        let before_boundary = schedule.next_commit_leader_schedule();
        let computed_before_boundary = schedule.compute_next_commit_leader_schedule();
        assert_eq!(before_boundary.next_commit_index, 12);
        assert_eq!(
            before_boundary.min_next_leader_round,
            computed_before_boundary.min_next_leader_round
        );
        assert_eq!(before_boundary.num_leaders, initial.num_leaders);
        assert_eq!(before_boundary.allowed_leaders, initial.allowed_leaders);

        schedule.add_commit(commits[11].clone());
        assert_eq!(schedule.next_commit_leader_schedule().next_commit_index, 13);
        // On boundary: cached schedule was just fully recomputed.
        assert_current_schedule_matches_computed(&schedule);
    }

    #[tokio::test]
    async fn test_effective_interval_one_refreshes_full_schedule_every_commit() {
        for interval in [0, 1] {
            let (mut ctx, _) = Context::new_for_test(4);
            ctx.protocol_config
                .set_leader_schedule_update_interval_for_testing(interval);
            let context = Arc::new(ctx);
            let mut schedule = LeaderScheduleV3::new(context, vec![0; 4]);
            let commits = make_uniform_chain_commits(4);

            assert_current_schedule_matches_computed(&schedule);
            for commit in commits {
                schedule.add_commit(commit);
                assert_current_schedule_matches_computed(&schedule);
            }
        }
    }

    #[tokio::test]
    async fn test_recovery_matches_live_before_first_rotation_boundary() {
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_update_interval_for_testing(12);
        let context = Arc::new(ctx);
        let commits = make_uniform_chain_commits(5);

        let mut live = LeaderScheduleV3::new(context.clone(), vec![0; 4]);
        for commit in &commits {
            live.add_commit(commit.clone());
        }

        let replay_count = LeaderScheduleV3::replay_length(&context);
        let last_commit_index = commits.last().unwrap().commit_ref.index;
        let replay_start = last_commit_index.saturating_sub(replay_count) + 1;
        assert_eq!(replay_start, 1);
        let mut replayed = LeaderScheduleV3::new(context, vec![0; 4]);
        for commit in commits {
            replayed.add_commit(commit);
        }

        assert_eq!(live.next_commit_leader_schedule().next_commit_index, 6);
        assert_eq!(
            live.next_commit_leader_schedule().next_commit_index,
            replayed.next_commit_leader_schedule().next_commit_index
        );
    }

    #[tokio::test]
    async fn test_authority_with_no_votes_scores_zero() {
        // Score-zero comes from two distinct paths under the new rule:
        //   (a) no r+1 voting block from A → A skipped entirely.
        //   (b) A has a voting block but no r+2 block certifies it →
        //       certified_by_stake = 0, multiplicative score = 0.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        // C1: round-1 leader blocks for authorities 0 and 1.
        let l0_1 = make_block(1, 0, vec![]);
        let l1_1 = make_block(1, 1, vec![]);
        schedule.add_commit(make_commit(
            1,
            l0_1.reference(),
            vec![l0_1.clone(), l1_1.clone()],
        ));

        // C2: round-2 voting blocks from authorities 0 and 1.
        //   v0 links L0_1, v1 links L1_1.
        let v0 = make_block(2, 0, vec![l0_1.reference()]);
        let v1 = make_block(2, 1, vec![l1_1.reference()]);
        schedule.add_commit(make_commit(2, v0.reference(), vec![v0.clone(), v1.clone()]));

        // C3: round-3 certifying block q_to_v0 certifies v0 only — leaves v1
        // uncertified.
        let q_to_v0 = make_block(3, 1, vec![v0.reference()]);
        schedule.add_commit(make_commit(3, q_to_v0.reference(), vec![q_to_v0.clone()]));

        // C4: round-4 leader (the > r+2 sentinel for scoring C1).
        let l0_4 = make_block(4, 0, vec![q_to_v0.reference()]);
        schedule.add_commit(make_commit(4, l0_4.reference(), vec![l0_4.clone()]));

        // Expected:
        //   A=0: voting={v0}, voted_for_stake=s0; certifiers from q_to_v0 → qs={1},
        //        certified_by_stake=s1.   score[0] = s0 * s1.
        //   A=1: voting={v1}, voted_for_stake=s1; nothing certifies v1 →
        //        certified_by_stake=0. score[1] = 0 (multiplicative-zero arm).
        //   A=2, A=3: no voting block → 0 (skipped arm).
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        assert_eq!(schedule.total_scores_per_authority()[0], s0 * s1);
        assert_eq!(schedule.total_scores_per_authority()[1], 0);
        assert_eq!(schedule.total_scores_per_authority()[2], 0);
        assert_eq!(schedule.total_scores_per_authority()[3], 0);
    }

    #[tokio::test]
    async fn test_voting_scan_excludes_blocks_past_sentinel() {
        // 4 commits with strictly increasing leader rounds (1, 2, 3, 4).
        // When scoring C1: vote_round=2, certify_round=3.
        //   voting_commits    scan stops at C3 (leader.round=3 > 2): [C2, C3].
        //   certifying_commits scan stops at C4 (leader.round=4 > 3): [C2, C3, C4].
        //
        // Plant a round-2 voting block in C3 (inside the voting range) AND a
        // sibling round-2 block from the same author L1_2_in_C4 in C4
        // (outside the voting range). If the bound were broken and C4 were
        // scanned for r+1, auth 1 would have two voting blocks and score 0
        // by equivocation. Correct behavior: C4 isn't in the voting range, so
        // auth 1 has exactly one voting block (in C3) and score is nonzero.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        let l0_2 = make_block(2, 0, vec![l0_1.reference()]);
        schedule.add_commit(make_commit(2, l0_2.reference(), vec![l0_2.clone()]));

        // C3 leader at round 3; pack a forged round-2 voting block from auth 1.
        let l1_2_in_c3 = make_block(2, 1, vec![l0_1.reference()]);
        let l0_3 = make_block(3, 0, vec![l0_2.reference(), l1_2_in_c3.reference()]);
        schedule.add_commit(make_commit(
            3,
            l0_3.reference(),
            vec![l0_3.clone(), l1_2_in_c3.clone()],
        ));

        // C4 leader at round 4. Pack ANOTHER round-2 block from auth 1 here —
        // this one must NOT be scanned for r+1.
        let l1_2_in_c4 = make_block(2, 1, vec![]);
        let l0_4 = make_block(4, 0, vec![l0_3.reference()]);
        schedule.add_commit(make_commit(
            4,
            l0_4.reference(),
            vec![l0_4.clone(), l1_2_in_c4.clone()],
        ));

        // Expected (correct bound):
        //   voting_blocks_by_author = {0: [l0_2], 1: [l1_2_in_c3]}.
        //   A=0: voted_for_stake=s0, certifiers: l0_3 has l0_2 ancestor → qs={0},
        //        certified_by_stake=s0. score[0] = s0*s0.
        //   A=1: voted_for_stake=s0 (l1_2_in_c3 → l0_1), certifiers: l0_3 has
        //        l1_2_in_c3 ancestor → qs={0}. certified_by_stake=s0.
        //        score[1] = s0*s0.
        // If C4 were scanned for r+1, auth 1 would have two voting blocks → 0.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        assert_eq!(schedule.total_scores_per_authority()[0], s0 * s0);
        assert_eq!(schedule.total_scores_per_authority()[1], s0 * s0);
        assert_eq!(schedule.total_scores_per_authority()[2], 0);
        assert_eq!(schedule.total_scores_per_authority()[3], 0);
    }

    #[tokio::test]
    async fn test_certifying_scan_includes_sentinel_block() {
        // 4 commits with leader rounds 1, 2, 3, 4. When scoring C1, the
        // certifying scan range is [C2, C3, C4] — C4 is the >r+2=3 inclusive
        // sentinel. Place a round-3 block in C4 (as ancestor of C4's round-4
        // leader) and verify it contributes to certified_by_stake.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        let l0_2 = make_block(2, 0, vec![l0_1.reference()]);
        schedule.add_commit(make_commit(2, l0_2.reference(), vec![l0_2.clone()]));

        let l0_3 = make_block(3, 0, vec![l0_2.reference()]);
        schedule.add_commit(make_commit(3, l0_3.reference(), vec![l0_3.clone()]));

        // C4: leader at round 4, plus a round-3 block from auth 2 carried as
        // an ancestor. The round-3 block certifies l0_2, so it must show up
        // in certified_by_stake[0] alongside l0_3.
        let late_3 = make_block(3, 2, vec![l0_2.reference()]);
        let l0_4 = make_block(4, 0, vec![l0_3.reference(), late_3.reference()]);
        schedule.add_commit(make_commit(
            4,
            l0_4.reference(),
            vec![l0_4.clone(), late_3.clone()],
        ));

        // Expected:
        //   A=0: voting={l0_2}, voted_for_stake=s0.
        //        certifying blocks at round 3 in [C2, C3, C4] = {l0_3, late_3};
        //        both certify l0_2 → qs={0, 2}, certified_by_stake = s0 + s2.
        //        score[0] = s0 * (s0 + s2).
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s2 = context.committee.stake(AuthorityIndex::new_for_test(2));
        assert_eq!(schedule.total_scores_per_authority()[0], s0 * (s0 + s2));
    }

    #[tokio::test]
    async fn test_round_skip_collapses_voting_range() {
        // 4 commits with leader rounds 1, 3, 4, 5 (round 2 skipped). When
        // scoring C1: r=1, vote_round=2, certify_round=3.
        //   voting_commits:    starts at C2 (leader.round=3 > 2) → range=[C2].
        //   certifying_commits: C2 (3) ≤ 3 ok; C3 (4) > 3 stop → range=[C2, C3].
        //
        // Plant a round-2 block in C3 (auth 2) — outside the voting range,
        // must NOT contribute. l0_3 (in C2) links to it so that, IF the bound
        // were broken and the block were scanned for r+1, auth 2 would
        // accrue a nonzero score.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        // C2: leader at round 3 (skipping round 2). Pack a round-2 voting
        // block from auth 1 (inside the voting range).
        let l1_2 = make_block(2, 1, vec![l0_1.reference()]);
        let l2_2_in_c3 = make_block(2, 2, vec![l0_1.reference()]);
        let l0_3 = make_block(
            3,
            0,
            vec![l1_2.reference(), l2_2_in_c3.reference(), l0_1.reference()],
        );
        schedule.add_commit(make_commit(
            2,
            l0_3.reference(),
            vec![l0_3.clone(), l1_2.clone()],
        ));

        // C3: leader at round 4. Pack the round-2 block from auth 2 here —
        // outside the voting range; must be ignored.
        let l0_4 = make_block(4, 0, vec![l0_3.reference()]);
        schedule.add_commit(make_commit(
            3,
            l0_4.reference(),
            vec![l0_4.clone(), l2_2_in_c3.clone()],
        ));

        // C4: leader at round 5.
        let l0_5 = make_block(5, 0, vec![l0_4.reference()]);
        schedule.add_commit(make_commit(4, l0_5.reference(), vec![l0_5.clone()]));

        // Expected:
        //   A=1: voted_for_stake=s0 (l1_2 → l0_1). certifiers: l0_3 has l1_2
        //        ancestor → qs={0}. certified_by_stake=s0. score[1] = s0*s0.
        //   A=2: NOT scanned (l2_2_in_c3 is in C3, which is not in voting range).
        //        score[2] = 0. If the bound were broken, score[2] would be
        //        s0*s0 (l2_2_in_c3 → l0_1, and l0_3 has l2_2_in_c3 ancestor).
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        assert_eq!(schedule.total_scores_per_authority()[0], 0);
        assert_eq!(schedule.total_scores_per_authority()[1], s0 * s0);
        assert_eq!(schedule.total_scores_per_authority()[2], 0);
        assert_eq!(schedule.total_scores_per_authority()[3], 0);
    }

    #[tokio::test]
    async fn test_c_minus_3_with_multiple_round_r_leaders() {
        // C-3 (= C1) packs three round-1 leader blocks (auths 0, 1, 2), so
        // leader_refs is a multi-element set. A=3's voting block links all
        // three; voted_for_stake[3] must include all three stakes.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        let l1_1 = make_block(1, 1, vec![]);
        let l2_1 = make_block(1, 2, vec![]);
        schedule.add_commit(make_commit(
            1,
            l0_1.reference(),
            vec![l0_1.clone(), l1_1.clone(), l2_1.clone()],
        ));

        // A=3 produces the round-2 voting block, linking all three leaders.
        let v3 = make_block(
            2,
            3,
            vec![l0_1.reference(), l1_1.reference(), l2_1.reference()],
        );
        schedule.add_commit(make_commit(2, v3.reference(), vec![v3.clone()]));

        // A=0 produces a round-3 certifying block (q0) certifying v3.
        let q0 = make_block(3, 0, vec![v3.reference()]);
        schedule.add_commit(make_commit(3, q0.reference(), vec![q0.clone()]));

        let l0_4 = make_block(4, 0, vec![q0.reference()]);
        schedule.add_commit(make_commit(4, l0_4.reference(), vec![l0_4.clone()]));

        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let s2 = context.committee.stake(AuthorityIndex::new_for_test(2));
        // voted_for_stake[3] = s0 + s1 + s2; certified_by_stake[3] = s0.
        assert_eq!(
            schedule.total_scores_per_authority()[3],
            (s0 + s1 + s2) * s0
        );
    }

    #[tokio::test]
    async fn test_equivocation_zeros_voting_score() {
        // Authority 0 produces two distinct round-2 voting blocks → score=0.
        // Authority 1 produces exactly one → score follows the formula.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        // C2: pack two round-2 blocks from auth 0 (distinct ancestors → distinct
        // refs) plus one from auth 1.
        let v0_a = make_block(2, 0, vec![l0_1.reference()]);
        let v0_b = make_block(2, 0, vec![]);
        let v1 = make_block(2, 1, vec![l0_1.reference()]);
        schedule.add_commit(make_commit(
            2,
            v0_a.reference(),
            vec![v0_a.clone(), v0_b.clone(), v1.clone()],
        ));

        // C3: round-3 block from auth 0 certifying v1 (so certified_by_stake[1] > 0).
        let q = make_block(3, 0, vec![v1.reference()]);
        schedule.add_commit(make_commit(3, q.reference(), vec![q.clone()]));

        let l0_4 = make_block(4, 0, vec![q.reference()]);
        schedule.add_commit(make_commit(4, l0_4.reference(), vec![l0_4.clone()]));

        // A=0 has two round-2 blocks → equivocation → score=0.
        // A=1: voted_for_stake=s0, certified_by_stake=s0 → score=s0*s0.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        assert_eq!(schedule.total_scores_per_authority()[0], 0);
        assert_eq!(schedule.total_scores_per_authority()[1], s0 * s0);
    }

    #[tokio::test]
    async fn test_score_is_product_of_voted_for_and_certified_by_stake() {
        // 4 authorities. A=0's voting block links {auth 1, auth 2} round-1
        // leader blocks (voted_for_stake = s1 + s2); two distinct r+2 blocks
        // from auths 1 and 3 certify A=0's voting block
        // (certified_by_stake = s1 + s3). Assert the exact product.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l1_1 = make_block(1, 1, vec![]);
        let l2_1 = make_block(1, 2, vec![]);
        schedule.add_commit(make_commit(
            1,
            l1_1.reference(),
            vec![l1_1.clone(), l2_1.clone()],
        ));

        let v0 = make_block(2, 0, vec![l1_1.reference(), l2_1.reference()]);
        schedule.add_commit(make_commit(2, v0.reference(), vec![v0.clone()]));

        let q1 = make_block(3, 1, vec![v0.reference()]);
        schedule.add_commit(make_commit(3, q1.reference(), vec![q1.clone()]));

        let q3 = make_block(3, 3, vec![v0.reference()]);
        let l_4 = make_block(4, 0, vec![q1.reference(), q3.reference()]);
        schedule.add_commit(make_commit(
            4,
            l_4.reference(),
            vec![l_4.clone(), q3.clone()],
        ));

        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let s2 = context.committee.stake(AuthorityIndex::new_for_test(2));
        let s3 = context.committee.stake(AuthorityIndex::new_for_test(3));
        assert_eq!(
            schedule.total_scores_per_authority()[0],
            (s1 + s2) * (s1 + s3)
        );
    }

    #[tokio::test]
    async fn test_zero_voted_for_or_certified_by_stake_yields_zero() {
        // Two zero-product paths under one fixture:
        //   (a) A=1's voting block links no leader → voted_for_stake=0,
        //       short-circuit skip.
        //   (b) A=0's voting block links a leader, but no r+2 block certifies
        //       it → certified_by_stake=0 → multiplicative zero.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        let v0 = make_block(2, 0, vec![l0_1.reference()]);
        let v1_no_leader = make_block(2, 1, vec![]);
        schedule.add_commit(make_commit(
            2,
            v0.reference(),
            vec![v0.clone(), v1_no_leader.clone()],
        ));

        // C3: round-3 leader doesn't certify v0 (so certified_by_stake[0] = 0).
        let l0_3 = make_block(3, 0, vec![]);
        schedule.add_commit(make_commit(3, l0_3.reference(), vec![l0_3.clone()]));

        let l0_4 = make_block(4, 0, vec![l0_3.reference()]);
        schedule.add_commit(make_commit(4, l0_4.reference(), vec![l0_4.clone()]));

        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
    }

    #[tokio::test]
    async fn test_repeated_certifying_authority_counts_once() {
        // Authority 1 produces two distinct round-3 certifying blocks (in C3
        // and C4) that both certify A=0's voting block. certified_by_stake[0]
        // must include stake(1) exactly once.
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), vec![0; 4]);

        let l0_1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, l0_1.reference(), vec![l0_1.clone()]));

        let v0 = make_block(2, 0, vec![l0_1.reference()]);
        schedule.add_commit(make_commit(2, v0.reference(), vec![v0.clone()]));

        let q1_a = make_block(3, 1, vec![v0.reference()]);
        schedule.add_commit(make_commit(3, q1_a.reference(), vec![q1_a.clone()]));

        // Distinct second round-3 block from auth 1 — both certify v0,
        // distinct refs because of the extra l0_1 ancestor.
        let q1_b = make_block(3, 1, vec![v0.reference(), l0_1.reference()]);
        let l_4 = make_block(4, 0, vec![q1_a.reference(), q1_b.reference()]);
        schedule.add_commit(make_commit(
            4,
            l_4.reference(),
            vec![l_4.clone(), q1_b.clone()],
        ));

        // Both q1_a and q1_b certify v0; auth-1 hits qs twice, but the
        // BTreeSet dedups → certified_by_stake = s1 (not 2*s1).
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        assert_eq!(schedule.total_scores_per_authority()[0], s0 * s1);
    }
}

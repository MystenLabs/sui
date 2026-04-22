// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use consensus_types::block::Round;
use parking_lot::RwLock;
use rand::{SeedableRng, prelude::SliceRandom, rngs::StdRng};

use crate::{
    CommitIndex, CommitRef, CommittedSubDag, DagState,
    block::{BlockAPI, GENESIS_ROUND},
    commit::load_committed_subdag_from_store,
    context::Context,
};

/// Incremental, sliding-window leader scorer over recent commits.
///
/// On each new commit C, score contributions are computed and added to the
/// running totals. When the running window is full, the oldest commit is evicted
/// and its contributions are subtracted from the running totals.
///
/// Scoring rule for commit C (requires `C >= 3` so that C-1 and C-2 are present):
/// Let `r` be the leader round of commit C-2.
/// For each authority A: consider A's blocks at round `r+1` ("voting block") across commits
/// C and C-1.
/// Collect the set of distinct authorities B, such that at least one of A's voting blocks
/// vote to (has an ancestor that is) a B's round `r` ("leader block") block in commit C-2.
/// Then `score[A] += sum_{B in distinct set} stake(B)`.
pub(crate) struct LeaderScheduleV3 {
    context: Arc<Context>,
    next_commit_index: CommitIndex,
    total_scores_per_authority: Vec<u64>,
    // Sum of `num_leaders` across all entries in `scores_entry`.
    total_num_leaders: usize,
    // Sum of `leader_stakes` across all entries in `scores_entry`.
    total_leader_stakes: u64,

    // Holds at most the last two committed subdags, used as C-2 and C-1 when
    // scoring the next commit. Populated from CommittedSubDag directly so no
    // DagState lookup is needed.
    pending_commits: VecDeque<CommittedSubDag>,
    // One entry per scored commit still in the running window. Used to
    // subtract a commit's contributions from `total_scores_per_authority` on
    // eviction. Bounded by `leader_schedule_running_length`.
    scores_entry: VecDeque<ScoresEntry>,
}

/// This is used by the commit rule to figure out the next commit leaders.
#[allow(unused)]
pub(crate) struct NextCommitLeaderSchedule {
    // Index of the next commit.
    pub(crate) next_commit_index: CommitIndex,
    // Minimum round of the next commit leader.
    pub(crate) min_next_leader_round: Round,
    // Number of leaders to select.
    pub(crate) num_leaders: usize,
    // Allowed leaders to be selected from.
    pub(crate) allowed_leaders: Vec<AuthorityIndex>,
}

struct ScoresEntry {
    commit_ref: CommitRef,
    // Per-authority incremental score contributed by processing this commit,
    // indexed by AuthorityIndex. Retained so eviction can subtract it.
    score_contributions: Vec<u64>,
    // Number of round `r` (leader) blocks in the scored commit (C-2).
    // Retained so eviction can subtract it from `total_num_leaders`.
    num_leaders: usize,
    // Sum of stake(author) across all leader blocks in the scored commit.
    // Retained so eviction can subtract it from `total_leader_stakes`.
    leader_stakes: u64,
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
        let last_commit_index = dag_state.read().last_commit_index();
        if last_commit_index == 0 {
            return LeaderScheduleV3::new(context, 1, vec![0; committee_size]);
        }

        let replay_count = LeaderScheduleV3::replay_length(&context);
        // Replay maximum replay_count commits, and starting from commit 1 at minimum.
        let replay_start = last_commit_index.saturating_sub(replay_count) + 1;
        // The first replayed commit's index will be replay_start, so initializing next_commit_index
        // to replay_start.
        let mut leader_schedule =
            LeaderScheduleV3::new(context.clone(), replay_start, vec![0; committee_size]);
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

    pub(crate) fn new(
        context: Arc<Context>,
        next_commit_index: CommitIndex,
        total_scores_per_authority: Vec<u64>,
    ) -> Self {
        assert_eq!(total_scores_per_authority.len(), context.committee.size());
        let running_length = context.protocol_config.leader_schedule_running_length() as usize;
        Self {
            context,
            next_commit_index,
            total_scores_per_authority,
            total_num_leaders: 0,
            total_leader_stakes: 0,
            pending_commits: VecDeque::with_capacity(2),
            scores_entry: VecDeque::with_capacity(running_length),
        }
    }

    /// Number of recent committed sub-dags that must be replayed on startup
    /// to reconstruct the full state: `leader_schedule_running_length`
    /// for the scoring window plus 2 for the pending commits held before
    /// scoring begins.
    fn replay_length(context: &Context) -> u32 {
        context.protocol_config.leader_schedule_running_length() + 2
    }

    /// Returns the leader schedule for the next commit.
    ///
    /// next_commit_index and min_next_leader_round are determined from the current commit.
    /// num_leaders in next commit is a constant right now, but can become dynamic in future.
    /// Selects authorities with good enough performance and satisfying other constraints,
    /// which can become the next commit leaders.
    pub(crate) fn next_commit_leader_schedule(&self) -> NextCommitLeaderSchedule {
        let min_next_leader_round = self
            .pending_commits
            .back()
            .map(|c| c.leader.round)
            .unwrap_or(GENESIS_ROUND)
            + 1;
        let num_leaders = self
            .context
            .protocol_config
            .num_leaders_per_round()
            .unwrap_or(1);
        NextCommitLeaderSchedule {
            next_commit_index: self.next_commit_index,
            min_next_leader_round,
            num_leaders,
            allowed_leaders: self.select_allowed_leaders_with_fixed_config(),
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

        // A bad threshold of 0 will select all authorities.
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

    /// Scores a commit and adds it into the running window.
    /// Evicts the oldest scored commit and drops its scores, if the window is full.
    pub(crate) fn add_commit(&mut self, c: CommittedSubDag) {
        // Ensure commits are added in order.
        assert_eq!(c.commit_ref.index, self.next_commit_index);
        self.next_commit_index += 1;

        // There must be exactly 2 pending commits to compute the next scores.
        // Otherwise, this is a new consensus instance (new epoch or recovery).
        if self.pending_commits.len() < 2 {
            self.pending_commits.push_back(c);
            self.report_metrics();
            return;
        }

        // Keep at most `running_length - 1` commit scores so there is room to
        // push one more below.
        while self.scores_entry.len() >= self.running_length() {
            let evicted = self.scores_entry.pop_front().expect("non empty");
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

        // Compute score contributions for the new commit.
        let c_minus_2 = &self.pending_commits[0];
        let c_minus_1 = &self.pending_commits[1];

        let leader_round = c_minus_2.leader.round;
        let vote_round = leader_round + 1;

        let leader_refs: BTreeSet<_> = c_minus_2
            .blocks
            .iter()
            .filter(|b| b.round() == leader_round)
            .map(|b| b.reference())
            .collect();

        let voting_blocks: Vec<_> = c_minus_1
            .blocks
            .iter()
            .chain(c.blocks.iter())
            .filter(|b| b.round() == vote_round)
            .collect();

        let mut score_contributions = vec![0u64; self.context.committee.size()];

        if !voting_blocks.is_empty() && !leader_refs.is_empty() {
            // Collect a map from authorities voting on leader blocks, to leader authorities they voted on.
            // This consolidates votes and scores per authority.
            let mut per_authority_votes: BTreeMap<AuthorityIndex, BTreeSet<AuthorityIndex>> =
                BTreeMap::new();
            for voting_block in voting_blocks {
                let authority_votes = per_authority_votes
                    .entry(voting_block.author())
                    .or_default();
                for voted_block in voting_block.ancestors() {
                    if leader_refs.contains(voted_block) {
                        authority_votes.insert(voted_block.author);
                    }
                }
            }

            for (voting_authority, leader_authorities) in per_authority_votes {
                let total_leader_stake = leader_authorities
                    .into_iter()
                    .map(|i| self.context.committee.stake(i))
                    .sum();
                score_contributions[voting_authority.value()] = total_leader_stake;
            }
        }

        // Add the contributions to the running totals.
        for (i, delta) in score_contributions.iter().enumerate() {
            self.total_scores_per_authority[i] += *delta;
        }

        // Record this commit's scores for future eviction.
        let num_leaders = leader_refs.len();
        let leader_stakes: u64 = leader_refs
            .iter()
            .map(|r| self.context.committee.stake(r.author))
            .sum();
        self.total_num_leaders += num_leaders;
        self.total_leader_stakes += leader_stakes;
        self.scores_entry.push_back(ScoresEntry {
            commit_ref: c_minus_2.commit_ref,
            score_contributions,
            num_leaders,
            leader_stakes,
        });

        // Rotate the pending commits: drop C-2, keep C-1, add C.
        self.pending_commits.pop_front();
        self.pending_commits.push_back(c);

        self.report_metrics();
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
            let normalized = if self.total_leader_stakes == 0 {
                0.0
            } else {
                *score as f64 / self.total_leader_stakes as f64
            };
            metrics
                .leader_schedule_normalized_scores
                .with_label_values(&[hostname])
                .set(normalized);
        }
        if let Some(last) = self.scores_entry.back() {
            metrics
                .leader_schedule_last_num_leaders
                .set(last.num_leaders as i64);
        }
    }

    fn running_length(&self) -> usize {
        self.context
            .protocol_config
            .leader_schedule_running_length() as usize
    }

    /// Average number of leader-round blocks per scored commit in the current
    /// running window. Returns 0.0 when no commits have been scored yet.
    #[cfg(test)]
    fn average_num_leaders(&self) -> f64 {
        if self.scores_entry.is_empty() {
            0.0
        } else {
            self.total_num_leaders as f64 / self.scores_entry.len() as f64
        }
    }

    /// Average total leader stake per scored commit in the current running
    /// window. Returns 0.0 when no commits have been scored yet.
    #[cfg(test)]
    fn average_leader_stakes(&self) -> f64 {
        if self.scores_entry.is_empty() {
            0.0
        } else {
            self.total_leader_stakes as f64 / self.scores_entry.len() as f64
        }
    }

    #[cfg(test)]
    pub(crate) fn total_scores_per_authority(&self) -> &[u64] {
        &self.total_scores_per_authority
    }

    #[cfg(test)]
    pub(crate) fn scores_entry_len(&self) -> usize {
        self.scores_entry.len()
    }

    #[cfg(test)]
    pub(crate) fn pending_commits_len(&self) -> usize {
        self.pending_commits.len()
    }
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
        setup_with_running_length(committee_size, None)
    }

    fn setup_with_running_length(
        committee_size: usize,
        running_length: Option<u32>,
    ) -> Arc<Context> {
        let (mut context, _) = Context::new_for_test(committee_size);
        if let Some(len) = running_length {
            context
                .protocol_config
                .set_leader_schedule_running_length_for_testing(len);
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

    #[tokio::test]
    async fn test_new_state_is_zero() {
        let context = setup(4);
        let schedule = LeaderScheduleV3::new(context, 1, vec![0; 4]);
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entry_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 0);
        assert_eq!(schedule.average_num_leaders(), 0.0);
        assert_eq!(schedule.average_leader_stakes(), 0.0);
    }

    #[tokio::test]
    async fn test_first_two_commits_score_zero() {
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context, 1, vec![0; 4]);

        let leader1 = make_block(1, 0, vec![]);
        schedule.add_commit(make_commit(1, leader1.reference(), vec![leader1.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entry_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 1);

        let leader2 = make_block(2, 1, vec![leader1.reference()]);
        schedule.add_commit(make_commit(2, leader2.reference(), vec![leader2.clone()]));
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);
        assert_eq!(schedule.scores_entry_len(), 0);
        assert_eq!(schedule.pending_commits_len(), 2);
    }

    #[tokio::test]
    async fn test_third_commit_scores_from_c_minus_2() {
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), 1, vec![0; 4]);

        // Commit 1 (C-2): leader at round 1, authority 0. Blocks at round 1 from
        // all four authorities (these are B-candidates at C-2 leader round).
        let b0 = make_block(1, 0, vec![]);
        let b1 = make_block(1, 1, vec![]);
        let b2 = make_block(1, 2, vec![]);
        let b3 = make_block(1, 3, vec![]);
        schedule.add_commit(make_commit(
            1,
            b0.reference(),
            vec![b0.clone(), b1.clone(), b2.clone(), b3.clone()],
        ));

        // Commit 2 (C-1): two vote blocks at round 2 linking to commit 1 blocks.
        // Authority 0's round-2 block links to B0 and B1 at round 1.
        // Authority 1's round-2 block links to B2 only.
        let v0_c1 = make_block(2, 0, vec![b0.reference(), b1.reference()]);
        let v1_c1 = make_block(2, 1, vec![b2.reference()]);
        schedule.add_commit(make_commit(
            2,
            v0_c1.reference(),
            vec![v0_c1.clone(), v1_c1.clone()],
        ));
        // No scoring yet — we need C-2 to apply.
        assert_eq!(schedule.total_scores_per_authority(), &[0u64; 4]);

        // Commit 3 (C): one vote block at round 2 (C-2's leader round was 1, so
        // vote round = 2). Authority 2's round-2 block links to B1 and B3.
        let v2_c = make_block(2, 2, vec![b1.reference(), b3.reference()]);
        schedule.add_commit(make_commit(3, v2_c.reference(), vec![v2_c.clone()]));

        // Expected contributions (equal stake per authority in new_for_test):
        //   A=0: distinct Bs = {0, 1} → stake(0)+stake(1)
        //   A=1: distinct Bs = {2}    → stake(2)
        //   A=2: distinct Bs = {1, 3} → stake(1)+stake(3)
        //   A=3: no vote blocks at round 2 → 0
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let s2 = context.committee.stake(AuthorityIndex::new_for_test(2));
        let s3 = context.committee.stake(AuthorityIndex::new_for_test(3));
        assert_eq!(
            schedule.total_scores_per_authority(),
            &[s0 + s1, s2, s1 + s3, 0]
        );
        assert_eq!(schedule.scores_entry_len(), 1);
        assert_eq!(schedule.pending_commits_len(), 2);
        // C-2 (commit 1) has 4 leader-round blocks, so with one scored entry
        // the average is 4.0 and the average leader stake equals the sum of
        // all four authority stakes.
        assert_eq!(schedule.average_num_leaders(), 4.0);
        assert_eq!(schedule.average_leader_stakes(), (s0 + s1 + s2 + s3) as f64);
    }

    #[tokio::test]
    async fn test_eviction_subtracts_contributions() {
        // Uniform chain: rounds 1..=6, two authorities (0 and 1) at each round,
        // each round's blocks link to both blocks of the previous round.
        // With `running_length=3`, scores_entry tops out at 3 entries; the
        // 4th scored commit evicts the earliest. Because every scored commit
        // contributes the same delta, post-eviction totals equal pre-eviction
        // totals — which proves eviction is subtracting.
        let context = setup_with_running_length(4, Some(3));
        let mut schedule = LeaderScheduleV3::new(context.clone(), 1, vec![0; 4]);

        let mut blocks_by_round: Vec<Vec<VerifiedBlock>> = Vec::new();
        blocks_by_round.push(vec![make_block(1, 0, vec![]), make_block(1, 1, vec![])]);
        for round in 2..=6u32 {
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

        for i in 1..=5 {
            schedule.add_commit(commit(i, &blocks_by_round));
        }
        let after_5 = schedule.total_scores_per_authority().to_vec();
        assert_eq!(schedule.scores_entry_len(), 3);

        // 3 scored commits in a uniform chain, each contributing
        // `stake(0) + stake(1)` to authorities 0 and 1.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        let s1 = context.committee.stake(AuthorityIndex::new_for_test(1));
        let per_commit = s0 + s1;
        assert_eq!(after_5, vec![3 * per_commit, 3 * per_commit, 0, 0]);

        schedule.add_commit(commit(6, &blocks_by_round));
        assert_eq!(schedule.scores_entry_len(), 3);
        // cs3 was evicted and cs6 added; both deltas are identical, so totals
        // are unchanged. Had eviction not subtracted, totals would be 4 * per_commit.
        assert_eq!(schedule.total_scores_per_authority().to_vec(), after_5);
        // Every scored C-2 in this uniform chain has exactly 2 leader-round
        // blocks from authorities 0 and 1; both averages are unaffected by
        // eviction because each evicted entry's contributions are replaced
        // by identical ones.
        assert_eq!(schedule.average_num_leaders(), 2.0);
        assert_eq!(schedule.average_leader_stakes(), (s0 + s1) as f64);
    }

    #[tokio::test]
    async fn test_select_returns_next_commit_index_and_requested_count() {
        // With the default threshold disabled, no authority is "bad", so the
        // full randomized order is available and selection returns exactly
        // `num_leaders` authorities. The returned CommitIndex echoes the one
        // supplied to `new`.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_num_leaders_per_round_for_testing(Some(3));
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(0);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context, 7, vec![5, 10, 0, 2]);
        let next = schedule.next_commit_leader_schedule();
        assert_eq!(next.next_commit_index, 7);
        // No pending commits -> min_next_leader_round is GENESIS_ROUND + 1.
        assert_eq!(next.min_next_leader_round, 1);
        assert_eq!(next.num_leaders, 3);
        // Threshold is 0, so every authority is allowed.
        assert_eq!(next.allowed_leaders.len(), 4);
        // The allowed-leaders list is a permutation of distinct authorities.
        let mut sorted = next.allowed_leaders.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), next.allowed_leaders.len());
    }

    #[tokio::test]
    async fn test_select_skips_bad_nodes() {
        // 4 equal-stake authorities. Threshold 30% with total_stake=4 gives a
        // cutoff of 1; the single lowest-score authority (a1, score 0) is
        // filtered out of the allowed-leaders list. The remaining 3 must not
        // include it, regardless of the randomized order.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(30);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context.clone(), 1, vec![100, 0, 100, 100]);

        let allowed = schedule.select_allowed_leaders_with_fixed_config();
        assert_eq!(allowed.len(), 3);
        assert!(!allowed.contains(&AuthorityIndex::new_for_test(1)));
    }

    #[tokio::test]
    async fn test_select_bad_nodes_does_not_exceed_stake_threshold() {
        // 4 equal-stake authorities with total stake 10,000. A 30% threshold
        // permits excluding one 2,500-stake authority, but excluding the next
        // one would cross the 3,000-stake cutoff.
        let (mut ctx, _) = Context::new_for_test(4);
        let (committee, _) = local_committee_and_keys(0, vec![2500; 4]);
        ctx = ctx.with_committee(committee);
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(30);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context, 1, vec![10, 0, 1, 2]);

        let allowed = schedule.select_allowed_leaders_with_fixed_config();
        assert_eq!(allowed.len(), 3);
        assert!(!allowed.contains(&AuthorityIndex::new_for_test(1)));
        assert!(allowed.contains(&AuthorityIndex::new_for_test(2)));
    }

    #[tokio::test]
    async fn test_select_allowed_leaders_returns_only_good_stake() {
        // Threshold 80% with total_stake=4 gives a cutoff of 3; the three
        // lowest-score authorities accumulate exactly 3 stake and are filtered
        // from the allowed list. Only the single highest-score authority
        // remains.
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_bad_nodes_stake_threshold_for_testing(80);
        let context = Arc::new(ctx);
        let schedule = LeaderScheduleV3::new(context, 1, vec![10, 0, 1, 2]);

        let allowed = schedule.select_allowed_leaders_with_fixed_config();
        assert_eq!(allowed, vec![AuthorityIndex::new_for_test(0)]);
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
            let schedule = LeaderScheduleV3::new(context, 1, zero_scores.clone());
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
        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_running_length_for_testing(5);
        let context = Arc::new(ctx);
        assert_eq!(LeaderScheduleV3::replay_length(&context), 7);
    }

    #[tokio::test]
    async fn test_recovery_replay_produces_same_state_as_live() {
        // Invariant behind the `+2` recovery rule: replaying only the last
        // `running_length + 2` commits produces the same v3 state as feeding
        // the full history live, because earlier commits would have been
        // evicted anyway. We simulate it without going through storage by
        // stepping two schedules through identical `add_commit` sequences.

        let (mut ctx, _) = Context::new_for_test(4);
        ctx.protocol_config
            .set_leader_schedule_running_length_for_testing(5);
        let context = Arc::new(ctx);

        // Uniform chain: rounds 1..=10, authorities 0 and 1 produce blocks
        // at each round linking back to both of the previous round's blocks.
        let mut blocks_by_round: Vec<Vec<VerifiedBlock>> = Vec::new();
        blocks_by_round.push(vec![make_block(1, 0, vec![]), make_block(1, 1, vec![])]);
        for round in 2..=10u32 {
            let prev = &blocks_by_round[(round - 2) as usize];
            let ancestors: Vec<_> = prev.iter().map(|b| b.reference()).collect();
            blocks_by_round.push(vec![
                make_block(round, 0, ancestors.clone()),
                make_block(round, 1, ancestors),
            ]);
        }
        let commit_at = |i: CommitIndex, brs: &[Vec<VerifiedBlock>]| -> CommittedSubDag {
            let blocks = brs[(i - 1) as usize].clone();
            let leader = blocks[0].reference();
            make_commit(i, leader, blocks)
        };

        // Live path: feed commits 1..=10 into a fresh schedule starting at index 1.
        let mut live = LeaderScheduleV3::new(context.clone(), 1, vec![0; 4]);
        for i in 1..=10 {
            live.add_commit(commit_at(i, &blocks_by_round));
        }
        assert_eq!(live.scores_entry_len(), 5);
        assert_eq!(live.pending_commits_len(), 2);

        // Recovery path: replay only the last `running_length + 2 = 7`
        // commits (indices 4..=10) into a fresh schedule starting at index 4.
        let replay_count = LeaderScheduleV3::replay_length(&context);
        assert_eq!(replay_count, 7);
        let mut replayed = LeaderScheduleV3::new(context.clone(), 4, vec![0; 4]);
        for i in 4..=10 {
            replayed.add_commit(commit_at(i, &blocks_by_round));
        }

        // The two schedules must be indistinguishable from the outside.
        assert_eq!(
            live.total_scores_per_authority(),
            replayed.total_scores_per_authority()
        );
        assert_eq!(live.scores_entry_len(), replayed.scores_entry_len());
        assert_eq!(live.pending_commits_len(), replayed.pending_commits_len());
        assert_eq!(live.average_num_leaders(), replayed.average_num_leaders());
        assert_eq!(
            live.average_leader_stakes(),
            replayed.average_leader_stakes()
        );
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
        let schedule = LeaderScheduleV3::new(context, 42, vec![1, 2, 3, 4]);
        let first = schedule.next_commit_leader_schedule();
        let second = schedule.next_commit_leader_schedule();
        assert_eq!(first.allowed_leaders, second.allowed_leaders);
    }

    #[tokio::test]
    async fn test_authority_with_no_votes_scores_zero() {
        let context = setup(4);
        let mut schedule = LeaderScheduleV3::new(context.clone(), 1, vec![0; 4]);

        let b0 = make_block(1, 0, vec![]);
        let b1 = make_block(1, 1, vec![]);
        schedule.add_commit(make_commit(1, b0.reference(), vec![b0.clone(), b1.clone()]));

        // Commit 2 has only authority 0's vote block at round 2 (linking to B0).
        let v0_c1 = make_block(2, 0, vec![b0.reference()]);
        schedule.add_commit(make_commit(2, v0_c1.reference(), vec![v0_c1.clone()]));

        // Commit 3 has no round-2 vote blocks — just a round-3 leader.
        let leader3 = make_block(3, 1, vec![v0_c1.reference()]);
        schedule.add_commit(make_commit(3, leader3.reference(), vec![leader3.clone()]));

        // When processing commit 3: r = 1 (C-2's leader round), vote round = 2.
        // Vote blocks at round 2 in {C, C-1} = {v0_c1}. Authority 0 links to B0,
        // so score[0] = stake(0). Authorities 1, 2, 3 have no round-2 vote
        // blocks, so they score 0.
        let s0 = context.committee.stake(AuthorityIndex::new_for_test(0));
        assert_eq!(schedule.total_scores_per_authority()[0], s0);
        assert_eq!(schedule.total_scores_per_authority()[1], 0);
        assert_eq!(schedule.total_scores_per_authority()[2], 0);
        assert_eq!(schedule.total_scores_per_authority()[3], 0);
    }
}

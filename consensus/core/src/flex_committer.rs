// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_config::{AuthorityIndex, DIGEST_LENGTH, DefaultHashFunction, Stake};
use consensus_types::block::{BlockRef, BlockTimestampMs, Round};
use fastcrypto::hash::HashFunction as _;
use itertools::Itertools as _;
use parking_lot::RwLock;
use rand::{SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};

use crate::{
    BlockAPI, VerifiedBlock,
    block::Slot,
    commit::{Commit, CommitAPI, CommittedSubDag, Decision, LeaderStatus, TrustedCommit},
    context::Context,
    dag_state::DagState,
    leader_schedule_v3::NextCommitLeaderSchedule,
    leader_slot_decider::{INDIRECT_COMMIT_DEPTH, LeaderSlotDecider},
};

#[cfg(test)]
#[path = "tests/flex_committer_tests.rs"]
mod flex_committer_tests;

/// FlexCommitter supports committing multiple and varying number of leaders per round,
/// based on DagState and the schedule for next commit leaders.
pub(crate) struct FlexCommitter {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    slot_decider: LeaderSlotDecider,
    pending_commit_state: PendingCommitState,
}

impl FlexCommitter {
    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let slot_decider = LeaderSlotDecider::new(context.clone(), dag_state.clone());
        Self {
            context,
            dag_state,
            slot_decider,
            pending_commit_state: PendingCommitState::default(),
        }
    }

    /// Attempts to create a commit based on the current DagState.
    pub(crate) fn try_commit(
        &mut self,
        next_commit_leaders: NextCommitLeaderSchedule,
    ) -> Option<(TrustedCommit, CommittedSubDag)> {
        self.maybe_refresh_pending_commit_state(next_commit_leaders);

        self.try_direct_commit();
        if let Some(commit_leader_round) = self.find_commit_leader_round() {
            return self.build_commit(commit_leader_round);
        }

        self.try_indirect_commit();
        if let Some(commit_leader_round) = self.find_commit_leader_round() {
            return self.build_commit(commit_leader_round);
        }

        None
    }

    fn maybe_refresh_pending_commit_state(
        &mut self,
        next_commit_leaders: NextCommitLeaderSchedule,
    ) {
        let current = &self.pending_commit_state.next_commit_leaders;
        if current.next_commit_index == next_commit_leaders.next_commit_index {
            assert_eq!(
                current.min_next_leader_round, next_commit_leaders.min_next_leader_round,
                "minimum leader round must not change at the same commit index"
            );
            assert_eq!(
                current.allowed_leaders, next_commit_leaders.allowed_leaders,
                "leader schedule must not change at the same commit index"
            );
            return;
        }
        let current_commit_index = current.next_commit_index;
        let current_min_next_leader_round = current.min_next_leader_round;
        let leader_schedule_changed =
            current.allowed_leaders != next_commit_leaders.allowed_leaders;
        assert!(
            current_commit_index < next_commit_leaders.next_commit_index,
            "next_commit_index should only move forward: {} vs {}",
            current_commit_index,
            next_commit_leaders.next_commit_index
        );
        assert!(
            current_min_next_leader_round < next_commit_leaders.min_next_leader_round,
            "min_next_leader_round should only move forward: {} vs {}",
            current_min_next_leader_round,
            next_commit_leaders.min_next_leader_round
        );

        if leader_schedule_changed {
            tracing::debug!(
                "Resetting pending commit state for a leader schedule change: old_commit_index={}, new_commit_index={}, min_next_leader_round={}, num_leaders={}",
                current_commit_index,
                next_commit_leaders.next_commit_index,
                next_commit_leaders.min_next_leader_round,
                next_commit_leaders.num_leaders(),
            );
            self.pending_commit_state = PendingCommitState {
                next_commit_leaders,
                rounds: VecDeque::new(),
            };
            return;
        }

        let min_next_leader_round = next_commit_leaders.min_next_leader_round;
        while self
            .pending_commit_state
            .rounds
            .front()
            .is_some_and(|state| state.round < min_next_leader_round)
        {
            self.pending_commit_state.rounds.pop_front();
        }
        if let Some(first) = self.pending_commit_state.rounds.front() {
            assert_eq!(first.round, min_next_leader_round);
        }
        tracing::debug!(
            "Advancing pending commit state: old_commit_index={}, new_commit_index={}, min_next_leader_round={}, retained_rounds={}",
            current_commit_index,
            next_commit_leaders.next_commit_index,
            min_next_leader_round,
            self.pending_commit_state.rounds.len(),
        );
        self.pending_commit_state.next_commit_leaders = next_commit_leaders;
    }

    /// Runs the direct commit rule on every leader slot pending to be decided.
    fn try_direct_commit(&mut self) {
        let min_next_leader_round = self
            .pending_commit_state
            .next_commit_leaders
            .min_next_leader_round;
        let highest_accepted_round = self.dag_state.read().highest_accepted_round();

        for leader_round in min_next_leader_round..highest_accepted_round {
            let round_state = self
                .pending_commit_state
                .get_or_create_round_state(leader_round);
            let slots_to_decide = round_state
                .leader_slots
                .iter()
                .filter_map(|s| {
                    if matches!(s.leader_status, LeaderStatus::Undecided(_)) {
                        Some(s.slot)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for slot in slots_to_decide {
                let status = self.slot_decider.try_direct_decide(slot);
                if status.is_decided() {
                    tracing::debug!("Direct decision for slot {slot}: {status}");
                }
                round_state.update_slot_decision(slot, status, Decision::Direct);
            }
        }
    }

    fn try_indirect_commit(&mut self) {
        let min_next_leader_round = self
            .pending_commit_state
            .next_commit_leaders
            .min_next_leader_round;
        let highest_accepted_round = self.dag_state.read().highest_accepted_round();
        // Decide leaders from highest to lowest round, because lower rounds need anchor blocks in
        // higher rounds to be decided first.
        for round in (min_next_leader_round
            ..=(highest_accepted_round.saturating_sub(INDIRECT_COMMIT_DEPTH)))
            .rev()
        {
            let Some(anchor_block) = self.find_anchor_block(round + INDIRECT_COMMIT_DEPTH) else {
                // Skip when there is no anchor block for this round yet.
                continue;
            };
            self.decide_with_anchor_block(anchor_block.clone(), round);
        }
    }

    fn find_anchor_block(&self, start_round: Round) -> Option<VerifiedBlock> {
        let start_index = start_round
            - self
                .pending_commit_state
                .next_commit_leaders
                .min_next_leader_round;
        for index in start_index as usize..self.pending_commit_state.rounds.len() {
            let round_state = &self.pending_commit_state.rounds[index];
            for slot in &round_state.leader_slots {
                match &slot.leader_status {
                    // First committed block becomes the anchor block, when there is no undecided slot before it.
                    // Every validator will select the same anchor block.
                    // This rule of choosing anchor block minimizes the chance of encountering undecided slots before it.
                    LeaderStatus::Commit(block) => return Some(block.clone()),
                    // There cannot be an anchor block after an undecided.
                    LeaderStatus::Undecided(_) => return None,
                    // Continue searching for an anchor block after skipped slots.
                    LeaderStatus::Skip(_) => {}
                }
            }
        }
        None
    }

    // Finds leader slots on `decision_round` and calls indirect commit rule with `anchor_block`
    // to decide on their statuses.
    fn decide_with_anchor_block(&mut self, anchor_block: VerifiedBlock, decision_round: Round) {
        let round_state = self
            .pending_commit_state
            .get_or_create_round_state(decision_round);
        if round_state.undecided_slots.is_empty() {
            // Skip indirect commit when the round has already been fully decided.
            return;
        }

        let slots: Vec<Slot> = round_state.leader_slots.iter().map(|s| s.slot).collect();
        let statuses = self.slot_decider.try_indirect_decide(&anchor_block, &slots);
        let anchor_ref = anchor_block.reference();
        for (slot, status) in slots.into_iter().zip_eq(statuses) {
            // Only log slots the indirect rule actually decides. Slots already decided
            // by the direct rule are re-evaluated (and asserted consistent) but their
            // decision is kept, so they should not appear as indirect decisions.
            if round_state.undecided_slots.contains(&slot) {
                tracing::debug!(
                    "Indirect decision for slot {slot} using anchor {anchor_ref}: {status}"
                );
            }
            round_state.update_slot_decision(slot, status, Decision::Indirect);
        }
    }

    // The next commit leader round must have all slots decided in this and earlier rounds,
    // and at least one slot in this round committed.
    fn find_commit_leader_round(&self) -> Option<Round> {
        for round_state in &self.pending_commit_state.rounds {
            if !round_state.undecided_slots.is_empty() {
                // Found undecided slot, so this and later rounds cannot be committed yet.
                return None;
            }
            if round_state.num_committed > 0 {
                // Found a fully decided round with at least one committed leader, so it can be committed.
                return Some(round_state.round);
            }
            // This round is fully decided but has no committed leader, so it is skipped.
        }
        None
    }

    /// Reports the leader decisions of every round through `commit_leader_round`.
    ///
    /// Each decision is reported one time: the next `maybe_refresh_pending_commit_state`
    /// sets `min_next_leader_round` to `commit_leader_round + 1`, so exactly these rounds
    /// are dropped from the pending state before the next commit is built.
    ///
    /// This does not count every decision the committer makes. A decision is counted only
    /// when a commit covers its round, so these stay unreported: rounds that are decided
    /// when the node stops committing, and rounds dropped because a certified commit moved
    /// `min_next_leader_round` past them. This is accepted, because commits cover almost
    /// all leader decisions, and tracking the remainder needs bookkeeping that this metric
    /// does not justify.
    fn report_decided_prefix_metrics(&self, commit_leader_round: Round) {
        for round_state in &self.pending_commit_state.rounds {
            if round_state.round > commit_leader_round {
                break;
            }
            for leader_slot in &round_state.leader_slots {
                let (authority, outcome) = match &leader_slot.leader_status {
                    LeaderStatus::Commit(block) => (block.author(), "commit"),
                    LeaderStatus::Skip(slot) => (slot.authority, "skip"),
                    LeaderStatus::Undecided(slot) => {
                        panic!("Unexpected undecided slot {}", slot)
                    }
                };
                let decision = match leader_slot
                    .decision
                    .expect("Decided slot must have a decision")
                {
                    Decision::Direct => "direct",
                    Decision::Indirect => "indirect",
                    Decision::Certified => "certified",
                };
                let commit_type = format!("{decision}-{outcome}");
                let leader_host = &self.context.committee.authority(authority).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .committed_leaders_total
                    .with_label_values(&[leader_host, &commit_type])
                    .inc();
            }
        }
    }

    /// Builds a single commit with leaders from `commit_leader_round`.
    ///
    /// Traversal starts from all committed leaders in `commit_leader_round`,
    /// read from `pending_commit_state`. Other committed blocks are selected
    /// via DFS from the leaders. Only uncommitted blocks above GC round are selected.
    ///
    /// Committed blocks are ordered deterministically: by round ascending, then
    /// by `hash(seed || block_digest)` within a round. The last block of the
    /// sorted leader round becomes the commit's named `leader`.
    fn build_commit(
        &mut self,
        commit_leader_round: Round,
    ) -> Option<(TrustedCommit, CommittedSubDag)> {
        let committed_leaders = {
            let round_state = self
                .pending_commit_state
                .get_round_state(commit_leader_round);
            let mut committed_leaders = Vec::new();
            for s in &round_state.leader_slots {
                match &s.leader_status {
                    LeaderStatus::Commit(block) => {
                        committed_leaders.push(block.clone());
                    }
                    LeaderStatus::Skip(_) => {}
                    LeaderStatus::Undecided(slot) => panic!("Unexpected undecided slot {}", slot),
                }
            }
            committed_leaders
        };
        assert!(!committed_leaders.is_empty(), "No committed leaders found");
        self.report_decided_prefix_metrics(commit_leader_round);

        let mut dag_state = self.dag_state.write();
        let last_commit_digest = dag_state.last_commit_digest();
        let gc_round = dag_state.gc_round();

        // DFS from all committed leaders. For each block, select its uncommitted
        // ancestors above gc_round, mark them committed, and continue until the
        // frontier is empty.
        for leader in &committed_leaders {
            assert!(
                leader.round() > gc_round,
                "Leader block {:?} at or below gc_round {gc_round} should not be committed",
                leader.reference()
            );
            assert!(
                dag_state.set_committed(&leader.reference()),
                "Leader block {:?} attempted to be committed twice",
                leader.reference()
            );
        }
        let mut to_visit: Vec<VerifiedBlock> = committed_leaders.clone();
        let mut to_commit: Vec<VerifiedBlock> = Vec::new();
        while let Some(block) = to_visit.pop() {
            to_commit.push(block.clone());
            for ancestor_ref in block.ancestors().iter().copied() {
                if ancestor_ref.round <= gc_round || !dag_state.set_committed(&ancestor_ref) {
                    continue;
                }
                to_visit.push(dag_state.get_block(&ancestor_ref).unwrap());
            }
        }
        drop(dag_state);

        // Sort the committed blocks deterministically, first by round ascending,
        // then by deterministic hash specific to this commit and each block.
        let seed = compute_sort_seed(&committed_leaders);
        sort_committed_blocks(&mut to_commit, &seed);

        // Named leader = the committed leader that ended up last among the
        // committed leaders in the sorted sub-dag.
        let leader_ref = to_commit
            .last()
            .map(|b| b.reference())
            .expect("At least one committed leader must be in to_commit");
        assert_eq!(leader_ref.round, commit_leader_round);

        // Compute deterministic commit timestamp from leader parents.
        let timestamp_ms =
            calculate_commit_timestamp(&self.context, &self.dag_state.read(), &committed_leaders);

        let commit = Commit::new(
            self.pending_commit_state
                .next_commit_leaders
                .next_commit_index,
            last_commit_digest,
            timestamp_ms,
            leader_ref,
            to_commit.iter().map(|b| b.reference()).collect(),
        );
        let serialized = commit
            .serialize()
            .unwrap_or_else(|e| panic!("Failed to serialize commit: {e}"));
        let trusted_commit = TrustedCommit::new_trusted(commit, serialized);
        let sub_dag = CommittedSubDag::new(
            leader_ref,
            to_commit,
            timestamp_ms,
            trusted_commit.reference(),
        );

        tracing::debug!(
            "Built commit {} for leader round {} with {} committed leaders and {} blocks. Scheduled min next leader round {}, num allowed leaders {}",
            trusted_commit.reference(),
            commit_leader_round,
            committed_leaders.len(),
            sub_dag.blocks.len(),
            self.pending_commit_state
                .next_commit_leaders
                .min_next_leader_round,
            self.pending_commit_state
                .next_commit_leaders
                .allowed_leaders
                .len(),
        );

        Some((trusted_commit, sub_dag))
    }

    /// Builds a CommittedSubDag for a certified commit synced from a peer, and
    /// marks every committed block in DagState. The commit is already
    /// quorum-certified, so the commit rule is skipped here — unlike local
    /// commits, which are produced by `build_commit`.
    pub(crate) fn handle_certified_commit(&self, commit: &TrustedCommit) -> CommittedSubDag {
        tracing::debug!(
            "Handling certified commit {} with leader {} and {} blocks",
            commit.reference(),
            commit.leader(),
            commit.blocks().len()
        );
        let mut dag_state = self.dag_state.write();
        for block_ref in commit.blocks() {
            assert!(
                dag_state.set_committed(block_ref),
                "Block {:?} attempted to be committed twice",
                block_ref
            );
        }
        let blocks: Vec<VerifiedBlock> = dag_state
            .get_blocks(commit.blocks())
            .into_iter()
            .map(|b| b.expect("All blocks referenced by a trusted commit must be in DagState"))
            .collect();
        drop(dag_state);

        // Every block in the commit's leader round is a committed leader (same-round
        // blocks cannot be ancestors of each other, so they can only appear in the
        // commit as leaders). Report them like `build_commit` does for local
        // decisions, so `committed_leaders_total` covers the commit sync path too.
        for block in &blocks {
            if block.round() == commit.leader().round {
                let leader_host = &self.context.committee.authority(block.author()).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .committed_leaders_total
                    .with_label_values(&[leader_host, "certified-commit"])
                    .inc();
            }
        }

        let mut subdag = CommittedSubDag::new(
            commit.leader(),
            blocks,
            commit.timestamp_ms(),
            commit.reference(),
        );
        subdag.decided_with_local_blocks = false;
        subdag
    }
}

/// Tracks intermediate state for future commits.
///
/// An advanced commit removes the old round prefix. The remaining decisions stay valid while the
/// ordered leader schedule is unchanged. A schedule change resets all round state.
#[derive(Clone, Debug, Default)]
struct PendingCommitState {
    // Leader schedule for generating the upcoming commit.
    next_commit_leaders: NextCommitLeaderSchedule,
    // Intermediate state per round.
    rounds: VecDeque<RoundState>,
}

impl PendingCommitState {
    /// Returns the round' state and panics if it doesn't exist.
    /// Use `get_or_create_round_state` to create it if needed.
    fn get_round_state(&mut self, round: Round) -> &mut RoundState {
        let index = round
            .checked_sub(self.next_commit_leaders.min_next_leader_round)
            .unwrap() as usize;
        self.rounds.get_mut(index).unwrap()
    }

    /// Populate state up to round, if they don't exist yet. Then return the round's state.
    fn get_or_create_round_state(&mut self, round: Round) -> &mut RoundState {
        let add_from_round = self
            .rounds
            .back()
            .map(|state| state.round + 1)
            .unwrap_or(self.next_commit_leaders.min_next_leader_round);
        // No-op if the round's state exists.
        for r in add_from_round..=round {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[28..].copy_from_slice(&r.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);
            let mut leaders = self.next_commit_leaders.allowed_leaders.clone();
            leaders.shuffle(&mut rng);
            let leader_slots: Vec<LeaderSlot> = leaders
                .into_iter()
                .map(|leader| {
                    let slot = Slot::new(r, leader);
                    LeaderSlot {
                        slot,
                        leader_status: LeaderStatus::Undecided(slot),
                        decision: None,
                    }
                })
                .collect();
            let undecided_slots: BTreeSet<Slot> = leader_slots.iter().map(|s| s.slot).collect();
            self.rounds.push_back(RoundState {
                round: r,
                leader_slots,
                undecided_slots,
                num_committed: 0,
            });
        }

        self.get_round_state(round)
    }
}

/// Per-round intermediate state computed with commit rule and current leader schedule
#[derive(Clone, Debug)]
struct RoundState {
    round: Round,
    // Leader slots selected in the round, in order.
    leader_slots: Vec<LeaderSlot>,
    // Leader slots that haven't decided.
    undecided_slots: BTreeSet<Slot>,
    // Number of leader slots voted to be committed.
    // They may not appear in an actual commit as leaders though.
    num_committed: usize,
}

impl RoundState {
    fn update_slot_decision(&mut self, slot: Slot, new_status: LeaderStatus, decision: Decision) {
        let leader_slot = self
            .leader_slots
            .iter_mut()
            .find(|s| s.slot == slot)
            .unwrap_or_else(|| panic!("Slot {} must be in round {}", slot, self.round));
        if matches!(new_status, LeaderStatus::Undecided(_)) {
            if matches!(leader_slot.leader_status, LeaderStatus::Undecided(_)) {
                // No-op if the status is still undecided.
                return;
            } else {
                // Undeciding a previously decided slot should not happen.
                panic!(
                    "Cannot undecide slot {}: {:?} vs {:?}",
                    slot, leader_slot.leader_status, new_status
                );
            }
        }
        if self.undecided_slots.remove(&slot) {
            if matches!(new_status, LeaderStatus::Commit(_)) {
                self.num_committed += 1;
            }
            leader_slot.leader_status = new_status;
            // Record the rule that first decided this slot. A slot decided by the direct
            // rule may be re-evaluated by the indirect rule in the same round, but the
            // initial decision is kept (the else branch leaves `decision` untouched).
            leader_slot.decision = Some(decision);
        } else {
            assert_eq!(
                leader_slot.leader_status, new_status,
                "Cannot update slot {} decision: {:?} vs {:?}",
                slot, leader_slot.leader_status, new_status
            );
        }
    }
}

#[derive(Clone, Debug)]
struct LeaderSlot {
    slot: Slot,
    leader_status: LeaderStatus,
    // Which commit rule decided the slot (direct or indirect), or None while undecided.
    decision: Option<Decision>,
}

/// From a deterministic array of leaders, compute a deterministic digest used as seed for sort.
fn compute_sort_seed(committed_leaders: &[VerifiedBlock]) -> [u8; DIGEST_LENGTH] {
    let mut hasher = DefaultHashFunction::new();
    for leader in committed_leaders {
        hasher.update(leader.digest().as_ref());
    }
    hasher.finalize().into()
}

/// Per-block sort key:
/// - Primary part: block round.
/// - Secondary part: hash of seed and block digest.
fn block_sort_key(
    seed: &[u8; DIGEST_LENGTH],
    block_ref: &BlockRef,
) -> (Round, [u8; DIGEST_LENGTH]) {
    let mut hasher = DefaultHashFunction::new();
    hasher.update(seed);
    hasher.update(block_ref.digest);
    (block_ref.round, hasher.finalize().into())
}

fn sort_committed_blocks(blocks: &mut [VerifiedBlock], seed: &[u8; DIGEST_LENGTH]) {
    blocks.sort_by_cached_key(|b| block_sort_key(seed, &b.reference()));
}

/// Takes the union of each committed leader's parent-round ancestors,
/// and returns the median timestamp of parent blocks weighted by stake.
/// Monotonicity is enforced against `last_commit_timestamp_ms`.
fn calculate_commit_timestamp(
    context: &Context,
    dag_state: &DagState,
    committed_leaders: &[VerifiedBlock],
) -> BlockTimestampMs {
    let leader_round = committed_leaders[0].round();
    debug_assert!(committed_leaders.iter().all(|b| b.round() == leader_round));

    // When there are enough leader stake, use leaders to compute the commit timestamp.
    let total_leader_stake = committed_leaders
        .iter()
        .map(|b| context.committee.stake(b.author()))
        .sum::<Stake>();
    if total_leader_stake >= context.committee.certification_threshold() {
        let ts = crate::linearizer::median_timestamp_by_stake(context, committed_leaders)
            .unwrap_or_else(|e| panic!("Cannot compute commit timestamp: {e}"));
        return ts.max(dag_state.last_commit_timestamp_ms());
    }

    let mut parent_refs: BTreeMap<AuthorityIndex, BlockRef> = BTreeMap::new();
    for leader in committed_leaders {
        for ancestor in leader.ancestors() {
            if ancestor.round + 1 == leader_round {
                parent_refs.insert(ancestor.author, *ancestor);
            }
        }
    }
    let parent_refs: Vec<BlockRef> = parent_refs.values().cloned().collect();
    let block_opts = dag_state.get_blocks(&parent_refs);
    let blocks = block_opts
        .iter()
        .map(|b| b.as_ref().expect("Parent block must be in dag state"));
    let ts = crate::linearizer::median_timestamp_by_stake(context, blocks)
        .unwrap_or_else(|e| panic!("Cannot compute commit timestamp: {e}"));
    ts.max(dag_state.last_commit_timestamp_ms())
}

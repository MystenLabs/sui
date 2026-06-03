// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// FlexCommitter is wired into Core in a follow-up PR.
#![allow(dead_code)]

use std::{collections::BTreeSet, sync::Arc};

use consensus_config::{DIGEST_LENGTH, DefaultHashFunction};
use consensus_types::block::{BlockRef, BlockTimestampMs, Round};
use fastcrypto::hash::HashFunction as _;
use itertools::Itertools as _;
use parking_lot::RwLock;
use rand::{SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};

use crate::{
    BlockAPI, VerifiedBlock,
    block::Slot,
    commit::{Commit, CommitAPI, CommittedSubDag, LeaderStatus, TrustedCommit},
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
        if self
            .pending_commit_state
            .next_commit_leaders
            .next_commit_index
            == next_commit_leaders.next_commit_index
        {
            // Use cached pending commit state for the same commit index.
            return;
        }
        assert!(
            self.pending_commit_state
                .next_commit_leaders
                .next_commit_index
                < next_commit_leaders.next_commit_index,
            "next_commit_index should only move forward: {} vs {}",
            self.pending_commit_state
                .next_commit_leaders
                .next_commit_index,
            next_commit_leaders.next_commit_index
        );
        self.pending_commit_state = PendingCommitState {
            next_commit_leaders,
            rounds: vec![],
        };
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
                round_state.update_slot_decision(slot, status);
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
                    // First committed block becomes the anchor block, where there is no undecided slot before it.
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
        for (slot, status) in slots.into_iter().zip_eq(statuses) {
            round_state.update_slot_decision(slot, status);
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
        let committed_leaders: Vec<VerifiedBlock> = {
            let round_state = self
                .pending_commit_state
                .get_or_create_round_state(commit_leader_round);
            round_state
                .leader_slots
                .iter()
                .filter_map(|s| match &s.leader_status {
                    LeaderStatus::Commit(block) => Some(block.clone()),
                    LeaderStatus::Undecided(slot) => panic!("Unexpected undecided slot {}", slot),
                    _ => None,
                })
                .collect()
        };
        assert!(!committed_leaders.is_empty(), "No committed leaders found");

        let mut dag_state = self.dag_state.write();
        let last_commit_digest = dag_state.last_commit_digest();
        let gc_round = dag_state.gc_round();

        // DFS from all committed leaders. For each block, select its uncommitted
        // ancestors above gc_round, mark them committed, and continue until the
        // frontier is empty.
        for leader in &committed_leaders {
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
            let uncommitted_ancestor_refs: Vec<BlockRef> = block
                .ancestors()
                .iter()
                .copied()
                .filter(|a| a.round > gc_round && !dag_state.is_committed(a))
                .collect();
            for ancestor_ref in uncommitted_ancestor_refs {
                if !dag_state.set_committed(&ancestor_ref) {
                    continue;
                }
                to_visit.push(dag_state.get_block(&ancestor_ref).unwrap());
            }
        }
        assert!(
            to_commit.iter().all(|b| b.round() > gc_round),
            "No blocks at or below gc_round {gc_round} should be committed"
        );
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
        Some((trusted_commit, sub_dag))
    }

    /// Builds a CommittedSubDag for a certified commit synced from a peer, and
    /// marks every committed block in DagState. The commit is already
    /// quorum-certified, so the commit rule is skipped here — unlike local
    /// commits, which are produced by `build_commit`.
    pub(crate) fn handle_certified_commit(&self, commit: &TrustedCommit) -> CommittedSubDag {
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

#[derive(Clone, Debug, Default)]
struct PendingCommitState {
    next_commit_leaders: NextCommitLeaderSchedule,
    rounds: Vec<RoundState>,
}

impl PendingCommitState {
    fn get_or_create_round_state(&mut self, round: Round) -> &mut RoundState {
        let add_from_round = self
            .rounds
            .last()
            .map(|state| state.round + 1)
            .unwrap_or(self.next_commit_leaders.min_next_leader_round);
        // No-op if the round's state exists.
        for r in add_from_round..=round {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[28..].copy_from_slice(&r.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);
            let mut leaders = self
                .next_commit_leaders
                .allowed_leaders
                .choose_multiple(&mut rng, self.next_commit_leaders.num_leaders())
                .cloned()
                .collect::<Vec<_>>();
            leaders.shuffle(&mut rng);
            let leader_slots: Vec<LeaderSlot> = leaders
                .into_iter()
                .map(|leader| {
                    let slot = Slot::new(r, leader);
                    LeaderSlot {
                        slot,
                        leader_status: LeaderStatus::Undecided(slot),
                    }
                })
                .collect();
            let undecided_slots: BTreeSet<Slot> = leader_slots.iter().map(|s| s.slot).collect();
            self.rounds.push(RoundState {
                round: r,
                leader_slots,
                undecided_slots,
                num_committed: 0,
            });
        }
        let index = round
            .checked_sub(self.next_commit_leaders.min_next_leader_round)
            .unwrap() as usize;
        &mut self.rounds[index]
    }
}

#[derive(Clone, Debug)]
struct RoundState {
    round: Round,
    leader_slots: Vec<LeaderSlot>,
    undecided_slots: BTreeSet<Slot>,
    num_committed: usize,
}

impl RoundState {
    fn update_slot_decision(&mut self, slot: Slot, new_status: LeaderStatus) {
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

    let mut parent_refs: BTreeSet<BlockRef> = BTreeSet::new();
    for leader in committed_leaders {
        for ancestor in leader.ancestors() {
            if ancestor.round == leader_round.saturating_sub(1) {
                parent_refs.insert(*ancestor);
            }
        }
    }
    let parent_refs: Vec<BlockRef> = parent_refs.into_iter().collect();
    let blocks = dag_state
        .get_blocks(&parent_refs)
        .into_iter()
        .map(|b| b.expect("Parent block must be in dag state"));
    let ts = crate::linearizer::median_timestamp_by_stake(context, blocks)
        .unwrap_or_else(|e| panic!("Cannot compute commit timestamp: {e}"));
    ts.max(dag_state.last_commit_timestamp_ms())
}

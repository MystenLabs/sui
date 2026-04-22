// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use consensus_config::{DIGEST_LENGTH, DefaultHashFunction, Stake};
use consensus_types::block::{BlockRef, BlockTimestampMs, Round};
use fastcrypto::hash::HashFunction as _;
use parking_lot::RwLock;
use rand::{SeedableRng as _, rngs::StdRng, seq::SliceRandom as _};

use crate::{
    BlockAPI, VerifiedBlock,
    block::Slot,
    commit::{Commit, CommitAPI, CommittedSubDag, LeaderStatus, TrustedCommit},
    context::Context,
    dag_state::DagState,
    leader_schedule_v3::NextCommitLeaderSchedule,
    leader_slot_decider::LeaderSlotDecider,
};

#[cfg(test)]
#[path = "tests/flex_committer_tests.rs"]
mod flex_committer_tests;

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

    /// Runs the direct commit rule on every  leader slot pending to be decided.
    fn try_direct_commit(&mut self) {
        let min_next_leader_round = self
            .pending_commit_state
            .next_commit_leaders
            .min_next_leader_round;
        let highest_accepted_round = self.dag_state.read().highest_accepted_round();

        for leader_round in min_next_leader_round..highest_accepted_round {
            let slots_to_decide = self
                .pending_commit_state
                .get_or_create_round_state(leader_round)
                .leader_slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    if matches!(s.leader_status, LeaderStatus::Undecided(_)) {
                        Some((i, s.slot))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for (i, slot) in slots_to_decide {
                let status = self.slot_decider.try_direct_decide(slot);
                if matches!(status, LeaderStatus::Undecided(_)) {
                    continue;
                }
                let round_state = self
                    .pending_commit_state
                    .get_or_create_round_state(leader_round);
                round_state.undecided_slots.remove(&slot);
                if matches!(status, LeaderStatus::Commit(_)) {
                    round_state.num_committed += 1;
                }
                let leader_slot = &mut round_state.leader_slots[i];
                leader_slot.leader_status = status;
            }
        }
    }

    fn try_indirect_commit(&mut self) {
        let min_next_leader_round = self
            .pending_commit_state
            .next_commit_leaders
            .min_next_leader_round;
        let highest_accepted_round = self.dag_state.read().highest_accepted_round();
        for round in (min_next_leader_round..=(highest_accepted_round.saturating_sub(2))).rev() {
            let round_state = self.pending_commit_state.get_or_create_round_state(round);
            // Indirect commit is unnecessary when the round has been fully decided.
            if round_state.undecided_slots.is_empty() {
                continue;
            }
            // There is no anchor block for blocks in this round yet.
            let Some(anchor_block) = self.find_anchor_block(round + 2) else {
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

    fn decide_with_anchor_block(&mut self, anchor_block: VerifiedBlock, decision_round: Round) {
        let slots: Vec<Slot> = self
            .pending_commit_state
            .get_or_create_round_state(decision_round)
            .leader_slots
            .iter()
            .map(|s| s.slot)
            .collect();

        let statuses = self.slot_decider.try_indirect_decide(&anchor_block, &slots);

        let round_state = self
            .pending_commit_state
            .get_or_create_round_state(decision_round);
        for (i, status) in statuses.into_iter().enumerate() {
            let leader_slot = &mut round_state.leader_slots[i];
            let was_undecided = matches!(leader_slot.leader_status, LeaderStatus::Undecided(_));
            // Indirect and direct decisions must agree.
            if was_undecided {
                round_state.undecided_slots.remove(&leader_slot.slot);
                if matches!(status, LeaderStatus::Commit(_)) {
                    round_state.num_committed += 1;
                }
            } else {
                assert_eq!(
                    leader_slot.leader_status, status,
                    "Indirect and direct commit decisions must agree: {:?} vs {:?}",
                    leader_slot.leader_status, status
                );
            }
            leader_slot.leader_status = status.clone();
        }
    }

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

    /// Builds a single commit from `commit_leader_round`.
    ///
    /// Traversal starts from every committed leader in the round — v3 allows
    /// multiple committed leaders per round. Blocks are marked committed in
    /// DagState during the DFS so subsequent calls skip them. The resulting
    /// sub-dag is sorted by `sort_committed_blocks` (round ascending, then a
    /// deterministic per-block key keyed on the committed-leader set); the
    /// block that lands last among the committed leaders becomes the named
    /// `leader` of both the `Commit` and the `CommittedSubDag`.
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

        // DFS from all committed leaders. For each block, pull its uncommitted
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

        let seed = compute_sort_seed(&committed_leaders);
        sort_committed_blocks(&mut to_commit, &seed);

        // Named leader = the committed leader that ended up last among the
        // committed leaders in the sorted sub-dag.
        let leader_ref = to_commit
            .last()
            .map(|b| b.reference())
            .expect("At least one committed leader must be in to_commit");
        assert_eq!(leader_ref.round, commit_leader_round);

        let timestamp_ms = calculate_commit_timestamp(
            &self.context,
            &self.dag_state.read(),
            &committed_leaders,
            self.dag_state.read().last_commit_timestamp_ms(),
        );

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

    /// Builds a CommittedSubDag for a certified commit. The certified-sync
    /// path doesn't traverse blocks locally, so this method also marks every
    /// block referenced by `commit` as committed in DagState before loading
    /// them in `commit.blocks()` order — that order is authoritative, chosen
    /// by whichever node produced the commit. `decided_with_local_blocks` is
    /// set to `false` because the leader's certificate is not guaranteed to
    /// be reconstructable from the local DAG.
    ///
    /// Must not be called on the local decision path; `build_commit` already
    /// marks blocks during its DFS and returns its own CommittedSubDag.
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
        // This loop does not run when round's state has been created.
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
        &mut self.rounds[(round - self.next_commit_leaders.min_next_leader_round) as usize]
    }
}

#[derive(Clone, Debug)]
struct RoundState {
    round: Round,
    leader_slots: Vec<LeaderSlot>,
    undecided_slots: BTreeSet<Slot>,
    num_committed: usize,
}

#[derive(Clone, Debug)]
struct LeaderSlot {
    slot: Slot,
    leader_status: LeaderStatus,
}

/// Commit-level seed for v3 sub-dag sorting: a hash over the committed-leader
/// digests in iteration order. Changing the committed leader set or their
/// iteration order changes the seed, so the within-round block order changes
/// with the set of leaders being committed, without any validator-specific
/// state.
fn compute_sort_seed(committed_leaders: &[VerifiedBlock]) -> [u8; DIGEST_LENGTH] {
    let mut hasher = DefaultHashFunction::new();
    for leader in committed_leaders {
        hasher.update(leader.digest().as_ref());
    }
    hasher.finalize().into()
}

/// Per-block sort key:
/// - Primary: `block.round()`, so rounds remain grouped ascending.
/// - Secondary: `hash(seed || block.digest())`. Because block digests are
///   unique across distinct blocks, the key is a strict total order — no
///   ties even for equivocating blocks at the same slot.
///
/// Within a round this gives a deterministic but seed-dependent permutation
/// where neither the lowest authority nor the lowest digest wins by default.
fn block_sort_key(
    seed: &[u8; DIGEST_LENGTH],
    block: &VerifiedBlock,
) -> (Round, [u8; DIGEST_LENGTH]) {
    let mut hasher = DefaultHashFunction::new();
    hasher.update(seed);
    hasher.update(block.digest().as_ref());
    (block.round(), hasher.finalize().into())
}

fn sort_committed_blocks(blocks: &mut [VerifiedBlock], seed: &[u8; DIGEST_LENGTH]) {
    blocks.sort_by_cached_key(|b| block_sort_key(seed, b));
}

/// V3 commit timestamp: take the union of each committed leader's
/// parent-round ancestors, look them up in DagState, and return the median
/// timestamp weighted by stake. Monotonicity is enforced against
/// `last_commit_timestamp_ms`.
fn calculate_commit_timestamp(
    context: &Context,
    dag_state: &DagState,
    committed_leaders: &[VerifiedBlock],
    last_commit_timestamp_ms: BlockTimestampMs,
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
    let ts = median_timestamp_by_stake(context, blocks)
        .unwrap_or_else(|e| panic!("Cannot compute commit timestamp: {e}"));
    ts.max(last_commit_timestamp_ms)
}

/// Median timestamp of `blocks` weighted by authority stake. Errors if no
/// blocks are provided or if the total stake of the provided blocks is below
/// `quorum_threshold`. Local copy so `flex_committer` has no `linearizer`
/// dependency.
fn median_timestamp_by_stake(
    context: &Context,
    blocks: impl Iterator<Item = VerifiedBlock>,
) -> Result<BlockTimestampMs, String> {
    let mut total_stake: Stake = 0;
    let mut timestamps: Vec<(BlockTimestampMs, Stake)> = vec![];
    for block in blocks {
        let stake = context.committee.authority(block.author()).stake;
        timestamps.push((block.timestamp_ms(), stake));
        total_stake += stake;
    }
    if timestamps.is_empty() {
        return Err("No blocks provided".to_string());
    }
    if total_stake < context.committee.quorum_threshold() {
        return Err(format!(
            "Total stake {} < quorum threshold {}",
            total_stake,
            context.committee.quorum_threshold()
        ));
    }
    timestamps.sort_by_key(|(ts, _)| *ts);
    let mut cumulative: Stake = 0;
    for (ts, stake) in &timestamps {
        cumulative += stake;
        if cumulative > total_stake / 2 {
            return Ok(*ts);
        }
    }
    Ok(timestamps.last().unwrap().0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::TestBlock;

    fn build_blocks(rounds_x_authorities: &[(Round, u32)]) -> Vec<VerifiedBlock> {
        rounds_x_authorities
            .iter()
            .map(|(r, a)| VerifiedBlock::new_for_test(TestBlock::new(*r, *a).build()))
            .collect()
    }

    #[test]
    fn sort_committed_blocks_is_total_ordered_and_round_major() {
        let inputs = build_blocks(&[
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (2, 0),
            (2, 1),
            (2, 2),
            (2, 3),
        ]);
        let seed = [7u8; DIGEST_LENGTH];

        let mut a = inputs.clone();
        let mut b = inputs.clone();
        b.reverse();
        let mut c = inputs.clone();
        c.rotate_left(3);

        sort_committed_blocks(&mut a, &seed);
        sort_committed_blocks(&mut b, &seed);
        sort_committed_blocks(&mut c, &seed);

        let refs = |v: &[VerifiedBlock]| v.iter().map(|b| b.reference()).collect::<Vec<_>>();
        assert_eq!(
            refs(&a),
            refs(&b),
            "sort must be deterministic regardless of input order"
        );
        assert_eq!(refs(&a), refs(&c));

        // Round-major, ascending.
        let rounds: Vec<Round> = a.iter().map(|b| b.round()).collect();
        assert!(
            rounds.windows(2).all(|w| w[0] <= w[1]),
            "rounds must be non-decreasing"
        );
    }

    #[test]
    fn sort_committed_blocks_within_round_is_not_author_ascending() {
        // With 4 distinct authorities in one round, a meaningful shuffle will
        // not leave them in author-ascending order for every seed.
        let inputs = build_blocks(&[(1, 0), (1, 1), (1, 2), (1, 3)]);

        // Try several seeds — at least one should scramble the author order.
        let mut any_scrambled = false;
        for s in 0u8..16 {
            let seed = [s; DIGEST_LENGTH];
            let mut v = inputs.clone();
            sort_committed_blocks(&mut v, &seed);
            let authors: Vec<u32> = v.iter().map(|b| b.author().value() as u32).collect();
            if authors != vec![0, 1, 2, 3] {
                any_scrambled = true;
                break;
            }
        }
        assert!(
            any_scrambled,
            "v3 sort should not always leave authors in ascending order"
        );
    }

    #[test]
    fn sort_committed_blocks_responds_to_seed() {
        let inputs = build_blocks(&[
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (2, 0),
            (2, 1),
            (2, 2),
            (2, 3),
        ]);
        let seed_a = [0u8; DIGEST_LENGTH];
        let mut seed_b = [0u8; DIGEST_LENGTH];
        seed_b[0] = 1;

        let mut a = inputs.clone();
        let mut b = inputs.clone();
        sort_committed_blocks(&mut a, &seed_a);
        sort_committed_blocks(&mut b, &seed_b);

        let refs_a: Vec<_> = a.iter().map(|x| x.reference()).collect();
        let refs_b: Vec<_> = b.iter().map(|x| x.reference()).collect();
        assert_ne!(
            refs_a, refs_b,
            "different seeds should produce different within-round orders"
        );
    }
}

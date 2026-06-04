// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Production callers (FlexCommitter) land in a follow-up PR.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_types::block::{BlockRef, Round};
use parking_lot::RwLock;

use crate::{
    BlockAPI, VerifiedBlock,
    block::Slot,
    commit::LeaderStatus,
    context::Context,
    dag_state::DagState,
    stake_aggregator::{CertificationThreshold, QuorumThreshold, StakeAggregator},
};

#[cfg(test)]
#[path = "tests/leader_slot_decider_tests.rs"]
mod leader_slot_decider_tests;

/// Minimum number of rounds there an anchor block can indirectly decide a leader block.
pub(crate) const INDIRECT_COMMIT_DEPTH: Round = 2;

/// Stateless decision logic for commit rule v3.
pub(crate) struct LeaderSlotDecider {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
}

impl LeaderSlotDecider {
    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        Self { context, dag_state }
    }

    /// Evaluates if `slot` should be directly committed or skipped,
    /// based on voting information from DagState.
    ///
    /// A block is directly committed if it has a quorum of commit votes,
    /// and a block is directly skipped if it has a quorum of skip votes.
    /// Misbehaviors are assumed to be under the fault tolerance threshold, but we also
    /// try to detect if this assumption is broken, when it is convenient.
    pub(crate) fn try_direct_decide(&self, slot: Slot) -> LeaderStatus {
        let dag_state = self.dag_state.read();
        let Some(voting_round_info) = dag_state.get_round_info(slot.round + 1) else {
            // No blocks accepted in the next round yet, so there is no votes to the slot.
            return LeaderStatus::Undecided(slot);
        };
        if !voting_round_info
            .total_stake
            .reached_threshold(&self.context.committee)
        {
            // Next round has insufficient stake to decide the slot.
            return LeaderStatus::Undecided(slot);
        }

        let block_infos = dag_state.get_block_info_at_slot(slot);
        let num_blocks = block_infos.len();
        let mut num_skipped = 0;
        for block_info in block_infos {
            // Skip the leader block if it has a quorum of skip votes.
            // Byzantine authorities can vote on both sides, but its votes count at most once per side
            // against a decision block.
            let mut reject_votes = StakeAggregator::<QuorumThreshold>::new();
            // Add up votes skipping the block.
            for block_ref in voting_round_info.blocks.difference(&block_info.children) {
                reject_votes.add_unique(block_ref.author, &self.context.committee);
            }
            let mut block_rejected = false;
            if reject_votes.reached_threshold(&self.context.committee) {
                num_skipped += 1;
                block_rejected = true;
                // Continue checking if the block can be committed,
                // to detect if there are too many misbehaving authorities.
            }
            // Commit the leader block if it has a quorum of commit votes.
            if block_info
                .children_stake
                .reached_threshold(&self.context.committee)
            {
                assert!(
                    !block_rejected,
                    "Block {} cannot be both committed and skipped. Commit voters: {:?}, skip voters: {:?}",
                    block_info.block.reference(),
                    block_info.children_stake.authorities(),
                    reject_votes.authorities(),
                );
                return LeaderStatus::Commit(block_info.block);
            }
        }

        // When there is no block in the slot or all blocks in the slot are skipped,
        // the whole slot is skipped.
        // Even if new blocks arrive at the slot later, they are guaranteed to be
        // skipped directly since a quorum already skipped them.
        if num_skipped == num_blocks {
            return LeaderStatus::Skip(slot);
        }

        // There are undecided blocks, so the slot is undecided.
        LeaderStatus::Undecided(slot)
    }

    /// Evaluates every slot in `decision_slots` for indirect commit.
    /// Returns one LeaderStatus per `decision_slots` entry, in the same order.
    ///
    /// Commit votes per decision block are counted from voting blocks reachable
    /// via BFS from `anchor_block`. Then the commit votes on each decision block is
    /// compared against `certification_threshold`.
    ///
    /// It is possible for a block to have both commit and skip certificates. But skip
    /// votes and certificates are not tracked because they do not affect the decision
    /// on the slot.
    ///
    /// When a slot has exactly one block with commit certificate, the slot is considered
    /// committed. But if there are zero or multiple blocks with commit certificates,
    /// the slot is considered skipped because there cannot have been a direct commit on
    /// the slot.
    ///
    /// Each slot will be either committed or skipped, never undecided after applying the
    /// indirect commit rule.
    pub(crate) fn try_indirect_decide(
        &self,
        anchor_block: &VerifiedBlock,
        decision_slots: &[Slot],
    ) -> Vec<LeaderStatus> {
        assert!(!decision_slots.is_empty());
        let decision_round = decision_slots[0].round;
        assert!(decision_slots.iter().all(|s| s.round == decision_round));
        assert!(
            decision_round + INDIRECT_COMMIT_DEPTH <= anchor_block.round(),
            "Anchor block {} is too close to decision round {}",
            anchor_block.reference(),
            decision_round,
        );

        let dag_state = self.dag_state.read();

        let decision_blocks: BTreeSet<BlockRef> = decision_slots
            .iter()
            .flat_map(|slot| {
                dag_state
                    .get_uncommitted_blocks_at_slot(*slot)
                    .into_iter()
                    .map(|block| block.reference())
            })
            .collect();

        // BFS once from the anchor: collect votes from voting round blocks
        // against decision round blocks.
        let mut to_visit = VecDeque::new();
        to_visit.push_back(anchor_block.clone());
        let mut visited = BTreeSet::new();
        visited.insert(anchor_block.reference());

        // Count commit votes for decision blocks.
        let mut votes = BTreeMap::<BlockRef, StakeAggregator<CertificationThreshold>>::new();
        while let Some(block) = to_visit.pop_front() {
            for ancestor in block.ancestors() {
                // Set up next steps of BFS.
                if ancestor.round > decision_round && visited.insert(*ancestor) {
                    let ancestor_block = dag_state
                        .get_block(ancestor)
                        .unwrap_or_else(|| panic!("Block {} must exist", ancestor));
                    to_visit.push_back(ancestor_block);
                }
                // Count only voting block to decision block votes.
                if decision_blocks.contains(ancestor) && block.round() == decision_round + 1 {
                    let commit_stake = votes.entry(*ancestor).or_default();
                    commit_stake.add_unique(block.author(), &self.context.committee);
                }
            }
        }

        // Collect the blocks with commit certificates per slot.
        let mut certified_slots = BTreeMap::<Slot, Vec<VerifiedBlock>>::new();
        for (block_ref, commit_stake) in votes {
            let slot = Slot {
                round: block_ref.round,
                authority: block_ref.author,
            };
            if commit_stake.reached_threshold(&self.context.committee) {
                certified_slots.entry(slot).or_default().push(
                    dag_state
                        .get_block(&block_ref)
                        .unwrap_or_else(|| panic!("Block {} must exist", block_ref)),
                );
            }
        }

        // Make the decision on each slot.
        decision_slots
            .iter()
            .map(|slot| {
                let mut certified_blocks = certified_slots.get(slot).cloned().unwrap_or_default();
                // When there is no certified block at the slot, obviously the slot is skipped.
                // When there are multiple certified blocks at the slot, there cannot have been a direct commit
                // on the slot if there are <= f malicious stake. So it is also safe to skip the slot.
                if certified_blocks.is_empty() || certified_blocks.len() > 1 {
                    return LeaderStatus::Skip(*slot);
                }
                LeaderStatus::Commit(certified_blocks.pop().unwrap())
            })
            .collect()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Production callers (FlexCommitter) land in a follow-up PR.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use consensus_types::block::BlockRef;
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
        let mut committed = vec![];
        let mut num_skipped = 0;
        for block_info in block_infos {
            // Commit the leader block if it has a quorum of commit votes.
            if block_info
                .children_stake
                .reached_threshold(&self.context.committee)
            {
                committed.push(block_info.block.clone());
                continue;
            }
            // Skip the leader block if it has a quorum of skip votes.
            // Byzantine authorities that produce both vote and non-vote blocks
            // can count on both sides, but at most once per side against a decision block.
            let mut reject_votes = StakeAggregator::<QuorumThreshold>::new();
            for block_ref in voting_round_info.blocks.difference(&block_info.children) {
                reject_votes.add_unique(block_ref.author, &self.context.committee);
            }
            if reject_votes.reached_threshold(&self.context.committee) {
                num_skipped += 1;
            }
        }

        // When there is no block at the slot or all blocks at the slot are skipped,
        // the whole slot is skipped.
        // Even if new blocks arrive at the slot later, they are guaranteed to be
        // skipped directly since a quorum already rejected them.
        if num_skipped == num_blocks {
            return LeaderStatus::Skip(slot);
        }

        // Under safety assumption, at most one block can be committed in a slot.
        if committed.len() > 1 {
            panic!(
                "Multiple committed blocks found at leader slot {}: {:?}",
                slot, committed
            );
        }
        // When there is a committed block, assume no other block at the slot can be committed.
        if committed.len() == 1 {
            return LeaderStatus::Commit(committed.pop().unwrap());
        }

        // There are undecided blocks, so the slot is undecided.
        LeaderStatus::Undecided(slot)
    }

    /// Evaluates every slot in `slots` (all at `decision_round`) for indirect commit.
    /// Returns one LeaderStatus per `slots` entry, in the same order.
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
                if ancestor.round < decision_round {
                    // This ancestor and its ancestors are irrelevant for voting.
                    continue;
                }
                // Set up next setup of BFS.
                if visited.insert(*ancestor) {
                    let ancestor_block = dag_state
                        .get_block(ancestor)
                        .unwrap_or_else(|| panic!("Block {} must exist", ancestor));
                    to_visit.push_back(ancestor_block);
                }
                // Continue to only if this is a vote from the block to a decision block.
                if decision_blocks.contains(ancestor) && block.round() == decision_round + 1 {
                    let commit_stake = votes.entry(*ancestor).or_default();
                    commit_stake.add_unique(block.author(), &self.context.committee);
                }
            }
        }

        // Collect the blocks with commit certificates.
        let mut commit_certificates = BTreeMap::<Slot, Vec<VerifiedBlock>>::new();
        for (block_ref, commit_stake) in votes {
            let slot = Slot {
                round: block_ref.round,
                authority: block_ref.author,
            };
            if commit_stake.reached_threshold(&self.context.committee) {
                commit_certificates.entry(slot).or_default().push(
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
                let mut certified_blocks =
                    commit_certificates.get(slot).cloned().unwrap_or_default();
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

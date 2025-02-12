// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, fmt::Display, sync::Arc};

use consensus_config::{AuthorityIndex, Stake};
use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, BlockRef, Round, Slot, VerifiedBlock},
    commit::{LeaderStatus, WaveNumber, DEFAULT_WAVE_LENGTH, MINIMUM_WAVE_LENGTH},
    context::Context,
    dag_state::DagState,
    leader_schedule::LeaderSchedule,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

#[cfg(test)]
#[path = "tests/base_committer_tests.rs"]
mod base_committer_tests;

#[cfg(test)]
#[path = "tests/base_committer_declarative_tests.rs"]
mod base_committer_declarative_tests;

pub(crate) struct BaseCommitterOptions {
    /// TODO: Re-evaluate if we want this to be configurable after running experiments.
    /// The length of a wave (minimum 3)
    pub wave_length: u32,
    /// The offset used in the leader-election protocol. This is used by the
    /// multi-committer to ensure that each [`BaseCommitter`] instance elects
    /// a different leader.
    pub leader_offset: u32,
    /// The offset of the first wave. This is used by the pipelined committer to
    /// ensure that each[`BaseCommitter`] instances operates on a different
    /// view of the dag.
    pub round_offset: u32,
}

impl Default for BaseCommitterOptions {
    fn default() -> Self {
        Self {
            wave_length: DEFAULT_WAVE_LENGTH,
            leader_offset: 0,
            round_offset: 0,
        }
    }
}

/// The [`BaseCommitter`] contains the bare bone commit logic. Once instantiated,
/// the method `try_direct_decide` and `try_indirect_decide` can be called at any
/// time and any number of times (it is idempotent) to determine whether a leader
/// can be committed or skipped.
pub(crate) struct BaseCommitter {
    /// The per-epoch configuration of this authority.
    context: Arc<Context>,
    /// The consensus leader schedule to be used to resolve the leader for a
    /// given round.
    leader_schedule: Arc<LeaderSchedule>,
    /// In memory block store representing the dag state
    dag_state: Arc<RwLock<DagState>>,
    /// The options used by this committer
    options: BaseCommitterOptions,
}

impl BaseCommitter {
    pub fn new(
        context: Arc<Context>,
        leader_schedule: Arc<LeaderSchedule>,
        dag_state: Arc<RwLock<DagState>>,
        options: BaseCommitterOptions,
    ) -> Self {
        assert!(options.wave_length >= MINIMUM_WAVE_LENGTH);
        Self {
            context,
            leader_schedule,
            dag_state,
            options,
        }
    }

    /// Apply the direct decision rule to the specified leader to see whether we
    /// can direct-commit or direct-skip it.
    #[tracing::instrument(skip_all, fields(leader = %leader))]
    pub fn try_direct_decide(&self, leader: Slot) -> LeaderStatus {
        // Check whether the leader has enough blame. That is, whether there are 2f+1 non-votes
        // for that leader (which ensure there will never be a certificate for that leader).
        let voting_round = leader.round + 1;
        if self.enough_leader_blame(voting_round, leader.authority) {
            return LeaderStatus::Skip(leader);
        }

        // Check whether the leader(s) has enough support. That is, whether there are 2f+1
        // certificates over the leader. Note that there could be more than one leader block
        // (created by Byzantine leaders).
        let wave = self.wave_number(leader.round);
        let decision_round = self.decision_round(wave);
        let leader_blocks = self.dag_state.read().get_uncommitted_blocks_at_slot(leader);
        let mut leaders_with_enough_support: Vec<_> = leader_blocks
            .into_iter()
            .filter(|l| self.enough_leader_support(decision_round, l))
            .map(LeaderStatus::Commit)
            .collect();

        // There can be at most one leader with enough support for each round, otherwise it means
        // the BFT assumption is broken.
        if leaders_with_enough_support.len() > 1 {
            panic!("[{self}] More than one certified block for {leader}")
        }

        leaders_with_enough_support
            .pop()
            .unwrap_or(LeaderStatus::Undecided(leader))
    }

    /// Apply the indirect decision rule to the specified leader to see whether
    /// we can indirect-commit or indirect-skip it.
    #[tracing::instrument(skip_all, fields(leader = %leader_slot))]
    pub fn try_indirect_decide<'a>(
        &self,
        leader_slot: Slot,
        leaders: impl Iterator<Item = &'a LeaderStatus>,
    ) -> LeaderStatus {
        // The anchor is the first committed leader with round higher than the decision round of the
        // target leader. We must stop the iteration upon encountering an undecided leader.
        let anchors = leaders.filter(|x| leader_slot.round + self.options.wave_length <= x.round());

        for anchor in anchors {
            tracing::trace!(
                "[{self}] Trying to indirect-decide {leader_slot} using anchor {anchor}",
            );
            match anchor {
                LeaderStatus::Commit(anchor) => {
                    return self.decide_leader_from_anchor(anchor, leader_slot);
                }
                LeaderStatus::Skip(..) => (),
                LeaderStatus::Undecided(..) => break,
            }
        }

        LeaderStatus::Undecided(leader_slot)
    }

    pub fn elect_leader(&self, round: Round) -> Option<Slot> {
        let wave = self.wave_number(round);
        tracing::trace!(
            "elect_leader: round={}, wave={}, leader_round={}, leader_offset={}",
            round,
            wave,
            self.leader_round(wave),
            self.options.leader_offset
        );
        if self.leader_round(wave) != round {
            return None;
        }

        Some(Slot::new(
            round,
            self.leader_schedule
                .elect_leader(round, self.options.leader_offset),
        ))
    }

    /// Return the leader round of the specified wave. The leader round is always
    /// the first round of the wave. This takes into account round offset for when
    /// pipelining is enabled.
    pub(crate) fn leader_round(&self, wave: WaveNumber) -> Round {
        (wave * self.options.wave_length) + self.options.round_offset
    }

    /// Return the decision round of the specified wave. The decision round is
    /// always the last round of the wave. This takes into account round offset
    /// for when pipelining is enabled.
    pub(crate) fn decision_round(&self, wave: WaveNumber) -> Round {
        let wave_length = self.options.wave_length;
        (wave * wave_length) + wave_length - 1 + self.options.round_offset
    }

    /// Return the wave in which the specified round belongs. This takes into
    /// account the round offset for when pipelining is enabled.
    pub(crate) fn wave_number(&self, round: Round) -> WaveNumber {
        round.saturating_sub(self.options.round_offset) / self.options.wave_length
    }

    /// Find which block is supported at a slot (author, round) by the given block.
    /// Blocks can indirectly reference multiple other blocks at a slot, but only
    /// one block at a slot will be supported by the given block. If block A supports B
    /// at a slot, it is guaranteed that any processed block by the same author that
    /// directly or indirectly includes A will also support B at that slot.
    fn find_supported_block(&self, leader_slot: Slot, from: &VerifiedBlock) -> Option<BlockRef> {
        if from.round() < leader_slot.round {
            return None;
        }
        for ancestor in from.ancestors() {
            if Slot::from(*ancestor) == leader_slot {
                return Some(*ancestor);
            }
            // Weak links may point to blocks with lower round numbers than strong links.
            if ancestor.round <= leader_slot.round {
                continue;
            }
            let ancestor = self
                .dag_state
                .read()
                .get_block(ancestor)
                .unwrap_or_else(|| panic!("Block not found in storage: {:?}", ancestor));
            if let Some(support) = self.find_supported_block(leader_slot, &ancestor) {
                return Some(support);
            }
        }
        None
    }

    /// Check whether the specified block (`potential_vote`) is a vote for
    /// the specified leader (`leader_block`).
    fn is_vote(&self, potential_vote: &VerifiedBlock, leader_block: &VerifiedBlock) -> bool {
        let reference = leader_block.reference();
        let leader_slot = Slot::from(reference);
        self.find_supported_block(leader_slot, potential_vote) == Some(reference)
    }

    /// Check whether the specified block (`potential_certificate`) is a certificate
    /// for the specified leader (`leader_block`). An `all_votes` map can be
    /// provided as a cache to quickly skip checking against the block store on
    /// whether a reference is a vote. This is done for efficiency. Bear in mind
    /// that the `all_votes` should refer to votes considered to the same `leader_block`
    /// and it can't be reused for different leaders.
    fn is_certificate(
        &self,
        potential_certificate: &VerifiedBlock,
        leader_block: &VerifiedBlock,
        all_votes: &mut HashMap<BlockRef, bool>,
    ) -> bool {
        let (gc_enabled, gc_round) = {
            let dag_state = self.dag_state.read();
            (dag_state.gc_enabled(), dag_state.gc_round())
        };

        let mut votes_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for reference in potential_certificate.ancestors() {
            let is_vote = if let Some(is_vote) = all_votes.get(reference) {
                *is_vote
            } else {
                let potential_vote = self.dag_state.read().get_block(reference);

                let is_vote = if gc_enabled {
                    if let Some(potential_vote) = potential_vote {
                        self.is_vote(&potential_vote, leader_block)
                    } else {
                        assert!(reference.round <= gc_round, "Block not found in storage: {:?} , and is not below gc_round: {gc_round}", reference);
                        false
                    }
                } else {
                    let potential_vote = potential_vote
                        .unwrap_or_else(|| panic!("Block not found in storage: {:?}", reference));
                    self.is_vote(&potential_vote, leader_block)
                };

                all_votes.insert(*reference, is_vote);
                is_vote
            };

            if is_vote {
                tracing::trace!("[{self}] {reference} is a vote for {leader_block}");
                if votes_stake_aggregator.add(reference.author, &self.context.committee) {
                    tracing::trace!(
                        "[{self}] {potential_certificate} is a certificate for leader {leader_block}"
                    );
                    return true;
                }
            } else {
                tracing::trace!("[{self}] {reference} is not a vote for {leader_block}",);
            }
        }
        tracing::trace!(
            "[{self}] {potential_certificate} is not a certificate for leader {leader_block}"
        );
        false
    }

    /// Decide the status of a target leader from the specified anchor. We commit
    /// the target leader if it has a certified link to the anchor. Otherwise, we
    /// skip the target leader.
    fn decide_leader_from_anchor(&self, anchor: &VerifiedBlock, leader_slot: Slot) -> LeaderStatus {
        // Get the block(s) proposed by the leader. There could be more than one leader block
        // in the slot from a Byzantine authority.
        let leader_blocks = self
            .dag_state
            .read()
            .get_uncommitted_blocks_at_slot(leader_slot);

        // TODO: Re-evaluate this check once we have a better way to handle/track byzantine authorities.
        if leader_blocks.len() > 1 {
            tracing::warn!(
                "Multiple blocks found for leader slot {leader_slot}: {:?}",
                leader_blocks
            );
        }

        // Get all blocks that could be potential certificates for the target leader. These blocks
        // are in the decision round of the target leader and are linked to the anchor.
        let wave = self.wave_number(leader_slot.round);
        let decision_round = self.decision_round(wave);
        let potential_certificates = self
            .dag_state
            .read()
            .ancestors_at_round(anchor, decision_round);

        // Use those potential certificates to determine which (if any) of the target leader
        // blocks can be committed.
        let mut certified_leader_blocks: Vec<_> = leader_blocks
            .into_iter()
            .filter(|leader_block| {
                let mut all_votes = HashMap::new();
                potential_certificates.iter().any(|potential_certificate| {
                    self.is_certificate(potential_certificate, leader_block, &mut all_votes)
                })
            })
            .collect();

        // There can be at most one certified leader, otherwise it means the BFT assumption is broken.
        if certified_leader_blocks.len() > 1 {
            panic!("More than one certified block at wave {wave} from leader {leader_slot}")
        }

        // We commit the target leader if it has a certificate that is an ancestor of the anchor.
        // Otherwise skip it.
        match certified_leader_blocks.pop() {
            Some(certified_leader_block) => LeaderStatus::Commit(certified_leader_block),
            None => LeaderStatus::Skip(leader_slot),
        }
    }

    /// Check whether the specified leader has 2f+1 non-votes (blames) to be directly skipped.
    fn enough_leader_blame(&self, voting_round: Round, leader: AuthorityIndex) -> bool {
        let voting_blocks = self
            .dag_state
            .read()
            .get_uncommitted_blocks_at_round(voting_round);

        let mut blame_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for voting_block in &voting_blocks {
            let voter = voting_block.reference().author;
            if voting_block
                .ancestors()
                .iter()
                .all(|ancestor| ancestor.author != leader)
            {
                tracing::trace!(
                    "[{self}] {voting_block} is a blame for leader {}",
                    Slot::new(voting_round - 1, leader)
                );
                if blame_stake_aggregator.add(voter, &self.context.committee) {
                    return true;
                }
            } else {
                tracing::trace!(
                    "[{self}] {voting_block} is not a blame for leader {}",
                    Slot::new(voting_round - 1, leader)
                );
            }
        }
        false
    }

    /// Check whether the specified leader has 2f+1 certificates to be directly
    /// committed.
    fn enough_leader_support(&self, decision_round: Round, leader_block: &VerifiedBlock) -> bool {
        let decision_blocks = self
            .dag_state
            .read()
            .get_uncommitted_blocks_at_round(decision_round);

        // Quickly reject if there isn't enough stake to support the leader from
        // the potential certificates.
        let total_stake: Stake = decision_blocks
            .iter()
            .map(|b| self.context.committee.stake(b.author()))
            .sum();
        if !self.context.committee.reached_quorum(total_stake) {
            tracing::debug!(
                "Not enough support for {leader_block}. Stake not enough: {total_stake} < {}",
                self.context.committee.quorum_threshold()
            );
            return false;
        }

        let mut certificate_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        let mut all_votes = HashMap::new();
        for decision_block in &decision_blocks {
            let authority = decision_block.reference().author;
            if self.is_certificate(decision_block, leader_block, &mut all_votes)
                && certificate_stake_aggregator.add(authority, &self.context.committee)
            {
                return true;
            }
        }
        false
    }
}

impl Display for BaseCommitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Committer-L{}-R{}",
            self.options.leader_offset, self.options.round_offset
        )
    }
}

/// A builder for the base committer. By default, the builder creates a base committer
/// that has no leader or round offset. Which indicates single leader & pipelining
/// disabled.
#[cfg(test)]
mod base_committer_builder {
    use super::*;
    use crate::leader_schedule::LeaderSwapTable;

    pub(crate) struct BaseCommitterBuilder {
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        wave_length: u32,
        leader_offset: u32,
        round_offset: u32,
    }

    impl BaseCommitterBuilder {
        pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
            Self {
                context,
                dag_state,
                wave_length: DEFAULT_WAVE_LENGTH,
                leader_offset: 0,
                round_offset: 0,
            }
        }

        #[allow(unused)]
        pub(crate) fn with_wave_length(mut self, wave_length: u32) -> Self {
            self.wave_length = wave_length;
            self
        }

        #[allow(unused)]
        pub(crate) fn with_leader_offset(mut self, leader_offset: u32) -> Self {
            self.leader_offset = leader_offset;
            self
        }

        #[allow(unused)]
        pub(crate) fn with_round_offset(mut self, round_offset: u32) -> Self {
            self.round_offset = round_offset;
            self
        }

        pub(crate) fn build(self) -> BaseCommitter {
            let options = BaseCommitterOptions {
                wave_length: DEFAULT_WAVE_LENGTH,
                leader_offset: 0,
                round_offset: 0,
            };
            BaseCommitter::new(
                self.context.clone(),
                Arc::new(LeaderSchedule::new(
                    self.context,
                    LeaderSwapTable::default(),
                )),
                self.dag_state,
                options,
            )
        }
    }
}

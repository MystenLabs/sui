// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    block::{Block, BlockAPI, BlockRef},
    block_store::BlockStore,
    constants::{DEFAULT_WAVE_LENGTH, MINIMUM_WAVE_LENGTH},
    leader_schedule::LeaderSchedule,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    types::{AuthorityRound, LeaderStatus, Round, WaveNumber},
};

use std::{collections::HashMap, fmt::Display, sync::Arc};

use consensus_config::{AuthorityIndex, Committee, Stake};

pub struct BaseCommitterOptions {
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
#[allow(unused)]
pub struct BaseCommitter {
    /// The committee information
    committee: Arc<Committee>,
    /// The consensus leader schedule to be used to resolve the leader for a
    /// given round.
    leader_schedule: LeaderSchedule,
    /// Keep all block data
    block_store: BlockStore,
    /// The options used by this committer
    options: BaseCommitterOptions,
}

impl BaseCommitter {
    pub fn new(
        committee: Arc<Committee>,
        leader_schedule: LeaderSchedule,
        block_store: BlockStore,
    ) -> Self {
        Self {
            committee,
            leader_schedule,
            block_store,
            options: BaseCommitterOptions::default(),
        }
    }

    pub fn with_options(mut self, options: BaseCommitterOptions) -> Self {
        assert!(options.wave_length >= MINIMUM_WAVE_LENGTH);
        self.options = options;
        self
    }

    /// Return the wave in which the specified round belongs. This takes into
    /// account the round offset for when pipelining is enabled.
    fn wave_number(&self, round: Round) -> WaveNumber {
        round.saturating_sub(self.options.round_offset) / self.options.wave_length
    }

    /// Return the leader round of the specified wave. The leader round is always
    /// the first round of the wave. This takes into account round offset for when
    /// pipelining is enabled.
    fn leader_round(&self, wave: WaveNumber) -> Round {
        wave * self.options.wave_length + self.options.round_offset
    }

    /// Return the decision round of the specified wave. The decision round is
    /// always the last round of the wave. This takes into account round offset
    /// for when pipelining is enabled.
    fn decision_round(&self, wave: WaveNumber) -> Round {
        let wave_length = self.options.wave_length;
        wave * wave_length + wave_length - 1 + self.options.round_offset
    }

    pub fn elect_leader(&self, round: Round) -> Option<AuthorityRound> {
        let wave = self.wave_number(round);
        tracing::debug!(
            "elect_leader: round={}, wave={}, leader_round={}, leader_offset={}",
            round,
            wave,
            self.leader_round(wave),
            self.options.leader_offset
        );
        if self.leader_round(wave) != round {
            return None;
        }

        Some(AuthorityRound::new(
            self.leader_schedule
                .elect_leader(round, self.options.leader_offset),
            round,
        ))
    }

    /// Find which block is supported at (author, round) by the given block.
    /// Blocks can indirectly reference multiple other blocks at (author, round),
    /// but only one block at (author, round) will be supported by the given block.
    /// If block A supports B at (author, round), it is guaranteed that any
    /// processed block by the same author that directly or indirectly includes
    /// A will also support B at (author, round).
    fn find_supported_block(
        &self,
        search_author_round: AuthorityRound,
        from: &Block,
    ) -> Option<BlockRef> {
        if from.round() < search_author_round.round {
            return None;
        }
        for ancestor in from.ancestors() {
            if AuthorityRound::from(*ancestor) == search_author_round {
                return Some(*ancestor);
            }
            // Weak links may point to blocks with lower round numbers than strong links.
            if ancestor.round <= search_author_round.round {
                continue;
            }
            let ancestor = self
                .block_store
                .get_block(*ancestor)
                .expect("We should have the whole sub-dag by now");
            if let Some(support) = self.find_supported_block(search_author_round, &ancestor) {
                return Some(support);
            }
        }
        None
    }

    /// Check whether the specified block (`potential_vote`) is a vote for
    /// the specified leader (`leader_block`).
    fn is_vote(&self, potential_vote: &Block, leader_block: &Block) -> bool {
        let reference = leader_block.reference();
        let leader_author_round = AuthorityRound::from(reference);
        self.find_supported_block(leader_author_round, potential_vote) == Some(reference)
    }

    /// Check whether the specified block (`potential_certificate`) is a certificate
    /// for the specified leader (`leader_block`). An `all_votes` map can be
    /// provided as a cache to quickly skip checking against the block store on
    /// whether a reference is a vote. This is done for efficiency. Bear in mind
    /// that the `all_votes` should refer to votes considered to the same `leader_block`
    /// and it can't be reused for different leaders.
    fn is_certificate(
        &self,
        potential_certificate: &Block,
        leader_block: &Block,
        all_votes: &mut HashMap<BlockRef, bool>,
    ) -> bool {
        let mut votes_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for reference in potential_certificate.ancestors() {
            let is_vote = if let Some(is_vote) = all_votes.get(reference) {
                *is_vote
            } else {
                let potential_vote = self
                    .block_store
                    .get_block(*reference)
                    .expect("We should have the whole sub-dag by now");
                let is_vote = self.is_vote(&potential_vote, leader_block);
                all_votes.insert(*reference, is_vote);
                is_vote
            };

            if is_vote {
                tracing::trace!("[{self}] {reference} is a vote for {leader_block:?}");
                if votes_stake_aggregator.add(reference.author, &self.committee) {
                    return true;
                }
            } else {
                tracing::trace!("[{self}] {reference} is not a vote for {leader_block:?}");
            }
        }
        false
    }

    /// Decide the status of a target leader from the specified anchor. We commit
    /// the target leader if it has a certified link to the anchor. Otherwise, we
    /// skip the target leader.
    fn decide_leader_from_anchor(&self, anchor: &Block, leader: AuthorityRound) -> LeaderStatus {
        // Get the block(s) proposed by the leader. There could be more than one leader block
        // per round (produced by a Byzantine leader).
        let leader_blocks = self.block_store.get_blocks_at_authority_round(leader);

        // Get all blocks that could be potential certificates for the target leader. These blocks
        // are in the decision round of the target leader and are linked to the anchor.
        let wave = self.wave_number(leader.round);
        let decision_round = self.decision_round(wave);
        let potential_certificates = self.block_store.linked_to_round(anchor, decision_round);

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
            panic!("More than one certified block at wave {wave} from leader {leader}")
        }

        // We commit the target leader if it has a certificate that is an ancestor of the anchor.
        // Otherwise skip it.
        match certified_leader_blocks.pop() {
            Some(certified_leader_block) => LeaderStatus::Commit(certified_leader_block.clone()),
            None => LeaderStatus::Skip(leader),
        }
    }

    /// Check whether the specified leader has 2f+1 non-votes (blames) to be directly skipped.
    fn enough_leader_blame(&self, voting_round: Round, leader: AuthorityIndex) -> bool {
        let voting_blocks = self.block_store.get_blocks_by_round(voting_round);

        let mut blame_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for voting_block in &voting_blocks {
            let voter = voting_block.reference().author;
            if voting_block
                .ancestors()
                .iter()
                .all(|ancestor| ancestor.author != leader)
            {
                tracing::trace!(
                    "[{self}] {voting_block:?} is a blame for leader {}",
                    AuthorityRound::new(leader, voting_round - 1)
                );
                if blame_stake_aggregator.add(voter, &self.committee) {
                    return true;
                }
            }
        }
        false
    }

    /// Check whether the specified leader has 2f+1 certificates to be directly
    /// committed.
    fn enough_leader_support(&self, decision_round: Round, leader_block: &Block) -> bool {
        let decision_blocks = self.block_store.get_blocks_by_round(decision_round);

        // Quickly reject if there isn't enough stake to support the leader from
        // the potential certificates.
        let total_stake: Stake = decision_blocks
            .iter()
            .map(|b| self.committee.stake(b.author()))
            .sum();
        if total_stake < self.committee.quorum_threshold() {
            tracing::debug!(
                "Not enough support for: {}. Stake not enough: {} < {}",
                leader_block.round(),
                total_stake,
                self.committee.quorum_threshold()
            );
            return false;
        }

        let mut certificate_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        let mut all_votes = HashMap::new();
        for decision_block in &decision_blocks {
            let authority = decision_block.reference().author;
            if self.is_certificate(decision_block, leader_block, &mut all_votes) {
                tracing::trace!(
                    "[{self}] {decision_block:?} is a certificate for leader {leader_block:?}"
                );
                if certificate_stake_aggregator.add(authority, &self.committee) {
                    return true;
                }
            } else {
                tracing::trace!(
                    "[{self}] {decision_block:?} is not a certificate for leader {leader_block:?}"
                );
            }
        }
        false
    }

    /// Apply the indirect decision rule to the specified leader to see whether
    /// we can indirect-commit or indirect-skip it.
    #[tracing::instrument(skip_all, fields(leader = %leader))]
    pub fn try_indirect_decide<'a>(
        &self,
        leader: AuthorityRound,
        leaders: impl Iterator<Item = &'a LeaderStatus>,
    ) -> LeaderStatus {
        // The anchor is the first committed leader with round higher than the decision round of the
        // target leader. We must stop the iteration upon encountering an undecided leader.
        let anchors = leaders.filter(|x| leader.round + self.options.wave_length <= x.round());

        for anchor in anchors {
            tracing::trace!("[{self}] Trying to indirect-decide {leader} using anchor {anchor}",);
            match anchor {
                LeaderStatus::Commit(anchor) => {
                    return self.decide_leader_from_anchor(anchor, leader);
                }
                LeaderStatus::Skip(..) => (),
                LeaderStatus::Undecided(..) => break,
            }
        }

        LeaderStatus::Undecided(leader)
    }

    /// Apply the direct decision rule to the specified leader to see whether we
    /// can direct-commit or direct-skip it.
    #[tracing::instrument(skip_all, fields(leader = %leader))]
    pub fn try_direct_decide(&self, leader: AuthorityRound) -> LeaderStatus {
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
        let leader_blocks = self.block_store.get_blocks_at_authority_round(leader);
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

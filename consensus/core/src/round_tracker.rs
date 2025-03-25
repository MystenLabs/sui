// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! RoundTracker computes quorum rounds for the latest received and accepted rounds.
//! This round data is gathered from peers via RoundProber or via new Blocks received. Also
//! local accepted rounds are updated from new blocks proposed from this authority.
//!
//! Quorum rounds provides insight into how effectively each authority's blocks are propagated
//! and accepted across the network.

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Committee};
use itertools::Itertools;

use tracing::{debug, trace};

use crate::{
    block::{BlockAPI, ExtendedBlock},
    context::Context,
    Round,
};

/// A [`QuorumRound`] is a round range [low, high]. It is computed from
/// highest received or accepted rounds of an authority reported by all
/// authorities.
/// The bounds represent:
/// - the highest round lower or equal to rounds from a quorum (low)
/// - the lowest round higher or equal to rounds from a quorum (high)
///
/// [`QuorumRound`] is useful because:
/// - [low, high] range is BFT, always between the lowest and highest rounds
///   of honest validators, with < validity threshold of malicious stake.
/// - It provides signals about how well blocks from an authority propagates
///   in the network. If low bound for an authority is lower than its last
///   proposed round, the last proposed block has not propagated to a quorum.
///   If a new block is proposed from the authority, it will not get accepted
///   immediately by a quorum.
pub(crate) type QuorumRound = (Round, Round);

pub(crate) struct PeerRoundTracker {
    context: Arc<Context>,
    /// Highest accepted round per authority from received blocks (included/excluded ancestors)
    block_accepted_rounds: Vec<Vec<Round>>,
    /// Highest accepted round per authority from round prober
    probed_accepted_rounds: Vec<Vec<Round>>,
    /// Highest received round per authority from round prober
    probed_received_rounds: Vec<Vec<Round>>,
}

impl PeerRoundTracker {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let size = context.committee.size();
        Self {
            context,
            block_accepted_rounds: vec![vec![0; size]; size],
            probed_accepted_rounds: vec![vec![0; size]; size],
            probed_received_rounds: vec![vec![0; size]; size],
        }
    }

    /// Update accepted rounds based on a new block created locally or received from the network
    /// and its excluded ancestors
    pub(crate) fn update_from_accepted_block(&mut self, extended_block: &ExtendedBlock) {
        let block = &extended_block.block;
        let excluded_ancestors = &extended_block.excluded_ancestors;
        let author = block.author();

        // Update author accepted round from block round
        self.block_accepted_rounds[author][author] =
            self.block_accepted_rounds[author][author].max(block.round());

        // Update accepted rounds from included ancestors
        for ancestor in block.ancestors() {
            self.block_accepted_rounds[author][ancestor.author] =
                self.block_accepted_rounds[author][ancestor.author].max(ancestor.round);
        }

        // Update accepted rounds from excluded ancestors
        for excluded_ancestor in excluded_ancestors {
            self.block_accepted_rounds[author][excluded_ancestor.author] = self
                .block_accepted_rounds[author][excluded_ancestor.author]
                .max(excluded_ancestor.round);
        }
    }

    /// Update accepted & received rounds based on probing results
    pub(crate) fn update_from_probe(
        &mut self,
        accepted_rounds: Vec<Vec<Round>>,
        received_rounds: Vec<Vec<Round>>,
    ) {
        self.probed_accepted_rounds = accepted_rounds;
        self.probed_received_rounds = received_rounds;
    }

    // Returns the propagation delay of own blocks.
    pub(crate) fn calculate_propagation_delay(&self, last_proposed_round: Round) -> Round {
        let own_index = self.context.own_index;
        let node_metrics = &self.context.metrics.node_metrics;
        let received_quorum_rounds = self.compute_received_quorum_rounds();
        let accepted_quorum_rounds = self.compute_accepted_quorum_rounds();
        for ((low, high), (_, authority)) in received_quorum_rounds
            .iter()
            .zip(self.context.committee.authorities())
        {
            node_metrics
                .round_tracker_received_quorum_round_gaps
                .with_label_values(&[&authority.hostname])
                .set((high - low) as i64);
            node_metrics
                .round_tracker_low_received_quorum_round
                .with_label_values(&[&authority.hostname])
                .set(*low as i64);
            // The gap can be negative if this validator is lagging behind the network.
            node_metrics
                .round_tracker_current_received_round_gaps
                .with_label_values(&[&authority.hostname])
                .set(last_proposed_round as i64 - *low as i64);
        }

        for ((low, high), (_, authority)) in accepted_quorum_rounds
            .iter()
            .zip(self.context.committee.authorities())
        {
            node_metrics
                .round_tracker_accepted_quorum_round_gaps
                .with_label_values(&[&authority.hostname])
                .set((high - low) as i64);
            node_metrics
                .round_tracker_low_accepted_quorum_round
                .with_label_values(&[&authority.hostname])
                .set(*low as i64);
            // The gap can be negative if this validator is lagging behind the network.
            node_metrics
                .round_tracker_current_accepted_round_gaps
                .with_label_values(&[&authority.hostname])
                .set(last_proposed_round as i64 - *low as i64);
        }

        // TODO: consider using own quorum round gap to control proposing in addition to
        // propagation delay. For now they seem to be about the same.

        // It is possible more blocks arrive at a quorum of peers before the get_latest_rounds
        // requests arrive.
        // Using the lower bound to increase sensitivity about block propagation issues
        // that can reduce round rate.
        // Because of the nature of TCP and block streaming, propagation delay is expected to be
        // 0 in most cases, even when the actual latency of broadcasting blocks is high.
        // We will use the min propagation delay from either accepted or received rounds.
        // As stated above new blocks can arrive after the rounds have been probed, so its
        // likely accepted rounds from new blocks will provide us with the more accurate
        // propagation delay which is important because we now calculate the propagation
        // delay more frequently then before.
        let propagation_delay = last_proposed_round
            .saturating_sub(received_quorum_rounds[own_index].0)
            .min(last_proposed_round.saturating_sub(accepted_quorum_rounds[own_index].0));

        node_metrics
            .round_tracker_propagation_delays
            .observe(propagation_delay as f64);
        node_metrics
            .round_tracker_last_propagation_delay
            .set(propagation_delay as i64);

        debug!(
            "Computed propagation delay of {propagation_delay} based on last proposed \
                round ({last_proposed_round})."
        );

        propagation_delay
    }

    pub(crate) fn compute_accepted_quorum_rounds(&self) -> Vec<QuorumRound> {
        let highest_accepted_rounds = self
            .probed_accepted_rounds
            .iter()
            .zip(self.block_accepted_rounds.iter())
            .map(|(probed_rounds, block_rounds)| {
                probed_rounds
                    .iter()
                    .zip(block_rounds.iter())
                    .map(|(probed_round, block_round)| *probed_round.max(block_round))
                    .collect::<Vec<Round>>()
            })
            .collect::<Vec<Vec<Round>>>();
        let accepted_quorum_rounds = self
            .context
            .committee
            .authorities()
            .map(|(peer, _)| {
                compute_quorum_round(&self.context.committee, peer, &highest_accepted_rounds)
            })
            .collect::<Vec<_>>();

        trace!(
            "Computed accepted quorum round per authority: {}",
            self.context
                .committee
                .authorities()
                .zip(accepted_quorum_rounds.iter())
                .map(|((i, _), rounds)| format!("{i}: {rounds:?}"))
                .join(", ")
        );

        accepted_quorum_rounds
    }

    fn compute_received_quorum_rounds(&self) -> Vec<QuorumRound> {
        let received_quorum_rounds = self
            .context
            .committee
            .authorities()
            .map(|(peer, _)| {
                compute_quorum_round(&self.context.committee, peer, &self.probed_received_rounds)
            })
            .collect::<Vec<_>>();

        trace!(
            "Computed received quorum round per authority: {}",
            self.context
                .committee
                .authorities()
                .zip(received_quorum_rounds.iter())
                .map(|((i, _), rounds)| format!("{i}: {rounds:?}"))
                .join(", ")
        );

        received_quorum_rounds
    }
}

/// For the peer specified with target_index, compute and return its [`QuorumRound`].
fn compute_quorum_round(
    committee: &Committee,
    target_index: AuthorityIndex,
    highest_rounds: &[Vec<Round>],
) -> QuorumRound {
    let mut rounds_with_stake = highest_rounds
        .iter()
        .zip(committee.authorities())
        .map(|(rounds, (_, authority))| (rounds[target_index], authority.stake))
        .collect::<Vec<_>>();
    rounds_with_stake.sort();

    // Forward iteration and stopping at validity threshold would produce the same result currently,
    // with fault tolerance of f/3f+1 votes. But it is not semantically correct, and will provide an
    // incorrect value when fault tolerance and validity threshold are different.
    let mut total_stake = 0;
    let mut low = 0;
    for (round, stake) in rounds_with_stake.iter().rev() {
        total_stake += stake;
        if total_stake >= committee.quorum_threshold() {
            low = *round;
            break;
        }
    }

    let mut total_stake = 0;
    let mut high = 0;
    for (round, stake) in rounds_with_stake.iter() {
        total_stake += stake;
        if total_stake >= committee.quorum_threshold() {
            high = *round;
            break;
        }
    }

    (low, high)
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use consensus_config::AuthorityIndex;

    use crate::{
        block::{BlockDigest, ExtendedBlock},
        context::Context,
        round_tracker::{compute_quorum_round, PeerRoundTracker},
        BlockRef, TestBlock, VerifiedBlock,
    };

    #[tokio::test]
    async fn test_compute_quorum_round() {
        let (context, _) = Context::new_for_test(4);

        // Observe latest rounds from peers.
        let highest_received_rounds = vec![
            vec![10, 11, 12, 13],
            vec![5, 2, 7, 4],
            vec![0, 0, 0, 0],
            vec![3, 4, 5, 6],
        ];

        let round = compute_quorum_round(
            &context.committee,
            AuthorityIndex::new_for_test(0),
            &highest_received_rounds,
        );
        assert_eq!(round, (3, 5));

        let round = compute_quorum_round(
            &context.committee,
            AuthorityIndex::new_for_test(1),
            &highest_received_rounds,
        );
        assert_eq!(round, (2, 4));

        let round = compute_quorum_round(
            &context.committee,
            AuthorityIndex::new_for_test(2),
            &highest_received_rounds,
        );
        assert_eq!(round, (5, 7));

        let round = compute_quorum_round(
            &context.committee,
            AuthorityIndex::new_for_test(3),
            &highest_received_rounds,
        );
        assert_eq!(round, (4, 6));
    }

    #[tokio::test]
    async fn test_compute_received_quorum_round() {
        telemetry_subscribers::init_for_testing();
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let mut round_tracker = PeerRoundTracker::new(context);

        // Observe latest rounds from peers.
        let highest_received_rounds = vec![
            vec![10, 11, 12, 13],
            vec![5, 2, 7, 4],
            vec![0, 0, 0, 0],
            vec![3, 4, 5, 6],
        ];

        let expected_received_quorum_rounds = vec![(3, 5), (2, 4), (5, 7), (4, 6)];

        round_tracker.update_from_probe(vec![], highest_received_rounds);

        let received_quourum_rounds = round_tracker.compute_received_quorum_rounds();

        assert_eq!(expected_received_quorum_rounds, received_quourum_rounds);
    }

    #[tokio::test]
    async fn test_compute_accepted_quorum_round() {
        const NUM_AUTHORITIES: usize = 4;
        let (context, _) = Context::new_for_test(NUM_AUTHORITIES);
        let context = Arc::new(context);
        let own_index = context.own_index.value() as u32;
        let mut round_tracker = PeerRoundTracker::new(context);

        // Observe latest rounds from peers.
        let highest_accepted_rounds = vec![
            vec![10, 11, 12, 13],
            vec![5, 2, 7, 4],
            vec![0, 0, 0, 0],
            vec![3, 4, 5, 6],
        ];

        round_tracker.update_from_probe(highest_accepted_rounds, vec![]);

        // Simulate accepting a block from authority 3
        let test_block = TestBlock::new(7, 2)
            .set_ancestors(vec![BlockRef::new(
                6,
                AuthorityIndex::new_for_test(3),
                BlockDigest::MIN,
            )])
            .build();
        let block = VerifiedBlock::new_for_test(test_block);
        round_tracker.update_from_accepted_block(&ExtendedBlock {
            block,
            excluded_ancestors: vec![BlockRef::new(
                8,
                AuthorityIndex::new_for_test(1),
                BlockDigest::MIN,
            )],
        });

        // Simulate proposing a new block
        // note: not valid rounds, but tests that the max value will always be
        // considered in calculations.
        let test_block = TestBlock::new(11, own_index)
            .set_ancestors(vec![
                BlockRef::new(7, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
                BlockRef::new(6, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
            ])
            .build();
        let block = VerifiedBlock::new_for_test(test_block);
        round_tracker.update_from_accepted_block(&ExtendedBlock {
            block,
            excluded_ancestors: vec![BlockRef::new(
                8,
                AuthorityIndex::new_for_test(1),
                BlockDigest::MIN,
            )],
        });

        // Compute quorum rounds based on highest accepted rounds (max from prober
        // or from blocks):
        // 11, 11, 12, 13
        //  5,  2,  7,  4
        //  0,  8,  7,  6
        //  3,  4,  5,  6

        let expected_accepted_quorum_rounds = vec![(3, 5), (4, 8), (7, 7), (6, 6)];
        let accepted_quourum_rounds = round_tracker.compute_accepted_quorum_rounds();

        assert_eq!(expected_accepted_quorum_rounds, accepted_quourum_rounds);
    }

    #[tokio::test]
    async fn test_quorum_round_manager() {
        const NUM_AUTHORITIES: usize = 7;
        let context = Arc::new(Context::new_for_test(NUM_AUTHORITIES).0);

        let highest_received_rounds = vec![
            vec![110, 120, 130, 140, 150, 160, 170],
            vec![109, 121, 131, 0, 151, 161, 171],
            vec![101, 0, 103, 104, 105, 166, 107],
            vec![0, 0, 0, 0, 0, 0, 0],
            vec![100, 102, 133, 0, 155, 106, 177],
            vec![105, 115, 103, 0, 125, 126, 127],
            vec![0, 0, 0, 0, 0, 0, 0],
        ];

        let highest_accepted_rounds = vec![
            vec![110, 120, 130, 140, 150, 160, 170],
            vec![0, 121, 131, 0, 151, 161, 171],
            vec![1, 0, 103, 104, 105, 166, 107],
            vec![0, 0, 0, 0, 0, 0, 0],
            vec![0, 102, 133, 0, 155, 106, 177],
            vec![1, 115, 103, 0, 125, 126, 127],
            vec![0, 0, 0, 0, 0, 0, 0],
        ];

        let mut round_tracker = PeerRoundTracker::new(context.clone());

        round_tracker.update_from_probe(highest_accepted_rounds, highest_received_rounds);

        // Create test blocks for each authority with incrementing rounds starting at 110
        for authority in 0..NUM_AUTHORITIES {
            let round = 110 + (authority as u32 * 10);
            let block =
                VerifiedBlock::new_for_test(TestBlock::new(round, authority as u32).build());
            round_tracker.update_from_accepted_block(&ExtendedBlock {
                block,
                excluded_ancestors: vec![],
            });
        }

        // Compute quorum rounds and propagation delay based on last proposed round = 110,
        // and highest received rounds:
        // 110, 120, 130, 140, 150, 160, 170,
        // 109, 121, 131, 0,   151, 161, 171,
        // 101, 0,   103, 104, 105, 166, 107,
        // 0,   0,   0,   0,   0,   0,   0,
        // 100, 102, 133, 0,   155, 106, 177,
        // 105, 115, 103, 0,   125, 126, 127,
        // 0,   0,   0,   0,   0,   0,   0,

        let received_quorum_rounds = round_tracker.compute_received_quorum_rounds();
        let accepted_quorum_rounds = round_tracker.compute_accepted_quorum_rounds();
        assert_eq!(
            received_quorum_rounds,
            vec![
                (100, 105),
                (0, 115),
                (103, 130),
                (0, 0),
                (105, 150),
                (106, 160),
                (107, 170)
            ]
        );

        // Compute quorum rounds based on highest accepted rounds (max from prober
        // or from blocks):
        // 110, 120, 130, 140, 150, 160, 170,
        //   0, 121, 131,   0, 151, 161, 171,
        //   1,   0, 130, 104, 105, 166, 107,
        //   0,   0,   0, 140,   0,   0,   0,
        //   0, 102, 133,   0, 155, 106, 177,
        //   1, 115, 103,   0, 125, 160, 127,
        //   0,   0,   0,   0,   0,   0, 170,

        assert_eq!(
            accepted_quorum_rounds,
            vec![
                (0, 1),
                (0, 115),
                (103, 130),
                (0, 104),
                (105, 150),
                (106, 160),
                (127, 170)
            ]
        );

        let propagation_delay = round_tracker.calculate_propagation_delay(110);

        // 110 - 100 = 10
        assert_eq!(propagation_delay, 10);
    }
}

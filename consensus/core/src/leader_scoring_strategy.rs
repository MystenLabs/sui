// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, ops::Range};

use crate::{
    block::{BlockAPI, BlockRef, Slot},
    commit::DEFAULT_WAVE_LENGTH,
    leader_scoring::UnscoredSubdag,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

#[allow(unused)]
pub(crate) trait ScoringStrategy: Send + Sync {
    fn calculate_scores_for_leader(&self, subdag: &UnscoredSubdag, leader_slot: Slot) -> Vec<u64>;

    // Based on the scoring strategy there is a minimum number of rounds required
    // for the scores to be calculated. This method allows that to be set by the
    // scoring strategy.
    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32>;
}

/// This scoring strategy is like `CertifiedVoteScoringStrategyV1` but instead of
/// only giving one point for each vote that is included in 2f+1 certificates. We
/// give a score equal to the amount of stake of all certificates that included
/// the vote.
pub(crate) struct CertifiedVoteScoringStrategyV2 {}

impl ScoringStrategy for CertifiedVoteScoringStrategyV2 {
    fn calculate_scores_for_leader(&self, subdag: &UnscoredSubdag, leader_slot: Slot) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let decision_round = leader_slot.round + DEFAULT_WAVE_LENGTH - 1;

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::trace!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
            return scores_per_authority;
        }

        // At this point we are guaranteed that there is only one leader per slot
        // because we are operating on committed subdags.
        assert!(leader_blocks.len() == 1);

        let leader_block = leader_blocks.first().unwrap();

        let decision_blocks = subdag.get_blocks_at_round(decision_round);

        let mut all_votes: HashMap<BlockRef, (bool, StakeAggregator<QuorumThreshold>)> =
            HashMap::new();
        for potential_cert in decision_blocks {
            let authority = potential_cert.reference().author;
            for reference in potential_cert.ancestors() {
                if let Some((is_vote, stake_agg)) = all_votes.get_mut(reference) {
                    if *is_vote {
                        stake_agg.add(authority, &subdag.context.committee);
                    }
                } else if let Some(potential_vote) = subdag.get_block(reference) {
                    let is_vote = subdag.is_vote(&potential_vote, leader_block);
                    let mut stake_agg = StakeAggregator::<QuorumThreshold>::new();
                    stake_agg.add(authority, &subdag.context.committee);
                    all_votes.insert(*reference, (is_vote, stake_agg));
                } else {
                    tracing::trace!(
                        "Potential vote not found in unscored committed subdags: {:?}",
                        reference
                    );
                };
            }
        }

        for (vote_ref, (is_vote, stake_agg)) in all_votes {
            if is_vote {
                let authority = vote_ref.author;
                tracing::trace!(
                    "Found a certified vote {vote_ref} for leader {leader_block} from authority {authority}"
                );
                tracing::trace!(
                    "[{}] scores +{} reputation for {authority}!",
                    subdag.context.own_index,
                    stake_agg.stake()
                );
                scores_per_authority[authority] += stake_agg.stake();
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        // To be able to calculate scores using certified votes we require +1 round
        // for the votes on the leader and +1 round for the certificates of those votes.
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

/// This scoring strategy gives one point for each authority vote that is included
/// in 2f+1 certificates. We are calling this a certified vote.
pub(crate) struct CertifiedVoteScoringStrategyV1 {}

impl ScoringStrategy for CertifiedVoteScoringStrategyV1 {
    fn calculate_scores_for_leader(&self, subdag: &UnscoredSubdag, leader_slot: Slot) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let decision_round = leader_slot.round + DEFAULT_WAVE_LENGTH - 1;

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::trace!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
            return scores_per_authority;
        }

        // At this point we are guaranteed that there is only one leader per slot
        // because we are operating on committed subdags.
        assert!(leader_blocks.len() == 1);

        let leader_block = leader_blocks.first().unwrap();

        let decision_blocks = subdag.get_blocks_at_round(decision_round);

        let mut all_votes: HashMap<BlockRef, (bool, StakeAggregator<QuorumThreshold>)> =
            HashMap::new();
        for potential_cert in decision_blocks {
            let authority = potential_cert.reference().author;
            for reference in potential_cert.ancestors() {
                if let Some((is_vote, stake_agg)) = all_votes.get_mut(reference) {
                    if *is_vote {
                        stake_agg.add(authority, &subdag.context.committee);
                    }
                } else if let Some(potential_vote) = subdag.get_block(reference) {
                    let is_vote = subdag.is_vote(&potential_vote, leader_block);
                    let mut stake_agg = StakeAggregator::<QuorumThreshold>::new();
                    stake_agg.add(authority, &subdag.context.committee);
                    all_votes.insert(*reference, (is_vote, stake_agg));
                } else {
                    tracing::trace!(
                        "Potential vote not found in unscored committed subdags: {:?}",
                        reference
                    );
                };
            }
        }

        for (vote_ref, (is_vote, stake_agg)) in all_votes {
            if is_vote && stake_agg.reached_threshold(&subdag.context.committee) {
                let authority = vote_ref.author;
                tracing::trace!(
                    "Found a certified vote {vote_ref} for leader {leader_block} from authority {authority}"
                );
                tracing::trace!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        // To be able to calculate scores using certified votes we require +1 round
        // for the votes on the leader and +1 round for the certificates of those votes.
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

// This scoring strategy will give one point to any votes for the leader.
pub(crate) struct VoteScoringStrategy {}

impl ScoringStrategy for VoteScoringStrategy {
    fn calculate_scores_for_leader(&self, subdag: &UnscoredSubdag, leader_slot: Slot) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::trace!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
            return scores_per_authority;
        }

        // At this point we are guaranteed that there is only one leader per slot
        // because we are operating on committed subdags.
        assert!(leader_blocks.len() == 1);

        let leader_block = leader_blocks.first().unwrap();

        let voting_round = leader_slot.round + 1;
        let voting_blocks = subdag.get_blocks_at_round(voting_round);
        for potential_vote in voting_blocks {
            // TODO: use the decided leader as input instead of leader slot. If the leader was skipped,
            // votes to skip should be included in the score as well.
            if subdag.is_vote(&potential_vote, leader_block) {
                let authority = potential_vote.author();
                tracing::trace!(
                    "Found a vote {} for leader {leader_block} from authority {authority}",
                    potential_vote.reference()
                );
                tracing::trace!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        // To be able to calculate scores using votes we require +1 round
        // for the votes on the leader.
        assert!(min_round < max_round);
        min_round..max_round
    }
}

// This scoring strategy will give one point to any certificates for the leader.
pub(crate) struct CertificateScoringStrategy {}

impl ScoringStrategy for CertificateScoringStrategy {
    fn calculate_scores_for_leader(&self, subdag: &UnscoredSubdag, leader_slot: Slot) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let decision_round = leader_slot.round + DEFAULT_WAVE_LENGTH - 1;

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::trace!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
            return scores_per_authority;
        }

        // At this point we are guaranteed that there is only one leader per slot
        // because we are operating on committed subdags.
        assert!(leader_blocks.len() == 1);

        let leader_block = leader_blocks.first().unwrap();

        let decision_blocks = subdag.get_blocks_at_round(decision_round);
        let mut all_votes = HashMap::new();
        for potential_cert in decision_blocks {
            let authority = potential_cert.reference().author;
            if subdag.is_certificate(&potential_cert, leader_block, &mut all_votes) {
                tracing::trace!(
                    "Found a certificate {} for leader {leader_block} from authority {authority}",
                    potential_cert.reference()
                );
                tracing::trace!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        // To be able to calculate scores using certificates we require +1 round
        // for the votes on the leader and +1 round for the certificates of those votes.
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use std::{cmp::max, sync::Arc};

    use consensus_config::AuthorityIndex;

    use super::*;
    use crate::{
        commit::CommittedSubDag, context::Context, leader_scoring::ReputationScoreCalculator,
        test_dag_builder::DagBuilder,
    };

    #[tokio::test]
    async fn test_certificate_scoring_strategy() {
        let (context, unscored_subdags) = basic_setup();
        let scoring_strategy = CertificateScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![2, 1, 1, 1]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    async fn test_vote_scoring_strategy() {
        let (context, unscored_subdags) = basic_setup();
        let scoring_strategy = VoteScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    async fn test_certified_vote_scoring_strategy_v1() {
        let (context, unscored_subdags) = basic_setup();
        let scoring_strategy = CertifiedVoteScoringStrategyV1 {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![1, 1, 1, 1]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    async fn test_certified_vote_scoring_strategy_v2() {
        let (context, unscored_subdags) = basic_setup();
        let scoring_strategy = CertifiedVoteScoringStrategyV2 {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![5, 5, 5, 5]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    fn basic_setup() -> (Arc<Context>, Vec<CommittedSubDag>) {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        // Populate fully connected test blocks for round 0 ~ 3, authorities 0 ~ 3.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=3).build();
        // Build round 4 but with just the leader block
        dag_builder
            .layer(4)
            .authorities(vec![
                AuthorityIndex::new_for_test(1),
                AuthorityIndex::new_for_test(2),
                AuthorityIndex::new_for_test(3),
            ])
            .skip_block()
            .build();

        let leaders = dag_builder
            .leader_blocks(1..=4)
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let mut unscored_subdags = vec![];
        let mut last_committed_rounds = vec![0; 4];
        for (idx, leader) in leaders.into_iter().enumerate() {
            let commit_index = idx as u32 + 1;
            let (subdag, _commit) = dag_builder.get_sub_dag_and_commit(
                leader,
                last_committed_rounds.clone(),
                commit_index,
            );
            for block in subdag.blocks.iter() {
                last_committed_rounds[block.author().value()] =
                    max(block.round(), last_committed_rounds[block.author().value()]);
            }
            unscored_subdags.push(subdag);
        }
        (context, unscored_subdags)
    }
}

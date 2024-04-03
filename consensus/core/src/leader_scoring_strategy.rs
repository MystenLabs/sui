// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, ops::Range};

use crate::{
    base_committer::BaseCommitter,
    block::{BlockAPI, BlockRef, Slot},
    leader_scoring::UnscoredSubdag,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

pub(crate) trait ScoringStrategy {
    fn calculate_scores_for_leader(
        &self,
        subdag: &UnscoredSubdag,
        leader_slot: Slot,
        committer: &BaseCommitter,
    ) -> Vec<u64>;

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
    fn calculate_scores_for_leader(
        &self,
        subdag: &UnscoredSubdag,
        leader_slot: Slot,
        committer: &BaseCommitter,
    ) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let wave = committer.wave_number(leader_slot.round);
        let decision_round = committer.decision_round(wave);

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::info!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
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
                    tracing::info!(
                        "Potential vote not found in unscored committed subdags: {:?}",
                        reference
                    );
                };
            }
        }

        for (vote_ref, (is_vote, stake_agg)) in all_votes {
            if is_vote {
                let authority = vote_ref.author;
                tracing::info!(
                    "Found a certified vote {vote_ref} for leader {leader_block} from authority {authority}"
                );
                tracing::info!(
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
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

/// This scoring strategy gives one point for each authority vote that is included
/// in 2f+1 certificates. We are calling this a certified vote.
pub(crate) struct CertifiedVoteScoringStrategyV1 {}

impl ScoringStrategy for CertifiedVoteScoringStrategyV1 {
    fn calculate_scores_for_leader(
        &self,
        subdag: &UnscoredSubdag,
        leader_slot: Slot,
        committer: &BaseCommitter,
    ) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let wave = committer.wave_number(leader_slot.round);
        let decision_round = committer.decision_round(wave);

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::info!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
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
                    tracing::info!(
                        "Potential vote not found in unscored committed subdags: {:?}",
                        reference
                    );
                };
            }
        }

        for (vote_ref, (is_vote, stake_agg)) in all_votes {
            if is_vote && stake_agg.reached_threshold(&subdag.context.committee) {
                let authority = vote_ref.author;
                tracing::info!(
                    "Found a certified vote {vote_ref} for leader {leader_block} from authority {authority}"
                );
                tracing::info!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

// This scoring strategy will give one point to any votes for the leader.
pub(crate) struct VoteScoringStrategy {}

impl ScoringStrategy for VoteScoringStrategy {
    fn calculate_scores_for_leader(
        &self,
        subdag: &UnscoredSubdag,
        leader_slot: Slot,
        _committer: &BaseCommitter,
    ) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];
        let voting_round = leader_slot.round + 1;

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::info!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
            return scores_per_authority;
        }

        // At this point we are guaranteed that there is only one leader per slot
        // because we are operating on committed subdags.
        assert!(leader_blocks.len() == 1);

        let leader_block = leader_blocks.first().unwrap();

        let voting_blocks = subdag.get_blocks_at_round(voting_round);
        for potential_vote in voting_blocks {
            if subdag.is_vote(&potential_vote, leader_block) {
                let authority = potential_vote.author();
                tracing::info!(
                    "Found a vote {} for leader {leader_block} from authority {authority}",
                    potential_vote.reference()
                );
                tracing::info!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        assert!(min_round < max_round);
        min_round..max_round
    }
}

// This scoring strategy will give one point to any certificates for the leader.
pub(crate) struct CertificateScoringStrategy {}

impl ScoringStrategy for CertificateScoringStrategy {
    fn calculate_scores_for_leader(
        &self,
        subdag: &UnscoredSubdag,
        leader_slot: Slot,
        committer: &BaseCommitter,
    ) -> Vec<u64> {
        let num_authorities = subdag.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        let wave = committer.wave_number(leader_slot.round);
        let decision_round = committer.decision_round(wave);

        let leader_blocks = subdag.get_blocks_at_slot(leader_slot);

        if leader_blocks.is_empty() {
            tracing::info!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", subdag.context.own_index);
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
                tracing::info!(
                    "Found a certificate {} for leader {leader_block} from authority {authority}",
                    potential_cert.reference()
                );
                tracing::info!(
                    "[{}] scores +1 reputation for {authority}!",
                    subdag.context.own_index
                );
                scores_per_authority[authority] += 1;
            }
        }

        scores_per_authority
    }

    fn leader_scoring_round_range(&self, min_round: u32, max_round: u32) -> Range<u32> {
        assert!(min_round < max_round - 1);
        min_round..max_round.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::RwLock;

    use super::*;
    use crate::{
        block::{timestamp_utc_ms, BlockTimestampMs, TestBlock, VerifiedBlock},
        commit::CommitRange,
        context::Context,
        dag_state::DagState,
        leader_schedule::{LeaderSchedule, LeaderSwapTable},
        leader_scoring::ReputationScoreCalculator,
        storage::mem_store::MemStore,
        universal_committer::universal_committer_builder::UniversalCommitterBuilder,
        CommittedSubDag,
    };

    #[test]
    fn test_certificate_scoring_strategy() {
        let (context, committer, unscored_subdags) = basic_setup();

        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(CertificateScoringStrategy {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![2, 1, 1, 1]);
        assert_eq!(scores.commit_range, CommitRange::new(1..1));
    }

    #[test]
    fn test_vote_scoring_strategy() {
        let (context, committer, unscored_subdags) = basic_setup();

        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(VoteScoringStrategy {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, CommitRange::new(1..1));
    }

    #[test]
    fn test_certified_vote_scoring_strategy_v1() {
        let (context, committer, unscored_subdags) = basic_setup();

        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(CertifiedVoteScoringStrategyV1 {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![1, 1, 1, 1]);
        assert_eq!(scores.commit_range, CommitRange::new(1..1));
    }

    #[test]
    fn test_certified_vote_scoring_strategy_v2() {
        let (context, committer, unscored_subdags) = basic_setup();

        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(CertifiedVoteScoringStrategyV2 {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![5, 5, 5, 5]);
        assert_eq!(scores.commit_range, CommitRange::new(1..1));
    }

    fn basic_setup() -> (
        Arc<Context>,
        crate::universal_committer::UniversalCommitter,
        Vec<CommittedSubDag>,
    ) {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let leader_schedule = Arc::new(LeaderSchedule::new(
            context.clone(),
            LeaderSwapTable::default(),
        ));
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let committer = UniversalCommitterBuilder::new(
            context.clone(),
            leader_schedule.clone(),
            dag_state.clone(),
        )
        .with_pipeline(true)
        .build();

        // Populate fully connected test blocks for round 0 ~ 4, authorities 0 ~ 3.
        let max_round: u32 = 4;
        let num_authorities: u32 = 4;

        let mut blocks = Vec::new();
        let (genesis_references, genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        blocks.extend(genesis);

        let mut ancestors = genesis_references;
        let mut leader = None;
        for round in 1..=max_round {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                new_ancestors.push(block.reference());
                blocks.push(block.clone());

                // only write one block for the final round, which is the leader
                // of the committed subdag.
                if round == max_round {
                    leader = Some(block.clone());
                    break;
                }
            }
            ancestors = new_ancestors;
        }

        let leader_block = leader.unwrap();
        let leader_ref = leader_block.reference();
        let commit_index = 1;

        let unscored_subdags = vec![CommittedSubDag::new(
            leader_ref,
            blocks,
            timestamp_utc_ms(),
            commit_index,
        )];
        (context, committer, unscored_subdags)
    }
}

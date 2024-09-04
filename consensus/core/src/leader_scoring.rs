// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    ops::Bound::{Excluded, Included},
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use serde::{Deserialize, Serialize};

use crate::{
    block::{BlockAPI, BlockDigest, BlockRef, Slot, VerifiedBlock},
    commit::{CommitRange, CommittedSubDag},
    context::Context,
    leader_scoring_strategy::ScoringStrategy,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    Round,
};

pub(crate) struct ReputationScoreCalculator<'a> {
    // The range of commits that these scores are calculated from.
    pub(crate) commit_range: CommitRange,
    // The scores per authority. Vec index is the `AuthorityIndex`.
    pub(crate) scores_per_authority: Vec<u64>,

    // As leaders are sequenced the subdags are collected and cached in `DagState`.
    // Then when there are enough commits to trigger a `LeaderSchedule` change,
    // the subdags are then combined into one `UnscoredSubdag` so that we can
    // calculate the scores for the leaders in this subdag.
    unscored_subdag: UnscoredSubdag,
    // There are multiple scoring strategies that can be used to calculate the scores
    // and the `ReputationScoreCalculator` is responsible for applying the strategy.
    // For now this is dynamic while we are experimenting but eventually we can
    // replace this with the final strategy that works best.
    scoring_strategy: &'a dyn ScoringStrategy,
}

impl<'a> ReputationScoreCalculator<'a> {
    pub(crate) fn new(
        context: Arc<Context>,
        unscored_subdags: &[CommittedSubDag],
        scoring_strategy: &'a dyn ScoringStrategy,
    ) -> Self {
        let num_authorities = context.committee.size();
        let scores_per_authority = vec![0_u64; num_authorities];

        assert!(
            !unscored_subdags.is_empty(),
            "Attempted to calculate scores with no unscored subdags"
        );

        let unscored_subdag = UnscoredSubdag::new(context.clone(), unscored_subdags);
        let commit_range = unscored_subdag.commit_range.clone();

        Self {
            unscored_subdag,
            commit_range,
            scoring_strategy,
            scores_per_authority,
        }
    }

    pub(crate) fn calculate(&mut self) -> ReputationScores {
        let leaders = self.unscored_subdag.committed_leaders.clone();
        for leader in leaders {
            let leader_slot = Slot::from(leader);
            tracing::trace!("Calculating score for leader {leader_slot}");
            self.add_scores(
                self.scoring_strategy
                    .calculate_scores_for_leader(&self.unscored_subdag, leader_slot),
            );
        }

        ReputationScores::new(self.commit_range.clone(), self.scores_per_authority.clone())
    }

    fn add_scores(&mut self, scores: Vec<u64>) {
        assert_eq!(scores.len(), self.scores_per_authority.len());

        for (authority_idx, score) in scores.iter().enumerate() {
            self.scores_per_authority[authority_idx] += *score;
        }
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReputationScores {
    /// Score per authority. Vec index is the `AuthorityIndex`.
    pub(crate) scores_per_authority: Vec<u64>,
    // The range of commits these scores were calculated from.
    pub(crate) commit_range: CommitRange,
}

impl ReputationScores {
    pub(crate) fn new(commit_range: CommitRange, scores_per_authority: Vec<u64>) -> Self {
        Self {
            scores_per_authority,
            commit_range,
        }
    }

    // Returns the authorities index with score tuples.
    pub(crate) fn authorities_by_score(&self, context: Arc<Context>) -> Vec<(AuthorityIndex, u64)> {
        self.scores_per_authority
            .iter()
            .enumerate()
            .map(|(index, score)| {
                (
                    context
                        .committee
                        .to_authority_index(index)
                        .expect("Should be a valid AuthorityIndex"),
                    *score,
                )
            })
            .collect()
    }

    pub(crate) fn update_metrics(&self, context: Arc<Context>) {
        for (index, score) in self.scores_per_authority.iter().enumerate() {
            let authority_index = context
                .committee
                .to_authority_index(index)
                .expect("Should be a valid AuthorityIndex");
            let authority = context.committee.authority(authority_index);
            if !authority.hostname.is_empty() {
                context
                    .metrics
                    .node_metrics
                    .reputation_scores
                    .with_label_values(&[&authority.hostname])
                    .set(*score as i64);
            }
        }
    }
}

/// UnscoredSubdag represents a collection of subdags across multiple commits.
/// These subdags are considered unscored for the purposes of leader schedule
/// change. On a leader schedule change, reputation scores will be calculated
/// based on the dags collected in this struct. Similar graph traversal methods
/// that are provided in DagState are also added here to help calculate the
/// scores.
pub(crate) struct UnscoredSubdag {
    pub(crate) context: Arc<Context>,
    pub(crate) commit_range: CommitRange,
    pub(crate) committed_leaders: Vec<BlockRef>,
    // When the blocks are collected form the list of provided subdags we ensure
    // that the CommittedSubDag instances are contiguous in commit index order.
    // Therefore we can guarantee the blocks of UnscoredSubdag are also sorted
    // via the commit index.
    pub(crate) blocks: BTreeMap<BlockRef, VerifiedBlock>,
}

impl UnscoredSubdag {
    pub(crate) fn new(context: Arc<Context>, subdags: &[CommittedSubDag]) -> Self {
        let mut committed_leaders = vec![];
        let blocks = subdags
            .iter()
            .enumerate()
            .flat_map(|(subdag_index, subdag)| {
                committed_leaders.push(subdag.leader);
                if subdag_index == 0 {
                    subdag.blocks.iter()
                } else {
                    let previous_subdag = &subdags[subdag_index - 1];
                    let expected_next_subdag_index = previous_subdag.commit_ref.index + 1;
                    assert_eq!(
                        subdag.commit_ref.index, expected_next_subdag_index,
                        "Non-contiguous commit index (expected: {}, found: {})",
                        expected_next_subdag_index, subdag.commit_ref.index
                    );
                    subdag.blocks.iter()
                }
            })
            .map(|block| (block.reference(), block.clone()))
            .collect::<BTreeMap<_, _>>();

        // Guaranteed to have a contiguous list of commit indices
        let commit_range = CommitRange::new(
            subdags.first().unwrap().commit_ref.index..=subdags.last().unwrap().commit_ref.index,
        );

        assert!(
            !blocks.is_empty(),
            "Attempted to create UnscoredSubdag with no blocks"
        );

        Self {
            context,
            commit_range,
            committed_leaders,
            blocks,
        }
    }

    pub(crate) fn find_supported_leader_block(
        &self,
        leader_slot: Slot,
        from: &VerifiedBlock,
    ) -> Option<BlockRef> {
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
            if let Some(ancestor) = self.get_block(ancestor) {
                if let Some(support) = self.find_supported_leader_block(leader_slot, &ancestor) {
                    return Some(support);
                }
            } else {
                // TODO: Add unit test for this case once dagbuilder is ready.
                tracing::trace!(
                    "Potential vote's ancestor block not found in unscored committed subdags: {:?}",
                    ancestor
                );
                return None;
            }
        }
        None
    }

    pub(crate) fn is_vote(
        &self,
        potential_vote: &VerifiedBlock,
        leader_block: &VerifiedBlock,
    ) -> bool {
        let reference = leader_block.reference();
        let leader_slot = Slot::from(reference);
        self.find_supported_leader_block(leader_slot, potential_vote) == Some(reference)
    }

    pub(crate) fn is_certificate(
        &self,
        potential_certificate: &VerifiedBlock,
        leader_block: &VerifiedBlock,
        all_votes: &mut HashMap<BlockRef, bool>,
    ) -> bool {
        let mut votes_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for reference in potential_certificate.ancestors() {
            let is_vote = if let Some(is_vote) = all_votes.get(reference) {
                *is_vote
            } else if let Some(potential_vote) = self.get_block(reference) {
                let is_vote = self.is_vote(&potential_vote, leader_block);
                all_votes.insert(*reference, is_vote);
                is_vote
            } else {
                tracing::trace!(
                    "Potential vote not found in unscored committed subdags: {:?}",
                    reference
                );
                false
            };

            if is_vote {
                tracing::trace!("{reference} is a vote for {leader_block}");
                if votes_stake_aggregator.add(reference.author, &self.context.committee) {
                    tracing::trace!(
                        "{potential_certificate} is a certificate for leader {leader_block}"
                    );
                    return true;
                }
            } else {
                tracing::trace!("{reference} is not a vote for {leader_block}",);
            }
        }
        tracing::trace!("{potential_certificate} is not a certificate for leader {leader_block}");
        false
    }

    pub(crate) fn get_blocks_at_slot(&self, slot: Slot) -> Vec<VerifiedBlock> {
        let mut blocks = vec![];
        for (_block_ref, block) in self.blocks.range((
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MIN)),
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MAX)),
        )) {
            blocks.push(block.clone())
        }
        blocks
    }

    pub(crate) fn get_blocks_at_round(&self, round: Round) -> Vec<VerifiedBlock> {
        let mut blocks = vec![];
        for (_block_ref, block) in self.blocks.range((
            Included(BlockRef::new(round, AuthorityIndex::ZERO, BlockDigest::MIN)),
            Excluded(BlockRef::new(
                round + 1,
                AuthorityIndex::ZERO,
                BlockDigest::MIN,
            )),
        )) {
            blocks.push(block.clone())
        }
        blocks
    }

    pub(crate) fn get_block(&self, block_ref: &BlockRef) -> Option<VerifiedBlock> {
        self.blocks.get(block_ref).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::max;

    use super::*;
    use crate::commit::{CommitDigest, CommitRef};
    use crate::{leader_scoring_strategy::VoteScoringStrategy, test_dag_builder::DagBuilder};

    #[tokio::test]
    async fn test_reputation_scores_authorities_by_score() {
        let context = Arc::new(Context::new_for_test(4).0);
        let scores = ReputationScores::new((1..=300).into(), vec![4, 1, 1, 3]);
        let authorities = scores.authorities_by_score(context);
        assert_eq!(
            authorities,
            vec![
                (AuthorityIndex::new_for_test(0), 4),
                (AuthorityIndex::new_for_test(1), 1),
                (AuthorityIndex::new_for_test(2), 1),
                (AuthorityIndex::new_for_test(3), 3),
            ]
        );
    }

    #[tokio::test]
    async fn test_reputation_scores_update_metrics() {
        let context = Arc::new(Context::new_for_test(4).0);
        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3]);
        scores.update_metrics(context.clone());
        let metrics = context.metrics.node_metrics.reputation_scores.clone();
        assert_eq!(
            metrics
                .get_metric_with_label_values(&["test_host_0"])
                .unwrap()
                .get(),
            1
        );
        assert_eq!(
            metrics
                .get_metric_with_label_values(&["test_host_1"])
                .unwrap()
                .get(),
            2
        );
        assert_eq!(
            metrics
                .get_metric_with_label_values(&["test_host_2"])
                .unwrap()
                .get(),
            4
        );
        assert_eq!(
            metrics
                .get_metric_with_label_values(&["test_host_3"])
                .unwrap()
                .get(),
            3
        );
    }

    #[tokio::test]
    async fn test_reputation_score_calculator() {
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
        let scoring_strategy = VoteScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    #[should_panic(expected = "Attempted to calculate scores with no unscored subdags")]
    async fn test_reputation_score_calculator_no_subdags() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let unscored_subdags = vec![];
        let scoring_strategy = VoteScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        calculator.calculate();
    }

    #[tokio::test]
    #[should_panic(expected = "Attempted to create UnscoredSubdag with no blocks")]
    async fn test_reputation_score_calculator_no_subdag_blocks() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let blocks = vec![];
        let unscored_subdags = vec![CommittedSubDag::new(
            BlockRef::new(1, AuthorityIndex::ZERO, BlockDigest::MIN),
            blocks,
            context.clock.timestamp_utc_ms(),
            CommitRef::new(1, CommitDigest::MIN),
            vec![],
        )];
        let scoring_strategy = VoteScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        calculator.calculate();
    }

    #[tokio::test]
    async fn test_scoring_with_missing_block_in_subdag() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let mut dag_builder = DagBuilder::new(context.clone());
        // Build layer 1 with missing leader block, simulating it was committed
        // as part of another committed subdag.
        dag_builder
            .layer(1)
            .authorities(vec![AuthorityIndex::new_for_test(0)])
            .skip_block()
            .build();
        // Build fully connected layers 2 ~ 3.
        dag_builder.layers(2..=3).build();
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
            tracing::info!("{subdag:?}");
            for block in subdag.blocks.iter() {
                last_committed_rounds[block.author().value()] =
                    max(block.round(), last_committed_rounds[block.author().value()]);
            }
            unscored_subdags.push(subdag);
        }

        let scoring_strategy = VoteScoringStrategy {};
        let mut calculator =
            ReputationScoreCalculator::new(context.clone(), &unscored_subdags, &scoring_strategy);
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }
}

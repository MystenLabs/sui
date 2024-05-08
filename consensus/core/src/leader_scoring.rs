// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    ops::Bound::{Excluded, Included},
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use serde::{Deserialize, Serialize};

use crate::{
    block::{BlockAPI, BlockDigest, BlockRef, Slot, VerifiedBlock},
    commit::CommitRange,
    context::Context,
    leader_scoring_strategy::ScoringStrategy,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    universal_committer::UniversalCommitter,
    CommittedSubDag, Round,
};

#[allow(unused)]
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
    scoring_strategy: Box<dyn ScoringStrategy>,
    // We use the `UniversalCommitter` to elect the leaders from the `UnscoredSubdag`
    // that need to be scored.
    committer: &'a UniversalCommitter,
}

#[allow(unused)]
impl<'a> ReputationScoreCalculator<'a> {
    pub(crate) fn new(
        context: Arc<Context>,
        committer: &'a UniversalCommitter,
        unscored_subdags: &Vec<CommittedSubDag>,
        scoring_strategy: Box<dyn ScoringStrategy>,
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
            committer,
            commit_range,
            scoring_strategy,
            scores_per_authority,
        }
    }

    pub(crate) fn calculate(&mut self) -> ReputationScores {
        let (min_leader_round, max_leader_round) = self.unscored_subdag.get_leader_round_range();
        let scoring_round_range = self
            .scoring_strategy
            .leader_scoring_round_range(min_leader_round, max_leader_round);

        for leader_round in scoring_round_range {
            for committer in self.committer.get_committers() {
                tracing::info!(
                    "Electing leader for round {leader_round} with committer {committer}"
                );
                if let Some(leader_slot) = committer.elect_leader(leader_round) {
                    tracing::info!("Calculating score for leader {leader_slot}");
                    self.add_scores(self.scoring_strategy.calculate_scores_for_leader(
                        &self.unscored_subdag,
                        leader_slot,
                        committer,
                    ))
                }
            }
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

#[allow(unused)]
impl ReputationScores {
    pub(crate) fn new(commit_range: CommitRange, scores_per_authority: Vec<u64>) -> Self {
        Self {
            scores_per_authority,
            commit_range,
        }
    }

    // Returns the authorities in score descending order.
    pub(crate) fn authorities_by_score_desc(
        &self,
        context: Arc<Context>,
    ) -> Vec<(AuthorityIndex, u64)> {
        let mut authorities: Vec<_> = self
            .scores_per_authority
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
            .collect();

        authorities.sort_by(|a1, a2| {
            match a2.1.cmp(&a1.1) {
                Ordering::Equal => {
                    // we resolve the score equality deterministically by ordering in authority
                    // identifier order descending.
                    a2.0.cmp(&a1.0)
                }
                result => result,
            }
        });

        authorities
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
    pub context: Arc<Context>,
    // When the blocks are collected form the list of provided subdags we ensure
    // that the CommittedSubDag instances are contiguous in commit index order.
    // Therefore we can guarnatee the blocks of UnscoredSubdag are also sorted
    // via the commit index.
    pub blocks: BTreeMap<BlockRef, VerifiedBlock>,
    pub commit_range: CommitRange,
}

impl UnscoredSubdag {
    pub(crate) fn new(context: Arc<Context>, subdags: &[CommittedSubDag]) -> Self {
        let blocks = subdags
            .iter()
            .enumerate()
            .flat_map(|(subdag_index, subdag)| {
                if subdag_index == 0 {
                    subdag.blocks.iter()
                } else {
                    let previous_subdag = &subdags[subdag_index - 1];
                    let expected_next_subdag_index = previous_subdag.commit_index + 1;
                    assert_eq!(
                        subdag.commit_index, expected_next_subdag_index,
                        "Non-contiguous commit index (expected: {}, found: {})",
                        expected_next_subdag_index, subdag.commit_index
                    );
                    subdag.blocks.iter()
                }
            })
            .map(|block| (block.reference(), block.clone()))
            .collect::<BTreeMap<_, _>>();

        // Guaranteed to have a contiguous list of commit indices
        let commit_range = CommitRange::new(
            subdags.first().unwrap().commit_index..subdags.last().unwrap().commit_index + 1,
        );

        assert!(
            !blocks.is_empty(),
            "Attempted to create UnscoredSubdag with no blocks"
        );

        Self {
            context,
            blocks,
            commit_range,
        }
    }

    // Returns round range that has an inclusive start and end
    pub(crate) fn get_leader_round_range(&self) -> (Round, Round) {
        // Skip genesis round as we don't produce leaders for that round.
        let first_round = self
            .blocks
            .keys()
            .find(|block_ref| block_ref.round != 0)
            .map(|block_ref| block_ref.round)
            .expect("There should be a non-zero round in the set of blocks");
        let last_round = self.blocks.keys().last().unwrap().round;
        (first_round, last_round)
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
                tracing::info!(
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
    use parking_lot::RwLock;

    use super::*;
    use crate::{
        block::{BlockTimestampMs, TestBlock},
        dag_state::DagState,
        leader_schedule::{LeaderSchedule, LeaderSwapTable},
        leader_scoring_strategy::VoteScoringStrategy,
        storage::mem_store::MemStore,
        universal_committer::universal_committer_builder::UniversalCommitterBuilder,
    };

    #[test]
    fn test_reputation_scores_authorities_by_score_desc() {
        let context = Arc::new(Context::new_for_test(4).0);
        let scores = ReputationScores::new(CommitRange::new(1..300), vec![4, 1, 1, 3]);
        let authorities = scores.authorities_by_score_desc(context);
        assert_eq!(
            authorities,
            vec![
                (AuthorityIndex::new_for_test(0), 4),
                (AuthorityIndex::new_for_test(3), 3),
                (AuthorityIndex::new_for_test(2), 1),
                (AuthorityIndex::new_for_test(1), 1)
            ]
        );
    }

    #[test]
    fn test_reputation_scores_update_metrics() {
        let context = Arc::new(Context::new_for_test(4).0);
        let scores = ReputationScores::new(CommitRange::new(1..300), vec![1, 2, 4, 3]);
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

    #[test]
    fn test_reputation_score_calculator() {
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
            context.clock.timestamp_utc_ms(),
            commit_index,
        )];
        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(VoteScoringStrategy {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, CommitRange::new(1..2));
    }

    #[test]
    #[should_panic(expected = "Attempted to calculate scores with no unscored subdags")]
    fn test_reputation_score_calculator_no_subdags() {
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

        let unscored_subdags = vec![];
        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(VoteScoringStrategy {}),
        );
        calculator.calculate();
    }

    #[test]
    #[should_panic(expected = "Attempted to create UnscoredSubdag with no blocks")]
    fn test_reputation_score_calculator_no_subdag_blocks() {
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

        let blocks = vec![];
        let unscored_subdags = vec![CommittedSubDag::new(
            BlockRef::new(1, AuthorityIndex::ZERO, BlockDigest::MIN),
            blocks,
            context.clock.timestamp_utc_ms(),
            1,
        )];
        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(VoteScoringStrategy {}),
        );
        calculator.calculate();
    }

    #[test]
    fn test_scoring_with_missing_block_in_subdag() {
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
        for round in 1..=4 {
            let mut new_ancestors = vec![];
            for author in 0..4 {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                new_ancestors.push(block.reference());

                // Simulate referenced block which was part of another committed
                // subdag.
                if round == 1 && author == 0 {
                    tracing::info!("Skipping {block} in committed subdags blocks");
                    continue;
                }

                blocks.push(block.clone());

                if round == 4 && author == 0 {
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
            context.clock.timestamp_utc_ms(),
            commit_index,
        )];
        let mut calculator = ReputationScoreCalculator::new(
            context.clone(),
            &committer,
            &unscored_subdags,
            Box::new(VoteScoringStrategy {}),
        );
        let scores = calculator.calculate();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, CommitRange::new(1..2));
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashSet},
    fmt::Debug,
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use serde::{Deserialize, Serialize};

use crate::{
    block::{BlockAPI, BlockRef},
    commit::{CommitRange, CommittedSubDag},
    context::Context,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
};

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

/// ScoringSubdag represents the scoring votes in a collection of subdags across
/// multiple commits.
/// These subdags are "scoring" for the purposes of leader schedule change. As
/// new subdags are added, the DAG is traversed and votes for leaders are recorded
/// and scored along with stake. On a leader schedule change, finalized reputation
/// scores will be calculated based on the votes & stake collected in this struct.
pub(crate) struct ScoringSubdag {
    pub(crate) context: Arc<Context>,
    pub(crate) commit_range: Option<CommitRange>,
    // Only includes committed leaders for now.
    // TODO: Include skipped leaders as well
    pub(crate) leaders: HashSet<BlockRef>,
    // A map of votes to the stake of strongly linked blocks that include that vote
    // Note: Inlcuding stake aggregator so that we can quickly check if it exceeds
    // quourum threshold and only include those scores for certain scoring strategies.
    pub(crate) votes: BTreeMap<BlockRef, StakeAggregator<QuorumThreshold>>,
}

impl ScoringSubdag {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            commit_range: None,
            leaders: HashSet::new(),
            votes: BTreeMap::new(),
        }
    }

    pub(crate) fn add_subdags(&mut self, committed_subdags: Vec<CommittedSubDag>) {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["ScoringSubdag::add_unscored_committed_subdags"])
            .start_timer();
        for subdag in committed_subdags {
            // If the commit range is not set, then set it to the range of the first
            // committed subdag index.
            if self.commit_range.is_none() {
                self.commit_range = Some(CommitRange::new(
                    subdag.commit_ref.index..=subdag.commit_ref.index,
                ));
            } else {
                let commit_range = self.commit_range.as_mut().unwrap();
                commit_range.extend_to(subdag.commit_ref.index);
            }

            // Add the committed leader to the list of leaders we will be scoring.
            tracing::trace!("Adding new committed leader {} for scoring", subdag.leader);
            self.leaders.insert(subdag.leader);

            // Check each block in subdag. Blocks are in order so we should traverse the
            // oldest blocks first
            for block in subdag.blocks {
                for ancestor in block.ancestors() {
                    // Weak links may point to blocks with lower round numbers
                    // than strong links.
                    if ancestor.round != block.round().saturating_sub(1) {
                        continue;
                    }

                    // If a blocks strong linked ancestor is in leaders, then
                    // it's a vote for leader.
                    if self.leaders.contains(ancestor) {
                        // There should never be duplicate references to blocks
                        // with strong linked ancestors to leader.
                        tracing::trace!(
                            "Found a vote {} for leader {ancestor} from authority {}",
                            block.reference(),
                            block.author()
                        );
                        assert!(self
                            .votes
                            .insert(block.reference(), StakeAggregator::new())
                            .is_none(), "Vote {block} already exists. Duplicate vote found for leader {ancestor}");
                    }

                    if let Some(stake) = self.votes.get_mut(ancestor) {
                        // Vote is strongly linked to a future block, so we
                        // consider this a distributed vote.
                        tracing::trace!(
                            "Found a distributed vote {ancestor} from authority {}",
                            ancestor.author
                        );
                        stake.add(block.author(), &self.context.committee);
                    }
                }
            }
        }
    }

    // Iterate through votes and calculate scores for each authority based on
    // scoring strategy that is used. (Vote or CertifiedVote)
    pub(crate) fn calculate_scores(&self) -> ReputationScores {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["ScoringSubdag::calculate_scores"])
            .start_timer();

        let scores_per_authority = if self
            .context
            .protocol_config
            .consensus_distributed_vote_scoring_strategy()
        {
            self.score_distributed_votes()
        } else {
            self.score_votes()
        };

        // TODO: Normalize scores
        ReputationScores::new(
            self.commit_range
                .clone()
                .expect("CommitRange should be set if calculate_scores is called."),
            scores_per_authority,
        )
    }

    /// This scoring strategy will give one point to any votes for the leader.
    fn score_votes(&self) -> Vec<u64> {
        let num_authorities = self.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        for (vote, _) in self.votes.iter() {
            let authority = vote.author;
            tracing::trace!(
                "[{}] scores +1 reputation for {authority}!",
                self.context.own_index,
            );
            scores_per_authority[authority.value()] += 1;
        }

        scores_per_authority
    }

    /// This scoring strategy aims to give scores based on overall vote distribution.
    /// Instead of only giving one point for each vote that is included in 2f+1
    /// blocks. We give a score equal to the amount of stake of all blocks that
    /// included the vote.
    fn score_distributed_votes(&self) -> Vec<u64> {
        let num_authorities = self.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        for (vote, stake_agg) in self.votes.iter() {
            let authority = vote.author;
            let stake = stake_agg.stake();
            tracing::trace!(
                "[{}] scores +{stake} reputation for {authority}!",
                self.context.own_index,
            );
            scores_per_authority[authority.value()] += stake;
        }
        scores_per_authority
    }

    /// This scoring strategy gives points equal to the amount of stake in blocks
    /// that include the authority's vote, if that amount of total_stake > 2f+1.
    /// We consider this a certified vote.
    // TODO: This will be used for ancestor selection
    #[allow(unused)]
    fn score_certified_votes(&self) -> Vec<u64> {
        let num_authorities = self.context.committee.size();
        let mut scores_per_authority = vec![0_u64; num_authorities];

        for (vote, stake_agg) in self.votes.iter() {
            let authority = vote.author;
            if stake_agg.reached_threshold(&self.context.committee) {
                let stake = stake_agg.stake();
                tracing::trace!(
                    "[{}] scores +{stake} reputation for {authority}!",
                    self.context.own_index,
                );
                scores_per_authority[authority.value()] += stake;
            }
        }
        scores_per_authority
    }

    pub(crate) fn scored_subdags_count(&self) -> usize {
        if let Some(commit_range) = &self.commit_range {
            commit_range.size()
        } else {
            0
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.leaders.is_empty() && self.votes.is_empty() && self.commit_range.is_none()
    }

    pub(crate) fn clear(&mut self) {
        self.leaders.clear();
        self.votes.clear();
        self.commit_range = None;
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::max;

    use super::*;
    use crate::test_dag_builder::DagBuilder;

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
    async fn test_scoring_subdag() {
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

        let mut scoring_subdag = ScoringSubdag::new(context.clone());
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
            scoring_subdag.add_subdags(vec![subdag]);
        }

        let scores = scoring_subdag.calculate_scores();
        assert_eq!(scores.scores_per_authority, vec![5, 5, 5, 5]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    async fn test_vote_scoring_subdag() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_distributed_vote_scoring_strategy_for_testing(false);
        let context = Arc::new(context);

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

        let mut scoring_subdag = ScoringSubdag::new(context.clone());
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
            scoring_subdag.add_subdags(vec![subdag]);
        }

        let scores = scoring_subdag.calculate_scores();
        assert_eq!(scores.scores_per_authority, vec![3, 2, 2, 2]);
        assert_eq!(scores.commit_range, (1..=4).into());
    }

    #[tokio::test]
    async fn test_certified_vote_scoring_subdag() {
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

        let mut scoring_subdag = ScoringSubdag::new(context.clone());
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
            scoring_subdag.add_subdags(vec![subdag]);
        }

        let scores_per_authority = scoring_subdag.score_certified_votes();
        assert_eq!(scores_per_authority, vec![4, 4, 4, 4]);
        assert_eq!(scoring_subdag.commit_range.unwrap(), (1..=4).into());
    }
}

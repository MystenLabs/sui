// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use consensus_config::{AuthorityIndex, Stake};
use parking_lot::RwLock;
use rand::{prelude::SliceRandom, rngs::StdRng, SeedableRng};

use crate::{
    commit::CommitRange,
    context::Context,
    dag_state::DagState,
    leader_scoring::{ReputationScoreCalculator, ReputationScores},
    leader_scoring_strategy::{
        CertificateScoringStrategy, CertifiedVoteScoringStrategyV1, CertifiedVoteScoringStrategyV2,
        ScoringStrategy, VoteScoringStrategy,
    },
    universal_committer::UniversalCommitter,
    Round,
};

/// The `LeaderSchedule` is responsible for producing the leader schedule across
/// an epoch. The leader schedule is subject to change periodically based on
/// calculated `ReputationScores` of the authorities.
#[derive(Clone)]
pub(crate) struct LeaderSchedule {
    pub leader_swap_table: Arc<RwLock<LeaderSwapTable>>,
    context: Arc<Context>,
    num_commits_per_schedule: u64,
}

impl LeaderSchedule {
    /// The window where the schedule change takes place in consensus. It represents
    /// number of committed sub dags.
    /// TODO: move this to protocol config
    const CONSENSUS_COMMITS_PER_SCHEDULE: u64 = 300;

    pub(crate) fn new(context: Arc<Context>, leader_swap_table: LeaderSwapTable) -> Self {
        Self {
            context,
            num_commits_per_schedule: Self::CONSENSUS_COMMITS_PER_SCHEDULE,
            leader_swap_table: Arc::new(RwLock::new(leader_swap_table)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_num_commits_per_schedule(mut self, num_commits_per_schedule: u64) -> Self {
        self.num_commits_per_schedule = num_commits_per_schedule;
        self
    }

    /// Restores the `LeaderSchedule` from storage. It will attempt to retrieve the
    /// last stored `ReputationScores` and use them to build a `LeaderSwapTable`.
    pub(crate) fn from_store(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let leader_swap_table = dag_state.read().last_reputation_scores_from_store().map_or(
            LeaderSwapTable::default(),
            |(commit_range, scores_per_authority)| {
                LeaderSwapTable::new(
                    context.clone(),
                    ReputationScores::new(commit_range, scores_per_authority),
                    context
                        .protocol_config
                        .consensus_bad_nodes_stake_threshold(),
                )
            },
        );
        // create the schedule
        Self::new(context, leader_swap_table)
    }

    pub(crate) fn commits_until_leader_schedule_update(
        &self,
        dag_state: Arc<RwLock<DagState>>,
    ) -> usize {
        let unscored_committed_subdags_count = dag_state.read().unscored_committed_subdags_count();
        assert!(
            unscored_committed_subdags_count <= self.num_commits_per_schedule,
            "Unscored committed subdags count exceeds the number of commits per schedule"
        );
        self.num_commits_per_schedule
            .saturating_sub(unscored_committed_subdags_count) as usize
    }

    pub(crate) fn update_leader_schedule(
        &self,
        dag_state: Arc<RwLock<DagState>>,
        committer: &UniversalCommitter,
    ) {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["LeaderSchedule::update_leader_schedule"])
            .start_timer();

        let mut dag_state = dag_state.write();
        let unscored_subdags = dag_state.take_unscored_committed_subdags();

        // TODO: remove this once scoring strategy is finalized
        let scoring_strategy =
            if let Ok(scoring_strategy) = std::env::var("CONSENSUS_SCORING_STRATEGY") {
                tracing::info!(
                    "Using scoring strategy {scoring_strategy} for ReputationScoreCalculator"
                );

                let scoring_strategy: Box<dyn ScoringStrategy> = match scoring_strategy.as_str() {
                    "vote" => Box::new(VoteScoringStrategy {}),
                    "certified_vote_v1" => Box::new(CertifiedVoteScoringStrategyV1 {}),
                    "certified_vote_v2" => Box::new(CertifiedVoteScoringStrategyV2 {}),
                    "certificate" => Box::new(CertificateScoringStrategy {}),
                    _ => Box::new(VoteScoringStrategy {}),
                };
                scoring_strategy
            } else {
                tracing::info!(
                    "Using scoring strategy VoteScoringStrategy for ReputationScoreCalculator"
                );
                Box::new(VoteScoringStrategy {})
            };

        let score_calculation_timer = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["ReputationScoreCalculator::calculate"])
            .start_timer();
        let reputation_scores = ReputationScoreCalculator::new(
            self.context.clone(),
            committer,
            &unscored_subdags,
            scoring_strategy,
        )
        .calculate();
        drop(score_calculation_timer);

        reputation_scores.update_metrics(self.context.clone());

        self.update_leader_swap_table(LeaderSwapTable::new(
            self.context.clone(),
            reputation_scores.clone(),
            self.context
                .protocol_config
                .consensus_bad_nodes_stake_threshold(),
        ));

        self.context
            .metrics
            .node_metrics
            .num_of_bad_nodes
            .set(self.leader_swap_table.read().bad_nodes.len() as i64);

        // Buffer score and last commit rounds in dag state to be persisted later
        dag_state.add_commit_info(
            reputation_scores.commit_range,
            reputation_scores.scores_per_authority,
        );
    }

    pub(crate) fn elect_leader(&self, round: u32, leader_offset: u32) -> AuthorityIndex {
        cfg_if::cfg_if! {
            // TODO: we need to differentiate the leader strategy in tests, so for
            // some type of testing (ex sim tests) we can use the staked approach.
            if #[cfg(test)] {
                let leader = AuthorityIndex::new_for_test((round + leader_offset) % self.context.committee.size() as u32);
                let table = self.leader_swap_table.read();
                table.swap(leader, round, leader_offset).unwrap_or(leader)
            } else {
                let leader = self.elect_leader_stake_based(round, leader_offset);
                let table = self.leader_swap_table.read();
                table.swap(leader, round, leader_offset).unwrap_or(leader)
            }
        }
    }

    pub(crate) fn elect_leader_stake_based(&self, round: u32, offset: u32) -> AuthorityIndex {
        assert!((offset as usize) < self.context.committee.size());

        // To ensure that we elect different leaders for the same round (using
        // different offset) we are using the round number as seed to shuffle in
        // a weighted way the results, but skip based on the offset.
        // TODO: use a cache in case this proves to be computationally expensive
        let mut seed_bytes = [0u8; 32];
        seed_bytes[32 - 4..].copy_from_slice(&(round).to_le_bytes());
        let mut rng = StdRng::from_seed(seed_bytes);

        let choices = self
            .context
            .committee
            .authorities()
            .map(|(index, authority)| (index, authority.stake as f32))
            .collect::<Vec<_>>();

        let leader_index = *choices
            .choose_multiple_weighted(&mut rng, self.context.committee.size(), |item| item.1)
            .expect("Weighted choice error: stake values incorrect!")
            .skip(offset as usize)
            .map(|(index, _)| index)
            .next()
            .unwrap();

        leader_index
    }

    /// Atomically updates the `LeaderSwapTable` with the new provided one. Any
    /// leader queried from now on will get calculated according to this swap
    /// table until a new one is provided again.
    fn update_leader_swap_table(&self, table: LeaderSwapTable) {
        let read = self.leader_swap_table.read();
        let old_commit_range = &read.reputation_scores.commit_range;
        let new_commit_range = &table.reputation_scores.commit_range;

        // Unless LeaderSchedule is brand new and using the default commit range
        // of CommitRange(0..0) all future LeaderSwapTables should be calculated
        // from a CommitRange of equal length and immediately following the
        // preceding commit range of the old swap table.
        if *old_commit_range != CommitRange::new(0..0) {
            assert!(
                old_commit_range.is_next_range(new_commit_range),
                "The new LeaderSwapTable has an invalid CommitRange. Old LeaderSwapTable {old_commit_range:?} vs new LeaderSwapTable {new_commit_range:?}",
            );
        }
        drop(read);

        tracing::trace!("Updating {table:?}");

        let mut write = self.leader_swap_table.write();
        *write = table;
    }
}

#[derive(Default, Clone)]
pub(crate) struct LeaderSwapTable {
    /// The list of `f` (by configurable stake) authorities with best scores as
    /// those defined by the provided `ReputationScores`. Those authorities will
    /// be used in the position of the `bad_nodes` on the final leader schedule.
    /// Storing the hostname & stake along side the authority index for debugging.
    pub(crate) good_nodes: Vec<(AuthorityIndex, String, Stake)>,

    /// The set of `f` (by configurable stake) authorities with the worst scores
    /// as those defined by the provided `ReputationScores`. Every time where such
    /// authority is elected as leader on the schedule, it will swapped by one of
    /// the authorities of the `good_nodes`.
    /// Storing the hostname & stake along side the authority index for debugging.
    pub(crate) bad_nodes: BTreeMap<AuthorityIndex, (String, Stake)>,

    // The scores for which the leader swap table was built from. This struct is
    // used for debugging purposes. Once `good_nodes` & `bad_nodes` are identified
    // the `reputation_scores` are no longer needed functionally for the swap table.
    pub(crate) reputation_scores: ReputationScores,
}

impl LeaderSwapTable {
    // Constructs a new table based on the provided reputation scores. The
    // `swap_stake_threshold` designates the total (by stake) nodes that will be
    // considered as "bad" based on their scores and will be replaced by good nodes.
    // The `swap_stake_threshold` should be in the range of [0 - 33].
    pub(crate) fn new(
        context: Arc<Context>,
        reputation_scores: ReputationScores,
        swap_stake_threshold: u64,
    ) -> Self {
        assert!(
            (0..=33).contains(&swap_stake_threshold),
            "The swap_stake_threshold ({swap_stake_threshold}) should be in range [0 - 33], out of bounds parameter detected"
        );

        // Calculating the good nodes
        let good_nodes = Self::retrieve_first_nodes(
            context.clone(),
            reputation_scores
                .authorities_by_score_desc(context.clone())
                .into_iter(),
            swap_stake_threshold,
        )
        .into_iter()
        .collect::<Vec<(AuthorityIndex, String, Stake)>>();

        // Calculating the bad nodes
        // Reverse the sorted authorities to score ascending so we get the first
        // low scorers up to the provided stake threshold.
        let bad_nodes = Self::retrieve_first_nodes(
            context.clone(),
            reputation_scores
                .authorities_by_score_desc(context.clone())
                .into_iter()
                .rev(),
            swap_stake_threshold,
        )
        .into_iter()
        .map(|(idx, hostname, stake)| (idx, (hostname, stake)))
        .collect::<BTreeMap<AuthorityIndex, (String, Stake)>>();

        good_nodes.iter().for_each(|(idx, hostname, stake)| {
            tracing::debug!(
                "Good node {hostname} with stake {stake} has score {} for {:?}",
                reputation_scores.scores_per_authority[idx.to_owned()],
                reputation_scores.commit_range,
            );
        });

        bad_nodes.iter().for_each(|(idx, (hostname, stake))| {
            tracing::debug!(
                "Bad node {hostname} with stake {stake} has score {} for {:?}",
                reputation_scores.scores_per_authority[idx.to_owned()],
                reputation_scores.commit_range,
            );
        });

        tracing::debug!("Scores used for new LeaderSwapTable: {reputation_scores:?}");

        Self {
            good_nodes,
            bad_nodes,
            reputation_scores,
        }
    }

    /// Checks whether the provided leader is a bad performer and needs to be
    /// swapped in the schedule with a good performer. If not, then the method
    /// returns None. Otherwise the leader to swap with is returned instead. The
    /// `leader_round` & `leader_offset` represents the DAG slot on which the
    /// provided `AuthorityIndex` is a leader on and is used as a seed to random
    /// function in order to calculate the good node that will swap in that round
    /// with the bad node. We are intentionally not doing weighted randomness as
    /// we want to give to all the good nodes equal opportunity to get swapped
    /// with bad nodes and nothave one node with enough stake end up swapping
    /// bad nodes more frequently than the others on the final schedule.
    pub(crate) fn swap(
        &self,
        leader: AuthorityIndex,
        leader_round: Round,
        leader_offset: u32,
    ) -> Option<AuthorityIndex> {
        if self.bad_nodes.contains_key(&leader) {
            // TODO: Re-work swap for the multileader case
            assert!(
                leader_offset == 0,
                "Swap for multi-leader case not implemented yet."
            );
            let mut seed_bytes = [0u8; 32];
            seed_bytes[24..28].copy_from_slice(&leader_round.to_le_bytes());
            seed_bytes[28..32].copy_from_slice(&leader_offset.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);

            let (idx, _hostname, _stake) = self
                .good_nodes
                .choose(&mut rng)
                .expect("There should be at least one good node available");

            tracing::trace!(
                "Swapping bad leader {} -> {} for round {}",
                leader,
                idx,
                leader_round
            );

            return Some(*idx);
        }
        None
    }

    /// Retrieves the first nodes provided by the iterator `authorities` until the
    /// `stake_threshold` has been reached. The `stake_threshold` should be between
    /// [0, 100] and expresses the percentage of stake that is considered the cutoff.
    /// It's the caller's responsibility to ensure that the elements of the `authorities`
    /// input is already sorted.
    fn retrieve_first_nodes(
        context: Arc<Context>,
        authorities: impl Iterator<Item = (AuthorityIndex, u64)>,
        stake_threshold: u64,
    ) -> Vec<(AuthorityIndex, String, Stake)> {
        let mut filtered_authorities = Vec::new();

        let mut stake = 0;
        for (authority_idx, _score) in authorities {
            stake += context.committee.stake(authority_idx);

            // If the total accumulated stake has surpassed the stake threshold
            // then we omit this last authority and we exit the loop. Important to
            // note that this means if the threshold is too low we may not have
            // any nodes returned.
            if stake > (stake_threshold * context.committee.total_stake()) / 100 as Stake {
                break;
            }

            let authority = context.committee.authority(authority_idx);
            filtered_authorities.push((authority_idx, authority.hostname.clone(), authority.stake));
        }

        filtered_authorities
    }
}

impl Debug for LeaderSwapTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "LeaderSwapTable for {:?}, good_nodes:{:?} with stake:{}, bad_nodes:{:?} with stake:{}",
            self.reputation_scores.commit_range,
            self.good_nodes
                .iter()
                .map(|(idx, _hostname, _stake)| idx.to_owned())
                .collect::<Vec<AuthorityIndex>>(),
            self.good_nodes
                .iter()
                .map(|(_idx, _hostname, stake)| stake)
                .sum::<Stake>(),
            self.bad_nodes.keys().map(|idx| idx.to_owned()),
            self.bad_nodes
                .values()
                .map(|(_hostname, stake)| stake)
                .sum::<Stake>(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::{
            timestamp_utc_ms, BlockDigest, BlockRef, BlockTimestampMs, TestBlock, VerifiedBlock,
        },
        commit::{CommitDigest, CommitInfo, CommitRange, CommittedSubDag, TrustedCommit},
        storage::{mem_store::MemStore, Store, WriteBatch},
        universal_committer::universal_committer_builder::UniversalCommitterBuilder,
    };

    #[test]
    fn test_elect_leader() {
        let context = Arc::new(Context::new_for_test(4).0);
        let leader_schedule = LeaderSchedule::new(context, LeaderSwapTable::default());

        assert_eq!(
            leader_schedule.elect_leader(0, 0),
            AuthorityIndex::new_for_test(0)
        );
        assert_eq!(
            leader_schedule.elect_leader(1, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader(5, 0),
            AuthorityIndex::new_for_test(1)
        );
        // ensure we elect different leaders for the same round for the multi-leader case
        assert_ne!(
            leader_schedule.elect_leader_stake_based(1, 1),
            leader_schedule.elect_leader_stake_based(1, 2)
        );
    }

    #[test]
    fn test_elect_leader_stake_based() {
        let context = Arc::new(Context::new_for_test(4).0);
        let leader_schedule = LeaderSchedule::new(context, LeaderSwapTable::default());

        assert_eq!(
            leader_schedule.elect_leader_stake_based(0, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader_stake_based(1, 0),
            AuthorityIndex::new_for_test(1)
        );
        assert_eq!(
            leader_schedule.elect_leader_stake_based(5, 0),
            AuthorityIndex::new_for_test(3)
        );
        // ensure we elect different leaders for the same round for the multi-leader case
        assert_ne!(
            leader_schedule.elect_leader_stake_based(1, 1),
            leader_schedule.elect_leader_stake_based(1, 2)
        );
    }

    #[test]
    fn test_leader_schedule_from_store() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold(33);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());

        // The CommitInfo for the first 10 commits are written to store. This is the
        // info that LeaderSchedule will be recovered from
        let commit_range = CommitRange::new(1..10);
        let reputation_scores = vec![4, 1, 1, 3];
        let last_committed_rounds = vec![9, 9, 10, 9];

        let commit_info = CommitInfo {
            reputation_scores,
            last_committed_rounds,
        };

        store
            .write(
                WriteBatch::default()
                    .commit_ranges_with_commit_info(vec![(commit_range, commit_info)]),
            )
            .unwrap();

        // CommitIndex '11' will be written to store. This should result in the cached
        // last_committed_rounds & unscored subdags in DagState to be updated with the
        // latest commit information on recovery.
        let leader_timestamp = timestamp_utc_ms();
        let blocks = vec![
            VerifiedBlock::new_for_test(
                TestBlock::new(11, 3)
                    .set_timestamp_ms(leader_timestamp)
                    .build(),
            ),
            VerifiedBlock::new_for_test(TestBlock::new(10, 0).build()),
            VerifiedBlock::new_for_test(TestBlock::new(10, 1).build()),
            VerifiedBlock::new_for_test(TestBlock::new(10, 3).build()),
        ];

        let leader = blocks[0].clone();
        let leader_ref = leader.reference();
        let last_commit_index = 11;
        let mut expected_subdag = CommittedSubDag::new(
            leader_ref,
            blocks.clone(),
            leader_timestamp,
            last_commit_index,
        );
        expected_subdag.sort();
        let expected_unscored_subdags = vec![expected_subdag.clone()];
        let expected_last_committed_rounds = vec![10, 10, 10, 11];
        let last_commit = TrustedCommit::new_for_test(
            last_commit_index,
            CommitDigest::MIN,
            leader_ref,
            blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
        );
        store
            .write(
                WriteBatch::default()
                    .blocks(blocks)
                    .commits(vec![last_commit]),
            )
            .unwrap();

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        // Check that DagState recovery from stored CommitInfo worked correctly
        assert_eq!(
            expected_last_committed_rounds,
            dag_state.read().last_committed_rounds()
        );
        let actual_unscored_subdags = dag_state.read().unscored_committed_subdags();
        assert_eq!(
            expected_unscored_subdags.len() as u64,
            dag_state.read().unscored_committed_subdags_count()
        );
        let mut actual_subdag = actual_unscored_subdags[0].clone();
        actual_subdag.sort();
        assert_eq!(expected_subdag, actual_subdag);

        let leader_schedule = LeaderSchedule::from_store(context.clone(), dag_state.clone());

        // Check that LeaderSchedule recovery from stored CommitInfo worked correctly
        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 1);
        assert_eq!(
            leader_swap_table.good_nodes[0].0,
            AuthorityIndex::new_for_test(0)
        );
        assert_eq!(leader_swap_table.bad_nodes.len(), 1);
        assert!(leader_swap_table
            .bad_nodes
            .contains_key(&AuthorityIndex::new_for_test(1)));
    }

    #[test]
    fn test_leader_schedule_commits_until_leader_schedule_update() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        let dag_state = Arc::new(RwLock::new(DagState::new(
            context,
            Arc::new(MemStore::new()),
        )));
        let unscored_subdags = vec![CommittedSubDag::new(
            BlockRef::new(1, AuthorityIndex::ZERO, BlockDigest::MIN),
            vec![],
            timestamp_utc_ms(),
            1,
        )];
        dag_state
            .write()
            .add_unscored_committed_subdags(unscored_subdags);

        let commits_until_leader_schedule_update =
            leader_schedule.commits_until_leader_schedule_update(dag_state.clone());
        assert_eq!(commits_until_leader_schedule_update, 299);
    }

    #[test]
    fn test_leader_schedule_update_leader_schedule() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold(33);
        let context = Arc::new(context);
        let leader_schedule = Arc::new(LeaderSchedule::new(
            context.clone(),
            LeaderSwapTable::default(),
        ));
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));

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

                // Simulate referenced block which was part of another committed
                // subdag.
                if round == 3 && author == 0 {
                    tracing::info!("Skipping {block} in committed subdags blocks");
                    continue;
                }

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

        dag_state
            .write()
            .add_unscored_committed_subdags(unscored_subdags);

        let committer = UniversalCommitterBuilder::new(
            context.clone(),
            leader_schedule.clone(),
            dag_state.clone(),
        )
        .with_pipeline(true)
        .build();

        assert_eq!(
            leader_schedule.elect_leader(4, 0),
            AuthorityIndex::new_for_test(0)
        );

        leader_schedule.update_leader_schedule(dag_state.clone(), &committer);

        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 1);
        assert_eq!(
            leader_swap_table.good_nodes[0].0,
            AuthorityIndex::new_for_test(3)
        );
        assert_eq!(leader_swap_table.bad_nodes.len(), 1);
        assert!(leader_swap_table
            .bad_nodes
            .contains_key(&AuthorityIndex::new_for_test(0)));
        assert_eq!(
            leader_schedule.elect_leader(4, 0),
            AuthorityIndex::new_for_test(3)
        );
    }

    #[test]
    fn test_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            CommitRange::new(0..10),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context, reputation_scores, swap_stake_threshold);

        assert_eq!(leader_swap_table.good_nodes.len(), 1);
        assert_eq!(
            leader_swap_table.good_nodes[0].0,
            AuthorityIndex::new_for_test(3)
        );
        assert_eq!(leader_swap_table.bad_nodes.len(), 1);
        assert!(leader_swap_table
            .bad_nodes
            .contains_key(&AuthorityIndex::new_for_test(0)));
    }

    #[test]
    fn test_leader_swap_table_swap() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            CommitRange::new(0..10),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        // Test swapping a bad leader
        let leader = AuthorityIndex::new_for_test(0);
        let leader_round = 1;
        let leader_offset = 0;
        let swapped_leader = leader_swap_table.swap(leader, leader_round, leader_offset);
        assert_eq!(swapped_leader, Some(AuthorityIndex::new_for_test(3)));

        // Test not swapping a good leader
        let leader = AuthorityIndex::new_for_test(1);
        let leader_round = 1;
        let leader_offset = 0;
        let swapped_leader = leader_swap_table.swap(leader, leader_round, leader_offset);
        assert_eq!(swapped_leader, None);
    }

    #[test]
    fn test_leader_swap_table_retrieve_first_nodes() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let authorities = vec![
            (AuthorityIndex::new_for_test(0), 1),
            (AuthorityIndex::new_for_test(1), 2),
            (AuthorityIndex::new_for_test(2), 3),
            (AuthorityIndex::new_for_test(3), 4),
        ];

        let stake_threshold = 50;
        let filtered_authorities = LeaderSwapTable::retrieve_first_nodes(
            context.clone(),
            authorities.into_iter(),
            stake_threshold,
        );

        // Test setup includes 4 validators with even stake. Therefore with a
        // stake_threshold of 50% we should see 2 validators filtered.
        assert_eq!(filtered_authorities.len(), 2);
        let authority_0_idx = AuthorityIndex::new_for_test(0);
        let authority_0 = context.committee.authority(authority_0_idx);
        assert!(filtered_authorities.contains(&(
            authority_0_idx,
            authority_0.hostname.clone(),
            authority_0.stake
        )));
        let authority_1_idx = AuthorityIndex::new_for_test(1);
        let authority_1 = context.committee.authority(authority_1_idx);
        assert!(filtered_authorities.contains(&(
            authority_1_idx,
            authority_1.hostname.clone(),
            authority_1.stake
        )));
    }

    #[test]
    #[should_panic(
        expected = "The swap_stake_threshold (34) should be in range [0 - 33], out of bounds parameter detected"
    )]
    fn test_leader_swap_table_swap_stake_threshold_out_of_bounds() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 34;
        let reputation_scores = ReputationScores::new(
            CommitRange::new(0..10),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        LeaderSwapTable::new(context, reputation_scores, swap_stake_threshold);
    }

    #[test]
    fn test_update_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            CommitRange::new(1..10),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        // Update leader from brand new schedule to first real schedule
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            CommitRange::new(11..20),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        // Update leader from old swap table to new valid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());
    }

    #[test]
    #[should_panic(
        expected = "The new LeaderSwapTable has an invalid CommitRange. Old LeaderSwapTable CommitRange(11..20) vs new LeaderSwapTable CommitRange(21..25)"
    )]
    fn test_update_bad_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            CommitRange::new(1..10),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        // Update leader from brand new schedule to first real schedule
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            CommitRange::new(11..20),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        // Update leader from old swap table to new valid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            CommitRange::new(21..25),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new(context.clone(), reputation_scores, swap_stake_threshold);

        // Update leader from old swap table to new invalid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());
    }
}

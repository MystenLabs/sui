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
    commit::CommitRange, context::Context, dag_state::DagState, leader_scoring::ReputationScores,
    CommitIndex, Round,
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
    #[cfg(not(msim))]
    const CONSENSUS_COMMITS_PER_SCHEDULE: u64 = 300;
    #[cfg(msim)]
    const CONSENSUS_COMMITS_PER_SCHEDULE: u64 = 10;

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
        let leader_swap_table = dag_state.read().recover_last_commit_info().map_or(
            LeaderSwapTable::default(),
            |(last_commit_ref, last_commit_info)| {
                LeaderSwapTable::new(
                    context.clone(),
                    last_commit_ref.index,
                    last_commit_info.reputation_scores,
                )
            },
        );

        tracing::info!(
            "LeaderSchedule recovered using {leader_swap_table:?}. There are {} committed subdags scored in DagState.",
            dag_state.read().scoring_subdags_count(),
        );

        // create the schedule
        Self::new(context, leader_swap_table)
    }

    pub(crate) fn commits_until_leader_schedule_update(
        &self,
        dag_state: Arc<RwLock<DagState>>,
    ) -> usize {
        let subdag_count = dag_state.read().scoring_subdags_count() as u64;

        assert!(
            subdag_count <= self.num_commits_per_schedule,
            "Committed subdags count exceeds the number of commits per schedule"
        );
        self.num_commits_per_schedule
            .checked_sub(subdag_count)
            .unwrap() as usize
    }

    /// Checks whether the dag state sub dags list is empty. If yes then that means that
    /// either (1) the system has just started and there is no unscored sub dag available (2) the
    /// schedule has updated - new scores have been calculated. Both cases we consider as valid cases
    /// where the schedule has been updated.
    pub(crate) fn leader_schedule_updated(&self, dag_state: &RwLock<DagState>) -> bool {
        dag_state.read().is_scoring_subdag_empty()
    }

    pub(crate) fn update_leader_schedule_v2(&self, dag_state: &RwLock<DagState>) {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["LeaderSchedule::update_leader_schedule"])
            .start_timer();

        let (reputation_scores, last_commit_index) = {
            let dag_state = dag_state.read();
            let reputation_scores = dag_state.calculate_scoring_subdag_scores();

            let last_commit_index = dag_state.scoring_subdag_commit_range();

            (reputation_scores, last_commit_index)
        };

        {
            let mut dag_state = dag_state.write();
            // Clear scoring subdag as we have updated the leader schedule
            dag_state.clear_scoring_subdag();
            // Buffer score and last commit rounds in dag state to be persisted later
            dag_state.add_commit_info(reputation_scores.clone());
        }

        self.update_leader_swap_table(LeaderSwapTable::new(
            self.context.clone(),
            last_commit_index,
            reputation_scores.clone(),
        ));

        reputation_scores.update_metrics(self.context.clone());

        self.context
            .metrics
            .node_metrics
            .num_of_bad_nodes
            .set(self.leader_swap_table.read().bad_nodes.len() as i64);
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
        if *old_commit_range != CommitRange::default() {
            assert!(
                old_commit_range.is_next_range(new_commit_range) && old_commit_range.is_equal_size(new_commit_range),
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

    /// Scores by authority in descending order, needed by other parts of the system
    /// for a consistent view on how each validator performs in consensus.
    pub(crate) reputation_scores_desc: Vec<(AuthorityIndex, u64)>,

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
        commit_index: CommitIndex,
        reputation_scores: ReputationScores,
    ) -> Self {
        let swap_stake_threshold = context
            .protocol_config
            .consensus_bad_nodes_stake_threshold();
        Self::new_inner(
            context,
            swap_stake_threshold,
            commit_index,
            reputation_scores,
        )
    }

    fn new_inner(
        context: Arc<Context>,
        // Ignore linter warning in simtests.
        // TODO: maybe override protocol configs in tests for swap_stake_threshold, and call new().
        #[allow(unused_variables)] swap_stake_threshold: u64,
        commit_index: CommitIndex,
        reputation_scores: ReputationScores,
    ) -> Self {
        #[cfg(msim)]
        let swap_stake_threshold = 33;

        assert!(
            (0..=33).contains(&swap_stake_threshold),
            "The swap_stake_threshold ({swap_stake_threshold}) should be in range [0 - 33], out of bounds parameter detected"
        );

        // When reputation scores are disabled or at genesis, use the default value.
        if reputation_scores.scores_per_authority.is_empty() {
            return Self::default();
        }

        // Randomize order of authorities when they have the same score,
        // to avoid bias in the selection of the good and bad nodes.
        let mut seed_bytes = [0u8; 32];
        seed_bytes[28..32].copy_from_slice(&commit_index.to_le_bytes());
        let mut rng = StdRng::from_seed(seed_bytes);
        let mut authorities_by_score = reputation_scores.authorities_by_score(context.clone());
        assert_eq!(authorities_by_score.len(), context.committee.size());
        authorities_by_score.shuffle(&mut rng);
        // Stable sort the authorities by score descending. Order of authorities with the same score is preserved.
        authorities_by_score.sort_by(|a1, a2| a2.1.cmp(&a1.1));

        // Calculating the good nodes
        let good_nodes = Self::retrieve_first_nodes(
            context.clone(),
            authorities_by_score.iter(),
            swap_stake_threshold,
        )
        .into_iter()
        .collect::<Vec<(AuthorityIndex, String, Stake)>>();

        // Calculating the bad nodes
        // Reverse the sorted authorities to score ascending so we get the first
        // low scorers up to the provided stake threshold.
        let bad_nodes = Self::retrieve_first_nodes(
            context.clone(),
            authorities_by_score.iter().rev(),
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

        tracing::info!("Scores used for new LeaderSwapTable: {reputation_scores:?}");

        Self {
            good_nodes,
            bad_nodes,
            reputation_scores_desc: authorities_by_score,
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
    fn retrieve_first_nodes<'a>(
        context: Arc<Context>,
        authorities: impl Iterator<Item = &'a (AuthorityIndex, u64)>,
        stake_threshold: u64,
    ) -> Vec<(AuthorityIndex, String, Stake)> {
        let mut filtered_authorities = Vec::new();

        let mut stake = 0;
        for &(authority_idx, _score) in authorities {
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
            "LeaderSwapTable for {:?}, good_nodes: {:?} with stake: {}, bad_nodes: {:?} with stake: {}",
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
        block::{BlockDigest, BlockRef, BlockTimestampMs, TestBlock, VerifiedBlock},
        commit::{CommitDigest, CommitInfo, CommitRef, CommittedSubDag, TrustedCommit},
        storage::{mem_store::MemStore, Store, WriteBatch},
        test_dag_builder::DagBuilder,
    };

    #[tokio::test]
    async fn test_elect_leader() {
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

    #[tokio::test]
    async fn test_elect_leader_stake_based() {
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

    #[tokio::test]
    async fn test_leader_schedule_from_store() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());

        // Populate fully connected test blocks for round 0 ~ 11, authorities 0 ~ 3.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=11).build();
        let mut subdags = vec![];
        let mut expected_commits = vec![];
        let mut blocks_to_write = vec![];

        for (sub_dag, commit) in dag_builder.get_sub_dag_and_commits(1..=11) {
            for block in sub_dag.blocks.iter() {
                blocks_to_write.push(block.clone());
            }
            expected_commits.push(commit);
            subdags.push(sub_dag);
        }

        // The CommitInfo for the first 10 commits are written to store. This is the
        // info that LeaderSchedule will be recovered from
        let commit_range = (1..=10).into();
        let reputation_scores = ReputationScores::new(commit_range, vec![4, 1, 1, 3]);
        let committed_rounds = vec![9, 9, 10, 9];
        let commit_ref = expected_commits[9].reference();
        let commit_info = CommitInfo {
            reputation_scores,
            committed_rounds,
        };

        // CommitIndex '11' will be written to store. This should result in the cached
        // last_committed_rounds & unscored subdags in DagState to be updated with the
        // latest commit information on recovery.
        store
            .write(
                WriteBatch::default()
                    .commit_info(vec![(commit_ref, commit_info)])
                    .blocks(blocks_to_write)
                    .commits(expected_commits),
            )
            .unwrap();

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        // Check that DagState recovery from stored CommitInfo worked correctly
        assert_eq!(
            dag_builder.last_committed_rounds.clone(),
            dag_state.read().last_committed_rounds()
        );
        assert_eq!(1, dag_state.read().scoring_subdags_count());
        let recovered_scores = dag_state.read().calculate_scoring_subdag_scores();
        let expected_scores = ReputationScores::new((11..=11).into(), vec![0, 0, 0, 0]);
        assert_eq!(recovered_scores, expected_scores);

        let leader_schedule = LeaderSchedule::from_store(context.clone(), dag_state.clone());

        // Check that LeaderSchedule recovery from stored CommitInfo worked correctly
        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 1);
        assert_eq!(
            leader_swap_table.good_nodes[0].0,
            AuthorityIndex::new_for_test(0)
        );
        assert_eq!(leader_swap_table.bad_nodes.len(), 1);
        assert!(
            leader_swap_table
                .bad_nodes
                .contains_key(&AuthorityIndex::new_for_test(2)),
            "{:?}",
            leader_swap_table.bad_nodes
        );
    }

    #[tokio::test]
    async fn test_leader_schedule_from_store_no_commits() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let expected_last_committed_rounds = vec![0, 0, 0, 0];

        // Check that DagState recovery from stored CommitInfo worked correctly
        assert_eq!(
            expected_last_committed_rounds,
            dag_state.read().last_committed_rounds()
        );
        assert_eq!(0, dag_state.read().scoring_subdags_count());

        let leader_schedule = LeaderSchedule::from_store(context.clone(), dag_state.clone());

        // Check that LeaderSchedule recovery from stored CommitInfo worked correctly
        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 0);
        assert_eq!(leader_swap_table.bad_nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_leader_schedule_from_store_no_commit_info() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());

        // Populate fully connected test blocks for round 0 ~ 2, authorities 0 ~ 3.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder.layers(1..=2).build();

        let mut expected_scored_subdags = vec![];
        let mut expected_commits = vec![];
        let mut blocks_to_write = vec![];

        for (sub_dag, commit) in dag_builder.get_sub_dag_and_commits(1..=2) {
            for block in sub_dag.blocks.iter() {
                blocks_to_write.push(block.clone());
            }
            expected_commits.push(commit);
            expected_scored_subdags.push(sub_dag);
        }

        // The CommitInfo for the first 2 commits are written to store. 10 commits
        // would have been required for a leader schedule update so at this point
        // no commit info should have been persisted and no leader schedule should
        // be recovered. However dag state should have properly recovered the
        // unscored subdags & last committed rounds.
        store
            .write(
                WriteBatch::default()
                    .blocks(blocks_to_write)
                    .commits(expected_commits),
            )
            .unwrap();

        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        // Check that DagState recovery from stored CommitInfo worked correctly
        assert_eq!(
            dag_builder.last_committed_rounds.clone(),
            dag_state.read().last_committed_rounds()
        );
        assert_eq!(
            expected_scored_subdags.len(),
            dag_state.read().scoring_subdags_count()
        );
        let recovered_scores = dag_state.read().calculate_scoring_subdag_scores();
        let expected_scores = ReputationScores::new((1..=2).into(), vec![0, 0, 0, 0]);
        assert_eq!(recovered_scores, expected_scores);

        let leader_schedule = LeaderSchedule::from_store(context.clone(), dag_state.clone());

        // Check that LeaderSchedule recovery from stored CommitInfo worked correctly
        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 0);
        assert_eq!(leader_swap_table.bad_nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_leader_schedule_commits_until_leader_schedule_update() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let unscored_subdags = vec![CommittedSubDag::new(
            BlockRef::new(1, AuthorityIndex::ZERO, BlockDigest::MIN),
            vec![],
            vec![],
            context.clock.timestamp_utc_ms(),
            CommitRef::new(1, CommitDigest::MIN),
            vec![],
        )];
        dag_state.write().add_scoring_subdags(unscored_subdags);

        let commits_until_leader_schedule_update =
            leader_schedule.commits_until_leader_schedule_update(dag_state.clone());
        assert_eq!(commits_until_leader_schedule_update, 299);
    }

    #[tokio::test]
    async fn test_leader_schedule_update_leader_schedule() {
        telemetry_subscribers::init_for_testing();
        let mut context = Context::new_for_test(4).0;
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
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
        let rejected_transactions = vec![vec![]; blocks.len()];

        let last_commit = TrustedCommit::new_for_test(
            commit_index,
            CommitDigest::MIN,
            context.clock.timestamp_utc_ms(),
            leader_ref,
            blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>(),
        );

        let unscored_subdags = vec![CommittedSubDag::new(
            leader_ref,
            blocks,
            rejected_transactions,
            context.clock.timestamp_utc_ms(),
            last_commit.reference(),
            vec![],
        )];

        let mut dag_state_write = dag_state.write();
        dag_state_write.set_last_commit(last_commit);
        dag_state_write.add_scoring_subdags(unscored_subdags);
        drop(dag_state_write);

        assert_eq!(
            leader_schedule.elect_leader(4, 0),
            AuthorityIndex::new_for_test(0)
        );

        leader_schedule.update_leader_schedule_v2(&dag_state);

        let leader_swap_table = leader_schedule.leader_swap_table.read();
        assert_eq!(leader_swap_table.good_nodes.len(), 1);
        assert_eq!(
            leader_swap_table.good_nodes[0].0,
            AuthorityIndex::new_for_test(2)
        );
        assert_eq!(leader_swap_table.bad_nodes.len(), 1);
        assert!(leader_swap_table
            .bad_nodes
            .contains_key(&AuthorityIndex::new_for_test(0)));
        assert_eq!(
            leader_schedule.elect_leader(4, 0),
            AuthorityIndex::new_for_test(2)
        );
    }

    #[tokio::test]
    async fn test_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            (0..=10).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context, swap_stake_threshold, 0, reputation_scores);

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

    #[tokio::test]
    async fn test_leader_swap_table_swap() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            (0..=10).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

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

    #[tokio::test]
    async fn test_leader_swap_table_retrieve_first_nodes() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let authorities = [
            (AuthorityIndex::new_for_test(0), 1),
            (AuthorityIndex::new_for_test(1), 2),
            (AuthorityIndex::new_for_test(2), 3),
            (AuthorityIndex::new_for_test(3), 4),
        ];

        let stake_threshold = 50;
        let filtered_authorities = LeaderSwapTable::retrieve_first_nodes(
            context.clone(),
            authorities.iter(),
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

    #[tokio::test]
    #[should_panic(
        expected = "The swap_stake_threshold (34) should be in range [0 - 33], out of bounds parameter detected"
    )]
    async fn test_leader_swap_table_swap_stake_threshold_out_of_bounds() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 34;
        let reputation_scores = ReputationScores::new(
            (0..=10).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        LeaderSwapTable::new_inner(context, swap_stake_threshold, 0, reputation_scores);
    }

    #[tokio::test]
    async fn test_update_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            (1..=10).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        // Update leader from brand new schedule to first real schedule
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            (11..=20).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

        // Update leader from old swap table to new valid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());
    }

    #[tokio::test]
    #[should_panic(
        expected = "The new LeaderSwapTable has an invalid CommitRange. Old LeaderSwapTable CommitRange(11..=20) vs new LeaderSwapTable CommitRange(21..=25)"
    )]
    async fn test_update_bad_leader_swap_table() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let swap_stake_threshold = 33;
        let reputation_scores = ReputationScores::new(
            (1..=10).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

        let leader_schedule = LeaderSchedule::new(context.clone(), LeaderSwapTable::default());

        // Update leader from brand new schedule to first real schedule
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            (11..=20).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

        // Update leader from old swap table to new valid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());

        let reputation_scores = ReputationScores::new(
            (21..=25).into(),
            (0..4).map(|i| i as u64).collect::<Vec<_>>(),
        );
        let leader_swap_table =
            LeaderSwapTable::new_inner(context.clone(), swap_stake_threshold, 0, reputation_scores);

        // Update leader from old swap table to new invalid swap table
        leader_schedule.update_leader_swap_table(leader_swap_table.clone());
    }
}

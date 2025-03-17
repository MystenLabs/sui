// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;
use tracing::info;

use crate::{
    context::Context, dag_state::DagState, leader_scoring::ReputationScores,
    round_tracker::QuorumRound,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum AncestorState {
    Include,
    Exclude(u64),
}

#[derive(Clone)]
struct AncestorInfo {
    state: AncestorState,
    // This will be set to the future clock round for which this ancestor state
    // will be locked.
    lock_until_round: u32,
}

impl AncestorInfo {
    fn new() -> Self {
        Self {
            state: AncestorState::Include,
            lock_until_round: 0,
        }
    }

    fn is_locked(&self, current_clock_round: u32) -> bool {
        self.lock_until_round >= current_clock_round
    }

    fn set_lock(&mut self, lock_until_round: u32) {
        self.lock_until_round = lock_until_round;
    }
}

pub(crate) struct AncestorStateManager {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    state_map: Vec<AncestorInfo>,
    // This is the reputation scores that we use for leader election but we are
    // using it here as a signal for high quality block propagation as well.
    pub(crate) propagation_scores: ReputationScores,
}

impl AncestorStateManager {
    #[cfg(not(test))]
    const STATE_LOCK_CLOCK_ROUND_UPDATES: u32 = 450;
    #[cfg(test)]
    const STATE_LOCK_CLOCK_ROUND_UPDATES: u32 = 5;

    // Exclusion threshold is based on propagation (reputation) scores
    const EXCLUSION_THRESHOLD_PERCENTAGE: u64 = 20;

    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let state_map = vec![AncestorInfo::new(); context.committee.size()];

        Self {
            context,
            dag_state,
            state_map,
            propagation_scores: ReputationScores::default(),
        }
    }

    pub(crate) fn set_propagation_scores(&mut self, scores: ReputationScores) {
        self.propagation_scores = scores;
    }

    pub(crate) fn get_ancestor_states(&self) -> Vec<AncestorState> {
        self.state_map.iter().map(|info| info.state).collect()
    }

    /// Updates the state of all ancestors based on the latest scores and quorum rounds
    pub(crate) fn update_all_ancestors_state(&mut self, accepted_quorum_rounds: &[QuorumRound]) {
        let propagation_scores_by_authority = self
            .propagation_scores
            .scores_per_authority
            .clone()
            .into_iter()
            .enumerate()
            .map(|(idx, score)| {
                (
                    self.context
                        .committee
                        .to_authority_index(idx)
                        .expect("Index should be valid"),
                    score,
                )
            })
            .collect::<Vec<_>>();

        // If round prober has not run yet and we don't have network quorum round,
        // it is okay because network_high_quorum_round will be zero and we will
        // include all ancestors until we get more information.
        let network_high_quorum_round =
            self.calculate_network_high_quorum_round(accepted_quorum_rounds);

        // If propagation scores are not ready because the first 300 commits have not
        // happened, this is okay as we will only start excluding ancestors after that
        // point in time.
        for (authority_id, score) in propagation_scores_by_authority {
            let (_low, authority_high_quorum_round) = accepted_quorum_rounds[authority_id.value()];

            self.update_state(
                authority_id,
                score,
                authority_high_quorum_round,
                network_high_quorum_round,
            );
        }
    }

    /// Updates the state of the given authority based on current scores and quorum rounds.
    fn update_state(
        &mut self,
        authority_id: AuthorityIndex,
        propagation_score: u64,
        authority_high_quorum_round: u32,
        network_high_quorum_round: u32,
    ) {
        let block_hostname = &self.context.committee.authority(authority_id).hostname;
        let mut ancestor_info = self.state_map[authority_id].clone();

        let current_clock_round = self.dag_state.read().threshold_clock_round();
        if ancestor_info.is_locked(current_clock_round) {
            // If still locked, we won't make any state changes.
            return;
        }

        let low_score_threshold =
            (self.propagation_scores.highest_score() * Self::EXCLUSION_THRESHOLD_PERCENTAGE) / 100;

        match ancestor_info.state {
            // Check conditions to switch to EXCLUDE state
            AncestorState::Include => {
                if propagation_score <= low_score_threshold {
                    ancestor_info.state = AncestorState::Exclude(propagation_score);
                    let lock_until_round =
                        current_clock_round + Self::STATE_LOCK_CLOCK_ROUND_UPDATES;
                    ancestor_info.set_lock(lock_until_round);
                    info!(
                        "Authority {authority_id} moved to EXCLUDE state with score {propagation_score} <= threshold of {low_score_threshold} and locked until round {lock_until_round}",
                    );
                    self.context
                        .metrics
                        .node_metrics
                        .ancestor_state_change_by_authority
                        .with_label_values(&[block_hostname, "exclude"])
                        .inc();
                }
            }
            // Check conditions to switch back to INCLUDE state
            AncestorState::Exclude(_) => {
                // It should not be possible for the scores to get over the threshold
                // until the node is back in the INCLUDE state, but adding just in case.
                if propagation_score > low_score_threshold
                    || authority_high_quorum_round >= network_high_quorum_round
                {
                    ancestor_info.state = AncestorState::Include;

                    let lock_until_round =
                        current_clock_round + Self::STATE_LOCK_CLOCK_ROUND_UPDATES;
                    ancestor_info.set_lock(lock_until_round);
                    info!(
                        "Authority {authority_id} moved to INCLUDE state with {propagation_score} > threshold of {low_score_threshold} or {authority_high_quorum_round} >= {network_high_quorum_round} and locked until round {lock_until_round}",
                    );
                    self.context
                        .metrics
                        .node_metrics
                        .ancestor_state_change_by_authority
                        .with_label_values(&[block_hostname, "include"])
                        .inc();
                }
            }
        }

        // If any updates were made to state ensure they are persisted.
        self.state_map[authority_id] = ancestor_info;
    }

    /// Calculate the network's high quorum round based on accepted rounds via
    /// RoundTracker.
    ///
    /// The authority high quorum round is the lowest round higher or equal to rounds  
    /// from a quorum of authorities. The network high quorum round is using the high
    /// quorum round of each authority as tracked by the [`RoundTracker`] and then
    /// finding the high quroum round of those high quorum rounds.
    fn calculate_network_high_quorum_round(&self, accepted_quorum_rounds: &[QuorumRound]) -> u32 {
        let committee = &self.context.committee;

        let mut high_quorum_rounds_with_stake = accepted_quorum_rounds
            .iter()
            .zip(committee.authorities())
            .map(|((_low, high), (_, authority))| (*high, authority.stake))
            .collect::<Vec<_>>();
        high_quorum_rounds_with_stake.sort();

        let mut total_stake = 0;
        let mut network_high_quorum_round = 0;

        for (round, stake) in high_quorum_rounds_with_stake.iter() {
            total_stake += stake;
            if total_stake >= self.context.committee.quorum_threshold() {
                network_high_quorum_round = *round;
                break;
            }
        }

        network_high_quorum_round
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        leader_scoring::ReputationScores, storage::mem_store::MemStore,
        test_dag_builder::DagBuilder,
    };

    #[tokio::test]
    async fn test_calculate_network_high_accepted_quorum_round() {
        telemetry_subscribers::init_for_testing();

        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3]);
        let mut ancestor_state_manager =
            AncestorStateManager::new(context.clone(), dag_state.clone());
        ancestor_state_manager.set_propagation_scores(scores);

        // Quorum rounds are not set yet, so we should calculate a network
        // quorum round of 0 to start.
        let network_high_quorum_round =
            ancestor_state_manager.calculate_network_high_quorum_round(&[]);
        assert_eq!(network_high_quorum_round, 0);

        let accepted_quorum_rounds = vec![(50, 229), (175, 229), (179, 229), (179, 300)];

        let network_high_quorum_round =
            ancestor_state_manager.calculate_network_high_quorum_round(&accepted_quorum_rounds);
        assert_eq!(network_high_quorum_round, 229);
    }

    // Test all state transitions with probe_accepted_rounds = true
    // Default all INCLUDE -> EXCLUDE
    // EXCLUDE -> INCLUDE (Blocked due to lock)
    // EXCLUDE -> INCLUDE (Pass due to lock expired)
    // INCLUDE -> EXCLUDE (Blocked due to lock)
    // INCLUDE -> EXCLUDE (Pass due to lock expired)
    #[tokio::test]
    async fn test_update_all_ancestor_state_using_accepted_rounds() {
        telemetry_subscribers::init_for_testing();
        let (context, _key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let mut dag_builder = DagBuilder::new(context.clone());

        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3]);
        let mut ancestor_state_manager = AncestorStateManager::new(context, dag_state.clone());
        ancestor_state_manager.set_propagation_scores(scores);

        let accepted_quorum_rounds = vec![(225, 229), (225, 229), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Score threshold for exclude is (4 * 10) / 100 = 0
        // No ancestors should be excluded in with this threshold
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }

        let scores = ReputationScores::new((1..=300).into(), vec![10, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Score threshold for exclude is (100 * 10) / 100 = 10
        // 2 authorities should be excluded in with this threshold
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if (0..=1).contains(&authority) {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // 2 authorities should still be excluded with these scores and no new
        // clock round updates have happened to expire the locks.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if (0..=1).contains(&authority) {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        // Updating the clock round will expire the lock as we only need 5
        // clock round updates for tests.
        dag_builder.layers(1..=6).build();
        let blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        dag_state.write().accept_blocks(blocks);

        let accepted_quorum_rounds = vec![(229, 300), (225, 229), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Authority 0 should now be included again because high quorum round is
        // at the network high quorum round of 300. Authority 1's quorum round is
        // too low and will remain excluded.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 1 {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        let accepted_quorum_rounds = vec![(229, 300), (229, 300), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Ancestor 1 can transtion to the INCLUDE state. Ancestor 0 is still locked
        // in the INCLUDE state until a score update is performed which is why
        // even though the scores are still low it has not moved to the EXCLUDE
        // state.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }

        // Updating the clock round will expire the lock as we only need 5 updates for tests.
        dag_builder.layers(7..=12).build();
        let blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        dag_state.write().accept_blocks(blocks);

        let scores = ReputationScores::new((1..=300).into(), vec![100, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Ancestor 1 can transition to EXCLUDE state now that the lock expired
        // and its scores are below the threshold.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 1 {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }
    }
}

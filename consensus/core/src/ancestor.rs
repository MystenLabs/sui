// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Stake};
use parking_lot::RwLock;
use tracing::{debug, info};

use crate::{
    context::Context, dag_state::DagState, leader_scoring::ReputationScores,
    round_tracker::QuorumRound,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum AncestorState {
    Include,
    // Exclusion score is the value stored in this state
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

#[derive(Debug)]
struct StateTransition {
    authority_id: AuthorityIndex,
    // The authority propagation score taken from leader scoring.
    score: u64,
    // The stake of the authority that is transitioning state.
    stake: u64,
    // The authority high quorum round is the lowest round higher or equal to rounds
    // from a quorum of authorities
    high_quorum_round: u32,
}

pub(crate) struct AncestorStateManager {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    state_map: Vec<AncestorInfo>,
    excluded_nodes_stake_threshold: u64,
    // This is the running total of ancestors by stake that have been marked
    // as excluded. This cannot exceed the excluded_nodes_stake_threshold
    total_excluded_stake: Stake,
    // This is the reputation scores that we use for leader election but we are
    // using it here as a signal for high quality block propagation as well.
    pub(crate) propagation_scores: ReputationScores,
}

impl AncestorStateManager {
    // This value is based on the production round rates of between 10-15 rounds per second
    // which means we will be locking state between 30-45 seconds.
    #[cfg(not(test))]
    const STATE_LOCK_CLOCK_ROUNDS: u32 = 450;
    #[cfg(test)]
    const STATE_LOCK_CLOCK_ROUNDS: u32 = 5;

    // Exclusion threshold is based on propagation (reputation) scores
    const SCORE_EXCLUSION_THRESHOLD_PERCENTAGE: u64 = 20;

    pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
        let state_map = vec![AncestorInfo::new(); context.committee.size()];

        // Note: this value cannot be greater than the threshold used in leader
        // schedule to identify bad nodes.
        let excluded_nodes_stake_threshold_percentage = 2 * context
            .protocol_config
            .consensus_bad_nodes_stake_threshold()
            / 3;

        let excluded_nodes_stake_threshold = (excluded_nodes_stake_threshold_percentage
            * context.committee.total_stake())
            / 100 as Stake;

        Self {
            context,
            dag_state,
            state_map,
            excluded_nodes_stake_threshold,
            // All ancestors start in the include state.
            total_excluded_stake: 0,
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
        // If round prober has not run yet and we don't have network quorum round,
        // it is okay because network_high_quorum_round will be zero and we will
        // include all ancestors until we get more information.
        let network_high_quorum_round =
            self.calculate_network_high_quorum_round(accepted_quorum_rounds);

        let current_clock_round = self.dag_state.read().threshold_clock_round();
        let low_score_threshold = (self.propagation_scores.highest_score()
            * Self::SCORE_EXCLUSION_THRESHOLD_PERCENTAGE)
            / 100;

        debug!("Updating all ancestor state at round {current_clock_round} using network high quorum round of {network_high_quorum_round}, low score threshold of {low_score_threshold}, and exclude stake threshold of {}", self.excluded_nodes_stake_threshold);

        // We will first collect all potential state transitions as we need to ensure
        // we do not move more ancestors to EXCLUDE state than the excluded_nodes_stake_threshold
        // allows
        let mut exclude_to_include = Vec::new();
        let mut include_to_exclude = Vec::new();

        // If propagation scores are not ready because the first 300 commits have not
        // happened, this is okay as we will only start excluding ancestors after that
        // point in time.
        for (idx, score) in self
            .propagation_scores
            .scores_per_authority
            .iter()
            .enumerate()
        {
            let authority_id = self
                .context
                .committee
                .to_authority_index(idx)
                .expect("Index should be valid");
            let ancestor_info = &self.state_map[idx];
            let (_low, authority_high_quorum_round) = accepted_quorum_rounds[idx];
            let stake = self.context.committee.authority(authority_id).stake;

            // Skip if locked
            if ancestor_info.is_locked(current_clock_round) {
                continue;
            }

            match ancestor_info.state {
                AncestorState::Include => {
                    if *score <= low_score_threshold {
                        include_to_exclude.push(StateTransition {
                            authority_id,
                            score: *score,
                            stake,
                            high_quorum_round: authority_high_quorum_round,
                        });
                    }
                }
                AncestorState::Exclude(_) => {
                    if *score > low_score_threshold
                        || authority_high_quorum_round >= network_high_quorum_round
                    {
                        exclude_to_include.push(StateTransition {
                            authority_id,
                            score: *score,
                            stake,
                            high_quorum_round: authority_high_quorum_round,
                        });
                    }
                }
            }
        }

        // We can apply the state change for all ancestors that are moving to the
        // include state as that will never cause us to exceed the excluded_nodes_stake_threshold
        for transition in exclude_to_include {
            self.apply_state_change(transition, AncestorState::Include, current_clock_round);
        }

        // Sort include_to_exclude by worst scores first as these should take priority
        // to be excluded if we can't exclude them all due to the excluded_nodes_stake_threshold
        include_to_exclude.sort_by_key(|t| t.score);

        // We can now apply state change for all ancestors that are moving to the exclude
        // state as we know there is no new stake that will be freed up by ancestor
        // state transition to include.
        for transition in include_to_exclude {
            // If the stake of this ancestor would cause us to exceed the threshold
            // we do nothing. The lock will continue to be unlocked meaning we can
            // try again immediately on the next call to update_all_ancestors_state
            if self.total_excluded_stake + transition.stake <= self.excluded_nodes_stake_threshold {
                let new_state = AncestorState::Exclude(transition.score);
                self.apply_state_change(transition, new_state, current_clock_round);
            } else {
                info!(
                    "Authority {} would have moved to {:?} state with score {} & quorum_round {} but we would have exceeded total excluded stake threshold. current_excluded_stake {} + authority_stake {} > exclude_stake_threshold {}",
                    transition.authority_id,
                    AncestorState::Exclude(transition.score),
                    transition.score,
                    transition.high_quorum_round,
                    self.total_excluded_stake,
                    transition.stake,
                    self.excluded_nodes_stake_threshold
                );
            }
        }
    }

    fn apply_state_change(
        &mut self,
        transition: StateTransition,
        new_state: AncestorState,
        current_clock_round: u32,
    ) {
        let block_hostname = &self
            .context
            .committee
            .authority(transition.authority_id)
            .hostname;
        let ancestor_info = &mut self.state_map[transition.authority_id.value()];

        match (ancestor_info.state, new_state) {
            (AncestorState::Exclude(_), AncestorState::Include) => {
                self.total_excluded_stake = self.total_excluded_stake
                    .checked_sub(transition.stake)
                    .expect("total_excluded_stake underflow - trying to subtract more stake than we're tracking as excluded");
            }
            (AncestorState::Include, AncestorState::Exclude(_)) => {
                self.total_excluded_stake += transition.stake;
            }
            _ => {
                panic!("Calls to this function should only be made for state transition.")
            }
        }

        ancestor_info.state = new_state;
        let lock_until_round = current_clock_round + Self::STATE_LOCK_CLOCK_ROUNDS;
        ancestor_info.set_lock(lock_until_round);

        info!(
            "Authority {} moved to {new_state:?} state with score {} & quorum_round {} and locked until round {lock_until_round}. Total excluded stake: {}",
            transition.authority_id,
            transition.score,
            transition.high_quorum_round,
            self.total_excluded_stake
        );

        self.context
            .metrics
            .node_metrics
            .ancestor_state_change_by_authority
            .with_label_values(&[
                block_hostname,
                match new_state {
                    AncestorState::Include => "include",
                    AncestorState::Exclude(_) => "exclude",
                },
            ])
            .inc();
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
        let (mut context, _key_pairs) = Context::new_for_test(5);
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let mut dag_builder = DagBuilder::new(context.clone());

        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3, 4]);
        let mut ancestor_state_manager = AncestorStateManager::new(context, dag_state.clone());
        ancestor_state_manager.set_propagation_scores(scores);

        let accepted_quorum_rounds =
            vec![(225, 229), (225, 229), (229, 300), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Score threshold for exclude is (4 * 10) / 100 = 0
        // No ancestors should be excluded in with this threshold
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }

        let scores = ReputationScores::new((1..=300).into(), vec![10, 9, 100, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Score threshold for exclude is (100 * 10) / 100 = 10
        // Authority 1 with the lowest score will move to the EXCLUDE state
        // Authority 0 with the next lowest score is eligible to move to the EXCLUDE
        // state based on the score threshold but it would exceed the total excluded
        // stake threshold so it remains in the INCLUDE state.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 1 {
                assert_eq!(*state, AncestorState::Exclude(9));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // 1 authorities should still be excluded with these scores and no new
        // clock round updates have happened to expire the locks.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 1 {
                assert_eq!(*state, AncestorState::Exclude(9));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        // Updating the clock round will expire the lock as we only need 5
        // clock round updates for tests.
        dag_builder.layers(1..=6).build();
        let blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        dag_state.write().accept_blocks(blocks);

        let accepted_quorum_rounds =
            vec![(225, 229), (229, 300), (229, 300), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Authority 1 should now be included again because high quorum round is
        // at the network high quorum round of 300. Authority 0 will now be moved
        // to EXCLUDE state as its score is low.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 0 {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        let accepted_quorum_rounds =
            vec![(229, 300), (229, 300), (229, 300), (229, 300), (229, 300)];
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Ancestor 0 is still locked in the EXCLUDE state until there is more
        // clock round updates which is why even though the quorum rounds are
        // high enough it has not moved to the INCLUDE state.
        let state_map = ancestor_state_manager.get_ancestor_states();

        for (authority, state) in state_map.iter().enumerate() {
            if authority == 0 {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        // Updating the clock round will expire the lock as we only need 5 updates for tests.
        dag_builder.layers(7..=12).build();
        let blocks = dag_builder.blocks.values().cloned().collect::<Vec<_>>();
        dag_state.write().accept_blocks(blocks);

        let scores = ReputationScores::new((1..=300).into(), vec![10, 100, 100, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state(&accepted_quorum_rounds);

        // Ancestor 0 can transition to INCLUDE state now that the lock expired
        // and its quorum round is above the threshold.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }
    }
}

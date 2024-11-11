// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use tracing::info;

use crate::{context::Context, leader_scoring::ReputationScores, round_prober::QuorumRound};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum AncestorState {
    Include,
    Exclude(u64),
}

#[derive(Clone)]
struct AncestorInfo {
    state: AncestorState,
    // This will be set to the count of either the quorum round update count or
    // the score update count for which the EXCLUDE or INCLUDE state are locked
    // in respectively.
    lock_expiry_count: u32,
}

impl AncestorInfo {
    fn new() -> Self {
        Self {
            state: AncestorState::Include,
            lock_expiry_count: 0,
        }
    }

    fn is_locked(
        &self,
        propagation_score_update_count: u32,
        quorum_round_update_count: u32,
    ) -> bool {
        match self.state {
            AncestorState::Include => self.lock_expiry_count > propagation_score_update_count,
            AncestorState::Exclude(_) => self.lock_expiry_count > quorum_round_update_count,
        }
    }

    fn set_lock(&mut self, future_count: u32) {
        self.lock_expiry_count = future_count;
    }
}

pub(crate) struct AncestorStateManager {
    context: Arc<Context>,
    state_map: Vec<AncestorInfo>,
    propagation_score_update_count: u32,
    quorum_round_update_count: u32,
    pub(crate) quorum_round_per_authority: Vec<QuorumRound>,
    pub(crate) propagation_scores: ReputationScores,
}

impl AncestorStateManager {
    // Number of quorum round updates for which an ancestor is locked in the EXCLUDE state
    // Chose 10 updates as that should be ~50 seconds of waiting with the current round prober
    // interval of 5s
    #[cfg(not(test))]
    const STATE_LOCK_QUORUM_ROUND_UPDATES: u32 = 10;
    #[cfg(test)]
    const STATE_LOCK_QUORUM_ROUND_UPDATES: u32 = 1;

    // Number of propagation score updates for which an ancestor is locked in the INCLUDE state
    // Chose 2 leader schedule updates (~300 commits per schedule) which should be ~30-90 seconds
    // depending on the round rate for the authority to improve scores.
    #[cfg(not(test))]
    const STATE_LOCK_SCORE_UPDATES: u32 = 2;
    #[cfg(test)]
    const STATE_LOCK_SCORE_UPDATES: u32 = 1;

    // Exclusion threshold is based on reputation scores
    const EXCLUSION_THRESHOLD_PERCENTAGE: u64 = 10;

    // Inclusion threshold is based on network quorum round
    const INCLUSION_THRESHOLD_PERCENTAGE: u64 = 90;

    pub(crate) fn new(context: Arc<Context>, propagation_scores: ReputationScores) -> Self {
        let state_map = vec![AncestorInfo::new(); context.committee.size()];

        let quorum_round_per_authority = vec![(0, 0); context.committee.size()];
        Self {
            context,
            state_map,
            propagation_score_update_count: 0,
            quorum_round_update_count: 0,
            propagation_scores,
            quorum_round_per_authority,
        }
    }

    pub(crate) fn set_quorum_round_per_authority(&mut self, quorum_rounds: Vec<QuorumRound>) {
        self.quorum_round_per_authority = quorum_rounds;
        self.quorum_round_update_count += 1;
    }

    pub(crate) fn set_propagation_scores(&mut self, scores: ReputationScores) {
        self.propagation_scores = scores;
        self.propagation_score_update_count += 1;
    }

    pub(crate) fn get_ancestor_states(&self) -> Vec<AncestorState> {
        self.state_map.iter().map(|info| info.state).collect()
    }

    pub(crate) fn get_inclusion_stake_threshold(&self) -> u64 {
        self.context
            .committee
            .n_percent_stake_threshold(Self::INCLUSION_THRESHOLD_PERCENTAGE)
    }

    /// Updates the state of all ancestors based on the latest scores and quorum rounds
    pub(crate) fn update_all_ancestors_state(&mut self) {
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
        // it is okay because network_low_quorum_round will be zero and we will
        // include all ancestors until we get more information.
        let network_low_quorum_round = self.calculate_network_low_quorum_round();

        // If propagation scores are not ready because the first 300 commits have not
        // happened, this is okay as we will only start excluding ancestors after that
        // point in time.
        for (authority_id, score) in propagation_scores_by_authority {
            let (authority_low_quorum_round, _high) = self.quorum_round_per_authority[authority_id];

            self.update_state(
                authority_id,
                score,
                authority_low_quorum_round,
                network_low_quorum_round,
            );
        }
    }

    /// Updates the state of the given authority based on current scores and quorum rounds.
    fn update_state(
        &mut self,
        authority_id: AuthorityIndex,
        propagation_score: u64,
        authority_low_quorum_round: u32,
        network_low_quorum_round: u32,
    ) {
        let block_hostname = &self.context.committee.authority(authority_id).hostname;
        let mut ancestor_info = self.state_map[authority_id].clone();

        if ancestor_info.is_locked(
            self.propagation_score_update_count,
            self.quorum_round_update_count,
        ) {
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
                    ancestor_info.set_lock(
                        self.quorum_round_update_count + Self::STATE_LOCK_QUORUM_ROUND_UPDATES,
                    );
                    info!(
                        "Authority {authority_id} moved to EXCLUDE state with score {propagation_score} <= threshold of {low_score_threshold} and locked for {:?} quorum round updates",
                        Self::STATE_LOCK_QUORUM_ROUND_UPDATES
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
                    || authority_low_quorum_round >= (network_low_quorum_round)
                {
                    ancestor_info.state = AncestorState::Include;
                    ancestor_info.set_lock(
                        self.propagation_score_update_count + Self::STATE_LOCK_SCORE_UPDATES,
                    );
                    info!(
                        "Authority {authority_id} moved to INCLUDE state with {propagation_score} > threshold of {low_score_threshold} or {authority_low_quorum_round} >= {network_low_quorum_round} and locked for {:?} score updates.",
                        Self::STATE_LOCK_SCORE_UPDATES
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

    /// Calculate the network's quorum round from authorities by inclusion stake
    /// threshold, where quorum round is the highest round a block has been seen
    /// by a percentage (inclusion threshold) of authorities.
    fn calculate_network_low_quorum_round(&self) -> u32 {
        let committee = &self.context.committee;
        let inclusion_stake_threshold = self.get_inclusion_stake_threshold();
        let mut low_quorum_rounds_with_stake = self
            .quorum_round_per_authority
            .iter()
            .zip(committee.authorities())
            .map(|((low, _high), (_, authority))| (*low, authority.stake))
            .collect::<Vec<_>>();
        low_quorum_rounds_with_stake.sort();

        let mut total_stake = 0;
        let mut network_low_quorum_round = 0;

        for (round, stake) in low_quorum_rounds_with_stake.iter().rev() {
            let reached_quorum_before = total_stake >= inclusion_stake_threshold;
            total_stake += stake;
            if !reached_quorum_before && total_stake >= inclusion_stake_threshold {
                network_low_quorum_round = *round;
                break;
            }
        }

        network_low_quorum_round
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::leader_scoring::ReputationScores;

    #[tokio::test]
    async fn test_calculate_network_low_quorum_round() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3]);
        let mut ancestor_state_manager = AncestorStateManager::new(context, scores);

        // Quorum rounds are not set yet, so we should calculate a network quorum
        // round of 0 to start.
        let network_low_quorum_round = ancestor_state_manager.calculate_network_low_quorum_round();
        assert_eq!(network_low_quorum_round, 0);

        let quorum_rounds = vec![(225, 229), (225, 300), (229, 300), (229, 300)];
        ancestor_state_manager.set_quorum_round_per_authority(quorum_rounds);

        let network_low_quorum_round = ancestor_state_manager.calculate_network_low_quorum_round();
        assert_eq!(network_low_quorum_round, 225);
    }

    // Test all state transitions
    // Default all INCLUDE -> EXCLUDE
    // EXCLUDE -> INCLUDE (Blocked due to lock)
    // EXCLUDE -> INCLUDE (Pass due to lock expired)
    // INCLUDE -> EXCLUDE (Blocked due to lock)
    // INCLUDE -> EXCLUDE (Pass due to lock expired)
    #[tokio::test]
    async fn test_update_all_ancestor_state() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);

        let scores = ReputationScores::new((1..=300).into(), vec![1, 2, 4, 3]);
        let mut ancestor_state_manager = AncestorStateManager::new(context, scores);

        let quorum_rounds = vec![(225, 229), (225, 300), (229, 300), (229, 300)];
        ancestor_state_manager.set_quorum_round_per_authority(quorum_rounds);
        ancestor_state_manager.update_all_ancestors_state();

        // Score threshold for exclude is (4 * 10) / 100 = 0
        // No ancestors should be excluded in with this threshold
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }

        let scores = ReputationScores::new((1..=300).into(), vec![10, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state();

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

        ancestor_state_manager.update_all_ancestors_state();

        // 2 authorities should still be excluded with these scores and no new
        // quorum round updates have been set to expire the locks.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if (0..=1).contains(&authority) {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        // Updating the quorum rounds will expire the lock as we only need 1
        // quorum round update for tests.
        let quorum_rounds = vec![(229, 300), (225, 300), (229, 300), (229, 300)];
        ancestor_state_manager.set_quorum_round_per_authority(quorum_rounds);
        ancestor_state_manager.update_all_ancestors_state();

        // Authority 0 should now be included again because low quorum round is
        // at the network low quorum round of 229. Authority 1's quorum round is
        // too low and will remain excluded.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for (authority, state) in state_map.iter().enumerate() {
            if authority == 1 {
                assert_eq!(*state, AncestorState::Exclude(10));
            } else {
                assert_eq!(*state, AncestorState::Include);
            }
        }

        let quorum_rounds = vec![(229, 300), (229, 300), (229, 300), (229, 300)];
        ancestor_state_manager.set_quorum_round_per_authority(quorum_rounds);
        ancestor_state_manager.update_all_ancestors_state();

        // Ancestor 1 can transtion to the INCLUDE state. Ancestor 0 is still locked
        // in the INCLUDE state until a score update is performed which is why
        // even though the scores are still low it has not moved to the EXCLUDE
        // state.
        let state_map = ancestor_state_manager.get_ancestor_states();
        for state in state_map.iter() {
            assert_eq!(*state, AncestorState::Include);
        }

        // Updating the scores will expire the lock as we only need 1 update for tests.
        let scores = ReputationScores::new((1..=300).into(), vec![100, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state();

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

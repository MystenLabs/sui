// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use consensus_config::AuthorityIndex;
use tokio::time::{Duration, Instant};
use tracing::info;

use crate::{context::Context, leader_scoring::ReputationScores, round_prober::QuorumRound};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum AncestorState {
    Include,
    Exclude(u64),
}
struct AncestorInfo {
    state: AncestorState,
    lock_expiry_time: Option<Instant>,
}

impl AncestorInfo {
    fn new() -> Self {
        Self {
            state: AncestorState::Include,
            lock_expiry_time: None,
        }
    }
}

pub(crate) struct AncestorStateManager {
    context: Arc<Context>,
    state_map: HashMap<AuthorityIndex, AncestorInfo>,
    pub(crate) quorum_round_per_authority: Vec<QuorumRound>,
    pub(crate) propagation_scores: ReputationScores,
}

impl AncestorStateManager {
    #[cfg(not(test))]
    const STATE_LOCK_DURATION: Duration = Duration::from_secs(30);
    #[cfg(test)]
    const STATE_LOCK_DURATION: Duration = Duration::from_secs(5);

    const NETWORK_QUORUM_ROUND_LAG_THRESHOLD: u32 = 0;
    pub(crate) const EXCLUSION_THRESHOLD_PERCENTAGE: u64 = 10;

    pub(crate) fn new(context: Arc<Context>, propagation_scores: ReputationScores) -> Self {
        let mut state_map = HashMap::new();
        for (id, _) in context.committee.authorities() {
            state_map.insert(id, AncestorInfo::new());
        }

        let quorum_round_per_authority = vec![(0, 0); context.committee.size()];
        Self {
            context,
            state_map,
            propagation_scores,
            quorum_round_per_authority,
        }
    }

    pub(crate) fn set_quorum_round_per_authority(&mut self, quorum_rounds: Vec<QuorumRound>) {
        info!("Quorum round per authority set to: {quorum_rounds:?}");
        self.quorum_round_per_authority = quorum_rounds;
    }

    pub(crate) fn set_propagation_scores(&mut self, scores: ReputationScores) {
        self.propagation_scores = scores;
    }

    pub(crate) fn get_ancestor_states(&self) -> HashMap<AuthorityIndex, AncestorState> {
        self.state_map
            .iter()
            .map(|(&id, info)| (id, info.state))
            .collect()
    }

    /// Updates the state of all ancestors based on the latest scores and quorum rounds
    pub(crate) fn update_all_ancestors_state(&mut self) {
        // If round prober has not run yet and we don't have network quorum round,
        // it is okay because network_low_quorum_round will be zero and we will
        // include all ancestors until we get more information.
        let network_low_quorum_round = self.calculate_network_low_quorum_round();
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
        let ancestor_info = self.state_map.get_mut(&authority_id).expect(&format!(
            "Expected authority_id {authority_id} to be initialized in state_map",
        ));

        // Check if the lock period has expired
        if let Some(expiry) = ancestor_info.lock_expiry_time {
            if Instant::now() < expiry {
                // If still locked, return without making any changes
                return;
            } else {
                ancestor_info.lock_expiry_time = None;
            }
        }

        let low_score_threshold =
            (self.propagation_scores.high_score() * Self::EXCLUSION_THRESHOLD_PERCENTAGE) / 100;

        match ancestor_info.state {
            // Check conditions to switch to EXCLUDE state
            AncestorState::Include => {
                if propagation_score <= low_score_threshold {
                    ancestor_info.state = AncestorState::Exclude(propagation_score);
                    ancestor_info.lock_expiry_time =
                        Some(Instant::now() + Self::STATE_LOCK_DURATION);
                    info!(
                        "Authority {authority_id} moved to EXCLUDE state with score {propagation_score} <= threshold of {low_score_threshold} and locked for {:?}",
                        Self::STATE_LOCK_DURATION
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
                    || authority_low_quorum_round
                        >= (network_low_quorum_round - Self::NETWORK_QUORUM_ROUND_LAG_THRESHOLD)
                {
                    ancestor_info.state = AncestorState::Include;
                    ancestor_info.lock_expiry_time =
                        Some(Instant::now() + Self::STATE_LOCK_DURATION);
                    info!(
                        "Authority {authority_id} moved to INCLUDE state with {propagation_score} > threshold of {low_score_threshold} or {authority_low_quorum_round} >= {} and locked for {:?}.",
                        (network_low_quorum_round - Self::NETWORK_QUORUM_ROUND_LAG_THRESHOLD),
                        Self::STATE_LOCK_DURATION
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
    }

    /// Calculate the network's low quorum round from 2f+1 authorities by stake,
    /// where low quorum round is the highest round a block has been seen by 2f+1
    /// authorities.
    fn calculate_network_low_quorum_round(&self) -> u32 {
        let committee = &self.context.committee;
        let quorum_threshold = committee.quorum_threshold();
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
            let reached_quorum_before = total_stake >= quorum_threshold;
            total_stake += stake;
            if !reached_quorum_before && total_stake >= quorum_threshold {
                network_low_quorum_round = *round;
                break;
            }
        }

        network_low_quorum_round
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

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
    // EXCLUDE -> INCLUDE (BLocked due to lock)
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
        for state in state_map.values() {
            assert_eq!(*state, AncestorState::Include);
        }

        let scores = ReputationScores::new((1..=300).into(), vec![10, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state();

        // Score threshold for exclude is (100 * 10) / 100 = 10
        // 2 authorities should be excluded in with this threshold
        let state_map = ancestor_state_manager.get_ancestor_states();
        println!("{state_map:?}");
        for (authority, state) in state_map {
            if (0..=1).contains(&authority.value()) {
                assert_eq!(state, AncestorState::Exclude(10));
            } else {
                assert_eq!(state, AncestorState::Include);
            }
        }

        let scores = ReputationScores::new((1..=300).into(), vec![100, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state();

        // Score threshold for exclude is (100 * 10) / 100 = 10
        // No authority should be excluded in with this threshold, but authority 0
        // was just marked as excluded so it will remain in the EXCLUDED state
        // until 5s pass.
        let state_map = ancestor_state_manager.get_ancestor_states();
        println!("{state_map:?}");
        for (authority, state) in state_map {
            if (0..=1).contains(&authority.value()) {
                assert_eq!(state, AncestorState::Exclude(10));
            } else {
                assert_eq!(state, AncestorState::Include);
            }
        }

        // Sleep until the lock expires
        tokio::time::sleep_until(Instant::now() + Duration::from_secs(5)).await;

        ancestor_state_manager.update_all_ancestors_state();

        // Authority 0 should now be included again because scores are above the
        // threshold. Authority 1 should also be included again because quorum
        // rounds are at the network quorum round threashold 225 >= 225
        let state_map = ancestor_state_manager.get_ancestor_states();
        println!("{state_map:?}");
        for (_, state) in state_map {
            assert_eq!(state, AncestorState::Include);
        }

        let scores = ReputationScores::new((1..=300).into(), vec![100, 10, 100, 100]);
        ancestor_state_manager.set_propagation_scores(scores);
        ancestor_state_manager.update_all_ancestors_state();

        // Ancestor 1 cannot transition to EXCLUDE state until lock expires.
        let state_map = ancestor_state_manager.get_ancestor_states();
        println!("{state_map:?}");
        for (_, state) in state_map {
            assert_eq!(state, AncestorState::Include);
        }

        // Sleep until the lock expires
        tokio::time::sleep_until(Instant::now() + Duration::from_secs(5)).await;

        ancestor_state_manager.update_all_ancestors_state();

        // Ancestor 1 can transition to EXCLUDE state now that the lock expired.
        let state_map = ancestor_state_manager.get_ancestor_states();
        println!("{state_map:?}");
        for (authority, state) in state_map {
            if authority.value() == 1 {
                assert_eq!(state, AncestorState::Exclude(10));
            } else {
                assert_eq!(state, AncestorState::Include);
            }
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::HashMap, fmt::{Debug, Formatter}, ops::Range, sync::Arc};


use parking_lot::RwLock;
use rand::{prelude::SliceRandom, rngs::StdRng, SeedableRng};

use consensus_config::{Authority, AuthorityIndex, Stake};

use crate::{context::Context, CommitIndex, CommittedSubDag, Round};

/// The LeaderSchedule is responsible for producing the leader schedule across
/// an epoch. For now it is a simple wrapper around Context to provide a leader
/// for a round deterministically.
// TODO: complete full leader schedule changes
#[derive(Clone)]
pub(crate) struct LeaderSchedule {
    context: Arc<Context>,
    pub num_commits_per_schedule: u64,
    pub leader_swap_table: Arc<RwLock<LeaderSwapTable>>,
}

impl LeaderSchedule {
    /// The window where the schedule change takes place in consensus. It represents
    /// number of committed sub dags.
    /// TODO: move this to protocol config
    const CONSENSUS_COMMITS_PER_SCHEDULE: u64 = 300;

    pub fn new(context: Arc<Context> ) -> Self {
        Self {
            context,
            num_commits_per_schedule: Self::CONSENSUS_COMMITS_PER_SCHEDULE,
            leader_swap_table: Arc::new(RwLock::new(LeaderSwapTable::default())),
        }
    }

    /// Restores the LeaderSchedule by using the storage. It will attempt to retrieve the last committed
    /// "final" ReputationScores and use them to create build a LeaderSwapTable to use for the LeaderSchedule.
    // pub fn from_store(
    //     committee: Committee,
    //     store: Arc<ConsensusStore>,
    //     protocol_config: ProtocolConfig,
    // ) -> Self {
    //     let table = store
    //         .read_latest_commit_with_final_reputation_scores()
    //         .map_or(LeaderSwapTable::default(), |commit| {
    //             LeaderSwapTable::new(
    //                 &committee,
    //                 commit.leader_round(),
    //                 &commit.reputation_score(),
    //                 protocol_config.consensus_bad_nodes_stake_threshold(),
    //             )
    //         });
    //     // create the schedule
    //     Self::new(committee, table)
    // }

    // The remaining number of commits until the leader schedule change. This is
    // used to determine how many committed subdags to collect with the current 
    // leader schedule change.
    // pub fn num_commits_remaining_in_schedule(&self) -> usize {
    //     self.num_commits_per_schedule.saturating_sub(self.committed_subdags.len() as u64) as usize
    // }

    /// Atomically updates the leader swap table with the new provided one. Any leader queried from
    /// now on will get calculated according to this swap table until a new one is provided again.
    pub fn update_leader_swap_table(&self, table: LeaderSwapTable) {
        // tracing::trace!("Updating swap table {:?}", table);

        let mut write = self.leader_swap_table.write();
        *write = table;
    }

    pub fn elect_leader(&self, round: u32, leader_offset: u32) -> AuthorityIndex {
        cfg_if::cfg_if! {
            // TODO: we need to differentiate the leader strategy in tests, so for
            // some type of testing (ex sim tests) we can use the staked approach.

            // Add the logic to use the leader swap table. If the leader is a bad node
            // then we swap it with a good node.

            if #[cfg(test)] {
                AuthorityIndex::new_for_test((round + leader_offset) % self.context.committee.size() as u32)
            } else {
                self.elect_leader_stake_based(round, leader_offset)
            }
        }
    }

    pub fn elect_leader_stake_based(&self, round: u32, offset: u32) -> AuthorityIndex {
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
}

#[derive(Default, Clone)]
pub struct LeaderSwapTable {
    /// TODO(arun): remove this? multi leader means its might need to be a slot
    /// The round on which the leader swap table get into effect.
    round: Round,
    /// The list of `f` (by stake) authorities with best scores as those defined
    /// by the provided `ReputationScores`. Those authorities will be used in the
    /// position of the `bad_nodes` on the final leader schedule.
    good_nodes: Vec<(AuthorityIndex, Authority)>,
    /// The set of `f` (by stake) authorities with the worst scores as those defined
    /// by the provided `ReputationScores`. Every time where such authority is elected
    /// as leader on the schedule, it will swapped by one of the authorities of the
    /// `good_nodes`.
    bad_nodes: HashMap<AuthorityIndex, Authority>,
}

impl LeaderSwapTable {
    // Constructs a new table based on the provided reputation scores. The 
    // `bad_nodes_stake_threshold` designates the total (by stake) nodes that 
    // will be considered as "bad" based on their scores and will be replaced by 
    // good nodes. The `bad_nodes_stake_threshold` should be in the range of [0 - 33].
    pub fn new(
        context: Arc<Context>,
        round: Round,
        reputation_scores: &ReputationScores,
        bad_nodes_stake_threshold: u64,
    ) -> Self {
        assert!(
            (0..=33).contains(&bad_nodes_stake_threshold), 
            "The bad_nodes_stake_threshold should be in range [0 - 33], out of bounds parameter detected"
        );
        // assert!(reputation_scores.final_of_schedule, "Only reputation scores that have been calculated on the end of a schedule are accepted");

        // Calculating the good nodes
        let good_nodes = Self::retrieve_first_nodes(
            context.clone(),
            reputation_scores.authorities_by_score_desc(context.clone()).into_iter(),
            bad_nodes_stake_threshold,
        )
        .into_iter()
        .map(|authority| (context.committee.authority_index(&authority), authority))
        .collect::<Vec<(AuthorityIndex, Authority)>>();

        // Calculating the bad nodes
        // Reverse the sorted authorities to score ascending so we get the first 
        // low scorers up to the provided stake threshold.
        let bad_nodes = Self::retrieve_first_nodes(
            context.clone(),
            reputation_scores
                .authorities_by_score_desc(context.clone())
                .into_iter()
                .rev(),
            bad_nodes_stake_threshold,
        )
        .into_iter()
        .map(|authority| (context.committee.authority_index(&authority), authority))
        .collect::<HashMap<AuthorityIndex, Authority>>();

        good_nodes.iter().for_each(|(idx, good_node)| {
            tracing::debug!(
                "Good node on round {}: {} -> {}",
                round,
                good_node.hostname,
                reputation_scores
                    .scores_per_authority[idx.to_owned()]
            );
        });

        bad_nodes.iter().for_each(|(idx, bad_node)| {
            tracing::debug!(
                "Bad node on round {}: {} -> {}",
                round,
                bad_node.hostname,
                reputation_scores
                    .scores_per_authority[idx.to_owned()]
                    
            );
        });

        tracing::debug!("Reputation scores on round {round}: {reputation_scores:?}");

        Self {
            round,
            good_nodes,
            bad_nodes,
        }
    }

    /// Checks whether the provided leader is a bad performer and needs to be swapped in the schedule
    /// with a good performer. If not, then the method returns None. Otherwise the leader to swap with
    /// is returned instead. The `leader_round` represents the DAG round on which the provided AuthorityIdentifier
    /// is a leader on and is used as a seed to random function in order to calculate the good node that
    /// will swap in that round with the bad node. We are intentionally not doing weighted randomness as
    /// we want to give to all the good nodes equal opportunity to get swapped with bad nodes and not
    /// have one node with enough stake end up swapping bad nodes more frequently than the others on
    /// the final schedule.
    pub fn swap(&self, leader: &AuthorityIndex, leader_round: Round) -> Option<Authority> {
        if self.bad_nodes.contains_key(leader) {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[32 - 8..].copy_from_slice(&leader_round.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);

            let (idx, good_node) = self
                .good_nodes
                .choose(&mut rng)
                .expect("There should be at least one good node available");

            tracing::trace!(
                "Swapping bad leader {} -> {} for round {}",
                leader,
                idx,
                leader_round
            );

            return Some(good_node.to_owned());
        }
        None
    }

    // Retrieves the first nodes provided by the iterator `authorities` until the `stake_threshold` has been
    // reached. The `stake_threshold` should be between [0, 100] and expresses the percentage of stake that is
    // considered the cutoff. It's the caller's responsibility to ensure that the elements of the `authorities`
    // input is already sorted.
    fn retrieve_first_nodes(
        context: Arc<Context>,
        authorities: impl Iterator<Item = (AuthorityIndex, u64)>,
        stake_threshold: u64,
    ) -> Vec<Authority> {
        let mut filtered_authorities = Vec::new();

        let mut stake = 0;
        for (authority_idx, _score) in authorities {
            stake += context.committee.stake(authority_idx);

            // if the total accumulated stake has surpassed the stake threshold then we omit this
            // last authority and we exit the loop.
            if stake > (stake_threshold * context.committee.total_stake()) / 100 as Stake {
                break;
            }
            filtered_authorities.push(context.committee.authority(authority_idx).to_owned());
        }

        filtered_authorities
    }
}

impl Debug for LeaderSwapTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "LeaderSwapTable round:{}, good_nodes:{:?} with stake:{}, bad_nodes:{:?} with stake:{}",
            self.round,
            self.good_nodes
                .iter()
                .map(|(idx, _auth)| idx.to_owned())
                .collect::<Vec<AuthorityIndex>>(),
            self.good_nodes.iter().map(|(_idx, auth)| auth.stake).sum::<Stake>(),
            self.bad_nodes
                .iter()
                .map(|(idx, _auth)| idx.to_owned())
                .collect::<Vec<AuthorityIndex>>(),
            self.bad_nodes.iter().map(|(_idx, auth)| auth.stake).sum::<Stake>(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ReputationScores {
    /// Score per authority. Vec index is the AuthorityIndex.
    pub scores_per_authority: Vec<u64>,
    /// The range of commits these scores were calculated from.
    /// TODO (arun): may want to remove this and just add this to the store as a key
    pub commit_range: Range<CommitIndex> 
}

impl ReputationScores {
    pub fn new(context: Arc<Context>) -> Self {
        let num_authorities = context.committee.size();
        let scores_per_authority = vec![0_u64; num_authorities];
        // TODO(arun): Make this a parameter, and then loop through leaders and add score for each certified link
        let commit_range = 0..0;

        Self {
            scores_per_authority,
            commit_range,
        }
    }
    /// Adds the provided `score` to the existing score for the provided `authority`
    pub fn add_score(&mut self, authority_idx: AuthorityIndex, score: u64) {
        self.scores_per_authority[authority_idx] = self.scores_per_authority[authority_idx] + score;
    }

    pub fn total_authorities(&self) -> u64 {
        self.scores_per_authority.len() as u64
    }

    // Returns the authorities in score descending order.
    pub fn authorities_by_score_desc(&self, context: Arc<Context>) -> Vec<(AuthorityIndex, u64)> {
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
}

#[cfg(test)]
mod tests {
    use consensus_config::{local_committee_and_keys, Parameters};
    use sui_protocol_config::ProtocolConfig;

    use super::*;
    use crate::metrics::test_metrics;

    #[test]
    fn test_elect_leader() {
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
        let leader_schedule = LeaderSchedule::new(context);

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
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
        let leader_schedule = LeaderSchedule::new(context);

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
}

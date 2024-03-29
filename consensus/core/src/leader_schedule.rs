// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering, 
    ops::Bound::{Excluded, Included},
    collections::{BTreeMap, HashMap}, 
    fmt::{Debug, Formatter}, 
    sync::Arc
};

use parking_lot::RwLock;
use rand::{prelude::SliceRandom, rngs::StdRng, SeedableRng};

use consensus_config::{Authority, AuthorityIndex, Stake};

use crate::{
    base_committer::BaseCommitter, 
    block::{BlockAPI,BlockDigest, BlockRef, Slot, VerifiedBlock},
    commit::CommitRange, 
    context::Context, 
    dag_state::DagState, 
    stake_aggregator::{QuorumThreshold, StakeAggregator}, 
    universal_committer::UniversalCommitter, 
    CommittedSubDag, 
    Round
};

/// The `LeaderSchedule` is responsible for producing the leader schedule across
/// an epoch. The leader schedule is subject to change periodically based on 
/// `ReputationScores` of the authorities.
#[derive(Clone)]
pub(crate) struct LeaderSchedule {
    context: Arc<Context>,
    pub num_commits_per_schedule: u64,
    pub leader_swap_table: Arc<RwLock<LeaderSwapTable>>,
}

impl LeaderSchedule {
    /// The window where the schedule change takes place in consensus. It represents
    /// number of committed sub dags.
    /// TODO(arun): move this to protocol config
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
    pub(crate) fn from_store(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let leader_swap_table = dag_state
        .read()
            .last_reputation_scores_from_store()
            .map_or(LeaderSwapTable::default(), |(commit_range, scores_per_authority)| {
                LeaderSwapTable::new(
                    context.clone(),
                    ReputationScores::new(commit_range, scores_per_authority),
                    context.protocol_config.consensus_bad_nodes_stake_threshold()
                )
            });
        // create the schedule
        Self::new(context, leader_swap_table)
    }

    pub(crate) fn commits_until_leader_schedule_update(&self, dag_state: Arc<RwLock<DagState>>) -> usize {
        let unscored_committed_subdags_count = dag_state.read().unscored_committed_subdags_count();
        assert!(unscored_committed_subdags_count <= self.num_commits_per_schedule, "Unscored committed subdags count exceeds the number of commits per schedule");
        self.num_commits_per_schedule
                     .saturating_sub(unscored_committed_subdags_count)
                     as usize
    }

    pub(crate) fn update_leader_schedule(&self, dag_state: Arc<RwLock<DagState>>, committer: &UniversalCommitter) {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["LeaderSchedule::update_leader_schedule"])
            .start_timer();

        let mut dag_state = dag_state.write();
        let unscored_subdags = dag_state.take_unscored_committed_subdags();
        
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
        )
        .calculate();
        drop(score_calculation_timer);

        reputation_scores.update_metrics(self.context.clone());

        self.update_leader_swap_table(LeaderSwapTable::new(
            self.context.clone(),
            reputation_scores.clone(),
            self.context.protocol_config.consensus_bad_nodes_stake_threshold(),
        ));

        self.context.metrics.node_metrics
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
                table.swap(&leader, round, leader_offset).unwrap_or(leader)
            } else {
                let leader = self.elect_leader_stake_based(round, leader_offset);
                let table = self.leader_swap_table.read();
                table.swap(&leader, round, leader_offset).unwrap_or(leader)
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
        tracing::trace!("Updating {:?}", table);

        let mut write = self.leader_swap_table.write();
        *write = table;
    }
}

#[derive(Default, Clone)]
pub(crate) struct LeaderSwapTable {
    /// The list of `f` (by stake) authorities with best scores as those defined
    /// by the provided `ReputationScores`. Those authorities will be used in the
    /// position of the `bad_nodes` on the final leader schedule.
    pub good_nodes: Vec<(AuthorityIndex, Authority)>,
    /// The set of `f` (by stake) authorities with the worst scores as those defined
    /// by the provided `ReputationScores`. Every time where such authority is elected
    /// as leader on the schedule, it will swapped by one of the authorities of the
    /// `good_nodes`.
    pub bad_nodes: HashMap<AuthorityIndex, Authority>,

    // The scores for which the leader swap table was built from. Only part of this
    // struct for debugging purposes. Once `good_nodes` & `bad_nodes` the 
    // `reputation_scores` are no longer needed functionally for the swap table.
    pub reputation_scores: ReputationScores,
}

impl LeaderSwapTable {
    // Constructs a new table based on the provided reputation scores. The 
    // `bad_nodes_stake_threshold` designates the total (by stake) nodes that 
    // will be considered as "bad" based on their scores and will be replaced by 
    // good nodes. The `bad_nodes_stake_threshold` should be in the range of [0 - 33].
    pub fn new(
        context: Arc<Context>,
        reputation_scores: ReputationScores,
        bad_nodes_stake_threshold: u64,
    ) -> Self {
        assert!(
            (0..=33).contains(&bad_nodes_stake_threshold), 
            "The bad_nodes_stake_threshold should be in range [0 - 33], out of bounds parameter detected"
        );

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
                "Good node {} with score {} for {:?}",
                good_node.hostname,
                reputation_scores
                .scores_per_authority[idx.to_owned()],
                reputation_scores.commit_range,
            );
        });

        bad_nodes.iter().for_each(|(idx, bad_node)| {
            tracing::debug!(
                "Bad node {} with score {} for {:?}",
                bad_node.hostname,
                reputation_scores
                    .scores_per_authority[idx.to_owned()],
                reputation_scores.commit_range,
                    
            );
        });

        tracing::debug!("{reputation_scores:?}");

        Self {
            good_nodes,
            bad_nodes,
            reputation_scores
        }
    }

    /// Checks whether the provided leader is a bad performer and needs to be swapped in the schedule
    /// with a good performer. If not, then the method returns None. Otherwise the leader to swap with
    /// is returned instead. The `leader_round` & `leader_offset` represents the DAG slot on which 
    /// the provided `AuthorityIndex` is a leader on and is used as a seed to random function in order 
    /// to calculate the good node that will swap in that round with the bad node. We are intentionally 
    /// not doing weighted randomness as we want to give to all the good nodes equal opportunity to get 
    /// swapped with bad nodes and nothave one node with enough stake end up swapping bad nodes more 
    /// frequently than the others on the final schedule.
    pub fn swap(&self, leader: &AuthorityIndex, leader_round: Round, leader_offset: u32) -> Option<AuthorityIndex> {
        if self.bad_nodes.contains_key(leader) {
            tracing::info!("Elected leader {leader} is a bad leader, processing swap...");
            let mut seed_bytes = [0u8; 32];
            seed_bytes[24..28].copy_from_slice(&leader_round.to_le_bytes());
            seed_bytes[28..32].copy_from_slice(&leader_offset.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);
            
            let (idx, _good_node) = self
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
        } else {
            tracing::info!("Elected leader {leader} is not a bad leader, no swap needed.");
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
    ) -> Vec<Authority> {
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

            let authority = context.committee.authority(authority_idx).to_owned();
            filtered_authorities.push(authority);
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

pub(crate) struct ReputationScoreCalculator<'a> {
    context: Arc<Context>,
    unscored_blocks: BTreeMap<BlockRef, VerifiedBlock>,
    committer: &'a UniversalCommitter,
    pub commit_range: CommitRange,
    pub scores_per_authority: Vec<u64>
}

impl<'a> ReputationScoreCalculator<'a> {
    pub(crate) fn new(
        context: Arc<Context>,
        committer: &'a UniversalCommitter,
        unscored_subdags: &Vec<CommittedSubDag>,
    ) -> Self {
        let num_authorities = context.committee.size();
        let scores_per_authority = vec![0_u64; num_authorities];

        let unscored_blocks = unscored_subdags
            .iter()
            .flat_map(|subdag| subdag.blocks.iter())
            .map(|block| (block.reference(), block.clone()))
            .collect::<BTreeMap<_, _>>();

        assert!(!unscored_subdags.is_empty(), "Attempted to calculate scores with no unscored subdags");
        let commit_indexes = unscored_subdags
            .iter()
            .map(|subdag| subdag.commit_index)
            .collect::<Vec<_>>();
        let min_commit_index = *commit_indexes.iter().min().unwrap();
        let max_commit_index = *commit_indexes.iter().max().unwrap();
        let commit_range = CommitRange::new(min_commit_index..max_commit_index);

        Self {
            context,
            unscored_blocks,
            committer,
            commit_range,
            scores_per_authority
        }
    }

    pub(crate) fn calculate(&mut self) -> ReputationScores {
        assert!(!self.unscored_blocks.is_empty(), "Attempted to calculate scores with no blocks from unscored subdags");
        let rounds = self.unscored_blocks
            .iter()
            .map(|(block_ref, _)| block_ref.round);
        let min_round = rounds.clone().min().unwrap();
        let max_round = rounds.max().unwrap();

        // We will search for certificates for leaders up to R - 3.
        for round in min_round..=(max_round - 3) {
            for committer in self.committer.committers.iter() {

                if let Some(leader_slot) = committer.elect_leader(round) {
                    self.calculate_scores_for_leader(leader_slot, committer);
                }
            }
        }

        ReputationScores::new(self.commit_range.clone(), self.scores_per_authority.clone())
    }


    pub(crate) fn calculate_scores_for_leader(
        &mut self, 
        leader_slot: Slot, 
        committer: &BaseCommitter,
        )  {
            let wave = committer.wave_number(leader_slot.round);
            let decision_round = committer.decision_round(wave);
    
            let leader_blocks = self.get_blocks_at_slot(leader_slot);
    
            if leader_blocks.is_empty() {
                tracing::info!("[{}] No block for leader slot {leader_slot} in this set of unscored committed subdags, skip scoring", self.context.own_index);
                return;
            }
    
            // At this point we are guaranteed that there is only one leader per slot
            // because we are operating on committed subdags.
            assert!(leader_blocks.len() == 1);
    
            let leader_block = leader_blocks.first().unwrap();
    
            // TODO(arun): move to a separate "scoring strategy" method. Will need to do points 
            // for votes connected to certificates (certified vote). Can experiment with 
            // point per certificate  or 1 point per 2f+1 certs
            let decision_blocks = self.get_blocks_at_round(decision_round);
            let mut all_votes = HashMap::new();
            for potential_cert in decision_blocks {
                let authority = potential_cert.reference().author;
                if self.is_certificate(
                    &potential_cert,
                    leader_block,
                    &mut all_votes,
                ) {
                    tracing::info!("Found a certificate for leader {leader_block} from authority {authority}");
                    tracing::info!("[{}] scores +1 reputation for {authority}!",  self.context.own_index);
                    self.add_score(authority, 1);
                }
            }   
        
    }

    /// Adds the provided `score` to the existing score for the provided `authority`
    fn add_score(
        &mut self,
        authority_idx: AuthorityIndex, 
        score: u64
    ) {
        self.scores_per_authority[authority_idx] = self.scores_per_authority[authority_idx] + score;
    }

    fn find_supported_block(
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
            let ancestor = self.get_block(ancestor)
                .unwrap_or_else(|| panic!("Block not found in committed subdag: {:?}", ancestor));
            if let Some(support) = self.find_supported_block(leader_slot, &ancestor) {
                return Some(support);
            }
        }
        None
    }

    fn is_vote(
        &self,
        potential_vote: &VerifiedBlock,
        leader_block: &VerifiedBlock,
    ) -> bool {
        let reference = leader_block.reference();
        let leader_slot = Slot::from(reference);
        self.find_supported_block(leader_slot, potential_vote) == Some(reference)
    }

    fn is_certificate(
        &self,
        potential_certificate: &VerifiedBlock,
        leader_block: &VerifiedBlock,
        all_votes: &mut HashMap<BlockRef, bool>,
    ) -> bool {
        let mut votes_stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for reference in potential_certificate.ancestors() {
            let is_vote = if let Some(is_vote) = all_votes.get(reference) {
                *is_vote
            } else {
                let potential_vote = self.get_block(reference)
                    .unwrap_or_else(|| panic!("Block not found in committed subdags: {:?}", reference));
                let is_vote = self.is_vote(&potential_vote, leader_block);
                all_votes.insert(*reference, is_vote);
                is_vote
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

    fn get_blocks_at_slot(
        &self,
        slot: Slot,
    ) -> Vec<VerifiedBlock> {
        let mut blocks = vec![];
        for (_block_ref, block) in self.unscored_blocks.range((
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MIN)),
            Included(BlockRef::new(slot.round, slot.authority, BlockDigest::MAX)),
        )) {
            blocks.push(block.clone())
        }
        blocks
    }

    fn get_blocks_at_round(
        &self,
        round: Round,
    ) -> Vec<VerifiedBlock> {
        let mut blocks = vec![];
        for (_block_ref, block) in self.unscored_blocks.range((
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

    fn get_block(
        &self,
        block_ref: &BlockRef,
    ) -> Option<VerifiedBlock> {
        self.unscored_blocks.get(block_ref).cloned()
    }
}



#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ReputationScores {
    /// Score per authority. Vec index is the AuthorityIndex.
    pub scores_per_authority: Vec<u64>,
    // The range of commits these scores were calculated from.
    pub commit_range: CommitRange
}

impl ReputationScores {
    pub(crate) fn new(commit_range: CommitRange, scores_per_authority: Vec<u64>) -> Self {
        Self {
            scores_per_authority,
            commit_range,
        }
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

    pub(crate) fn update_metrics(&self, context: Arc<Context>)  {
        let authorities = self.authorities_by_score_desc(context.clone());
        for (authority_index, score) in authorities {
            let authority = context.committee.authority(authority_index);
            if !authority.hostname.is_empty() {
                context.metrics.node_metrics
                    .reputation_scores
                    .with_label_values(&[&authority.hostname])
                    .set(score as i64);
            }
        }
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
        let committee = local_committee_and_keys(0, vec![1, 1, 1, 1]).0;
        let metrics = test_metrics();
        let context = Arc::new(Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        ));
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
}

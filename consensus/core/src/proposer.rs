// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, iter, sync::Arc, time::Duration};

use consensus_config::ProtocolKeyPair;
use consensus_types::block::{BlockRef, BlockTimestampMs, Round};
use parking_lot::RwLock;
use tokio::time::Instant;
use tracing::{debug, info, trace};

use crate::{
    ancestor::{AncestorState, AncestorStateManager},
    block::{
        Block, BlockAPI, BlockV1, BlockV2, ExtendedBlock, GENESIS_ROUND, SignedBlock, Slot,
        VerifiedBlock,
    },
    context::Context,
    dag_state::DagState,
    round_tracker::RoundTracker,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    transaction::TransactionConsumer,
    transaction_vote_tracker::TransactionVoteTracker,
};

const MAX_COMMIT_VOTES_PER_BLOCK: usize = 100;

/// Trait for handling block proposal logic.
/// Only Validators have a proposer; Observers use None for the proposer field in Core.
pub(crate) trait Proposer: Send + Sync {
    /// Attempts to create and return a new block proposal.
    /// Returns None if conditions for proposal are not met.
    fn try_new_block(&mut self, force: bool) -> Option<ExtendedBlock>;

    /// Returns whether this node should propose blocks
    fn should_propose(&self) -> bool;

    /// Sets the propagation delay (validators only)
    fn set_propagation_delay(&mut self, delay: Round);

    /// Sets the last known proposed round (validators only)
    fn set_last_known_proposed_round(&mut self, round: Round);

    /// Gets the last known proposed round if applicable
    fn get_last_known_proposed_round(&self) -> Option<Round>;

    /// Returns the round of the last proposed block (validators only)
    fn last_proposed_round(&self) -> Round;

    /// Returns the last proposed block (validators only)
    fn last_proposed_block(&self) -> VerifiedBlock;

    /// Sets propagation scores on the ancestor state manager (validators only)
    fn set_propagation_scores(&mut self, scores: crate::leader_scoring::ReputationScores);

    /// Notifies transaction consumer about committed own blocks (validators only)
    fn notify_own_blocks_committed(&self, block_refs: Vec<BlockRef>, gc_round: Round);

    /// Returns the round tracker for tests (validators only)
    #[cfg(test)]
    fn round_tracker_for_tests(&self) -> Arc<RwLock<RoundTracker>>;
}

/// Validator proposal engine - full block proposal implementation
pub(crate) struct ValidatorProposer {
    context: Arc<Context>,
    transaction_consumer: TransactionConsumer,
    transaction_vote_tracker: TransactionVoteTracker,
    propagation_delay: Round,
    last_included_ancestors: Vec<Option<BlockRef>>,
    block_signer: ProtocolKeyPair,
    last_known_proposed_round: Option<Round>,
    ancestor_state_manager: AncestorStateManager,
    round_tracker: Arc<RwLock<RoundTracker>>,
    dag_state: Arc<RwLock<DagState>>,
    committer: Arc<crate::universal_committer::UniversalCommitter>,
}

impl ValidatorProposer {
    pub(crate) fn new(
        dag_state: Arc<RwLock<DagState>>,
        context: Arc<Context>,
        transaction_consumer: TransactionConsumer,
        transaction_vote_tracker: TransactionVoteTracker,
        block_signer: ProtocolKeyPair,
        last_known_proposed_round: Option<Round>,
        ancestor_state_manager: AncestorStateManager,
        round_tracker: Arc<RwLock<RoundTracker>>,
        committer: Arc<crate::universal_committer::UniversalCommitter>,
    ) -> Self {
        let last_included_ancestors = vec![None; context.committee.size()];
        Self {
            context,
            transaction_consumer,
            transaction_vote_tracker,
            propagation_delay: 0,
            last_included_ancestors,
            block_signer,
            last_known_proposed_round,
            ancestor_state_manager,
            round_tracker,
            dag_state,
            committer,
        }
    }

    fn last_proposed_block(&self) -> VerifiedBlock {
        self.dag_state
            .read()
            .get_last_proposed_block()
            .expect("A block should have been returned")
    }

    fn last_proposed_timestamp_ms(&self) -> BlockTimestampMs {
        self.dag_state
            .read()
            .get_last_proposed_block()
            .expect("A block should have been returned")
            .timestamp_ms()
    }

    fn leaders(&self, round: Round) -> Vec<Slot> {
        self.committer
            .get_leaders(round)
            .into_iter()
            .map(|authority_index| Slot::new(round, authority_index))
            .collect()
    }

    fn first_leader(&self, round: Round) -> consensus_config::AuthorityIndex {
        self.leaders(round).first().unwrap().authority
    }

    fn leaders_exist(&self, round: Round) -> bool {
        let dag_state = self.dag_state.read();
        for leader in self.leaders(round) {
            // Search for all the leaders. If at least one is not found, then return false.
            // A linear search should be fine here as the set of elements is not expected to be small enough and more sophisticated
            // data structures might not give us much here.
            if !dag_state.contains_cached_block_at_slot(leader) {
                return false;
            }
        }
        true
    }

    /// Retrieves the next ancestors to propose to form a block at `clock_round` round.
    /// If smart selection is enabled then this will try to select the best ancestors
    /// based on the propagation scores of the authorities.
    fn smart_ancestors_to_propose(
        &mut self,
        clock_round: Round,
        smart_select: bool,
    ) -> (Vec<VerifiedBlock>, BTreeSet<BlockRef>) {
        let node_metrics = &self.context.metrics.node_metrics;
        let _s = node_metrics
            .scope_processing_time
            .with_label_values(&["ValidatorProposer::smart_ancestors_to_propose"])
            .start_timer();

        // Now take the ancestors before the clock_round (excluded) for each authority.
        let all_ancestors = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(clock_round);

        assert_eq!(
            all_ancestors.len(),
            self.context.committee.size(),
            "Fatal error, number of returned ancestors don't match committee size."
        );

        // Ensure ancestor state is up to date before selecting for proposal.
        let accepted_quorum_rounds = self.round_tracker.read().compute_accepted_quorum_rounds();

        self.ancestor_state_manager
            .update_all_ancestors_state(&accepted_quorum_rounds);

        let ancestor_state_map = self.ancestor_state_manager.get_ancestor_states();

        let quorum_round = clock_round.saturating_sub(1);

        let mut score_and_pending_excluded_ancestors = Vec::new();
        let mut excluded_and_equivocating_ancestors = BTreeSet::new();

        // Propose only ancestors of higher rounds than what has already been proposed.
        // And always include own last proposed block first among ancestors.
        // Start by only including the high scoring ancestors. Low scoring ancestors
        // will be included in a second pass below.
        let included_ancestors = iter::once(self.last_proposed_block())
            .chain(
                all_ancestors
                    .into_iter()
                    .flat_map(|(ancestor, equivocating_ancestors)| {
                        if ancestor.author() == self.context.own_index {
                            return None;
                        }
                        if let Some(last_block_ref) =
                            self.last_included_ancestors[ancestor.author()]
                            && last_block_ref.round >= ancestor.round() {
                                return None;
                            }

                        // We will never include equivocating ancestors so add them immediately
                        excluded_and_equivocating_ancestors.extend(equivocating_ancestors);

                        let ancestor_state = ancestor_state_map[ancestor.author()];
                        match ancestor_state {
                            AncestorState::Include => {
                                trace!("Found ancestor {ancestor} with INCLUDE state for round {clock_round}");
                            }
                            AncestorState::Exclude(score) => {
                                trace!("Added ancestor {ancestor} with EXCLUDE state with score {score} to temporary excluded ancestors for round {clock_round}");
                                score_and_pending_excluded_ancestors.push((score, ancestor));
                                return None;
                            }
                        }

                        Some(ancestor)
                    }),
            )
            .collect::<Vec<_>>();

        let mut parent_round_quorum = StakeAggregator::<QuorumThreshold>::new();

        // Check total stake of high scoring parent round ancestors
        for ancestor in included_ancestors
            .iter()
            .filter(|a| a.round() == quorum_round)
        {
            parent_round_quorum.add(ancestor.author(), &self.context.committee);
        }

        if smart_select && !parent_round_quorum.reached_threshold(&self.context.committee) {
            node_metrics.smart_selection_wait.inc();
            debug!(
                "Only found {} stake of good ancestors to include for round {clock_round}, will wait for more.",
                parent_round_quorum.stake()
            );
            return (vec![], BTreeSet::new());
        }

        // Sort scores descending so we can include the best of the pending excluded
        // ancestors first until we reach the threshold.
        score_and_pending_excluded_ancestors.sort_by(|a, b| b.0.cmp(&a.0));

        let mut ancestors_to_propose = included_ancestors;
        let mut excluded_ancestors = Vec::new();
        for (score, ancestor) in score_and_pending_excluded_ancestors.into_iter() {
            let block_hostname = &self.context.committee.authority(ancestor.author()).hostname;
            if !parent_round_quorum.reached_threshold(&self.context.committee)
                && ancestor.round() == quorum_round
            {
                debug!(
                    "Including temporarily excluded parent round ancestor {ancestor} with score {score} to propose for round {clock_round}"
                );
                parent_round_quorum.add(ancestor.author(), &self.context.committee);
                ancestors_to_propose.push(ancestor);
                node_metrics
                    .included_excluded_proposal_ancestors_count_by_authority
                    .with_label_values(&[block_hostname.as_str(), "timeout"])
                    .inc();
            } else {
                excluded_ancestors.push((score, ancestor));
            }
        }

        // Iterate through excluded ancestors and include the ancestor or the ancestor's ancestor
        // that has been accepted by a quorum of the network. If the original ancestor itself
        // is not included then it will be part of excluded ancestors that are not
        // included in the block but will still be broadcasted to peers.
        for (score, ancestor) in excluded_ancestors.iter() {
            let excluded_author = ancestor.author();
            let block_hostname = &self.context.committee.authority(excluded_author).hostname;
            // A quorum of validators reported to have accepted blocks from the excluded_author up to the low quorum round.
            let mut accepted_low_quorum_round = accepted_quorum_rounds[excluded_author].0;
            // If the accepted quorum round of this ancestor is greater than or equal
            // to the clock round then we want to make sure to set it to clock_round - 1
            // as that is the max round the new block can include as an ancestor.
            accepted_low_quorum_round = accepted_low_quorum_round.min(quorum_round);

            let last_included_round = self.last_included_ancestors[excluded_author]
                .map(|block_ref| block_ref.round)
                .unwrap_or(GENESIS_ROUND);
            if ancestor.round() <= last_included_round {
                // This should have already been filtered out when filtering all_ancestors.
                // Still, ensure previously included ancestors are filtered out.
                continue;
            }

            if last_included_round >= accepted_low_quorum_round {
                excluded_and_equivocating_ancestors.insert(ancestor.reference());
                trace!(
                    "Excluded low score ancestor {} with score {score} to propose for round {clock_round}: last included round {last_included_round} >= accepted low quorum round {accepted_low_quorum_round}",
                    ancestor.reference()
                );
                node_metrics
                    .excluded_proposal_ancestors_count_by_authority
                    .with_label_values(&[block_hostname])
                    .inc();
                continue;
            }

            let ancestor = if ancestor.round() <= accepted_low_quorum_round {
                // Include the ancestor block as it has been seen & accepted by a strong quorum.
                ancestor.clone()
            } else {
                // Exclude this ancestor since it hasn't been accepted by a strong quorum
                excluded_and_equivocating_ancestors.insert(ancestor.reference());
                trace!(
                    "Excluded low score ancestor {} with score {score} to propose for round {clock_round}: ancestor round {} > accepted low quorum round {accepted_low_quorum_round} ",
                    ancestor.reference(),
                    ancestor.round()
                );
                node_metrics
                    .excluded_proposal_ancestors_count_by_authority
                    .with_label_values(&[block_hostname])
                    .inc();

                // Look for an earlier block in the ancestor chain that we can include as there
                // is a gap between the last included round and the accepted low quorum round.
                //
                // Note: Only cached blocks need to be propagated. Committed and GC'ed blocks
                // do not need to be propagated.
                match self.dag_state.read().get_last_cached_block_in_range(
                    excluded_author,
                    last_included_round + 1,
                    accepted_low_quorum_round + 1,
                ) {
                    Some(earlier_ancestor) => {
                        // Found an earlier block that has been propagated well - include it instead
                        earlier_ancestor
                    }
                    None => {
                        // No suitable earlier block found
                        continue;
                    }
                }
            };
            self.last_included_ancestors[excluded_author] = Some(ancestor.reference());
            ancestors_to_propose.push(ancestor.clone());
            trace!(
                "Included low scoring ancestor {} with score {score} seen at accepted low quorum round {accepted_low_quorum_round} to propose for round {clock_round}",
                ancestor.reference()
            );
            node_metrics
                .included_excluded_proposal_ancestors_count_by_authority
                .with_label_values(&[block_hostname.as_str(), "quorum"])
                .inc();
        }

        assert!(
            parent_round_quorum.reached_threshold(&self.context.committee),
            "Fatal error, quorum not reached for parent round when proposing for round {clock_round}. Possible mismatch between DagState and Core."
        );

        debug!(
            "Included {} ancestors & excluded {} low performing or equivocating ancestors for proposal in round {clock_round}",
            ancestors_to_propose.len(),
            excluded_and_equivocating_ancestors.len()
        );

        (ancestors_to_propose, excluded_and_equivocating_ancestors)
    }
}

impl Proposer for ValidatorProposer {
    fn try_new_block(&mut self, force: bool) -> Option<ExtendedBlock> {
        if !self.should_propose() {
            return None;
        }

        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["ValidatorProposer::try_new_block"])
            .start_timer();

        // Ensure the new block has a higher round than the last proposed block.
        let clock_round = {
            let dag_state = self.dag_state.read();
            let clock_round = dag_state.threshold_clock_round();
            if clock_round
                <= dag_state
                    .get_last_proposed_block()
                    .expect("A block should have been returned")
                    .round()
            {
                debug!(
                    "Skipping block proposal for round {} as it is not higher than the last proposed block {}",
                    clock_round,
                    dag_state
                        .get_last_proposed_block()
                        .expect("A block should have been returned")
                        .round()
                );
                return None;
            }
            clock_round
        };

        // There must be a quorum of blocks from the previous round.
        let quorum_round = clock_round.saturating_sub(1);

        // Create a new block either because we want to "forcefully" propose a block due to a leader timeout,
        // or because we are actually ready to produce the block (leader exists and min delay has passed).
        if !force {
            if !self.leaders_exist(quorum_round) {
                return None;
            }

            if Duration::from_millis(
                self.context
                    .clock
                    .timestamp_utc_ms()
                    .saturating_sub(self.last_proposed_timestamp_ms()),
            ) < self.context.parameters.min_round_delay
            {
                debug!(
                    "Skipping block proposal for round {} as it is too soon after the last proposed block timestamp {}; min round delay is {}ms",
                    clock_round,
                    self.last_proposed_timestamp_ms(),
                    self.context.parameters.min_round_delay.as_millis(),
                );
                return None;
            }
        }

        // Determine the ancestors to be included in proposal.
        let (ancestors, excluded_and_equivocating_ancestors) =
            self.smart_ancestors_to_propose(clock_round, !force);

        // If we did not find enough good ancestors to propose, continue to wait before proposing.
        if ancestors.is_empty() {
            assert!(
                !force,
                "Ancestors should have been returned if force is true!"
            );
            debug!(
                "Skipping block proposal for round {} because no good ancestor is found",
                clock_round,
            );
            return None;
        }

        let excluded_ancestors_limit = self.context.committee.size() * 2;
        if excluded_and_equivocating_ancestors.len() > excluded_ancestors_limit {
            debug!(
                "Dropping {} excluded ancestor(s) during proposal due to size limit",
                excluded_and_equivocating_ancestors.len() - excluded_ancestors_limit,
            );
        }
        let excluded_ancestors = excluded_and_equivocating_ancestors
            .into_iter()
            .take(excluded_ancestors_limit)
            .collect();

        // Update the last included ancestor block refs
        for ancestor in &ancestors {
            self.last_included_ancestors[ancestor.author()] = Some(ancestor.reference());
        }

        let leader_authority = &self
            .context
            .committee
            .authority(self.first_leader(quorum_round))
            .hostname;
        self.context
            .metrics
            .node_metrics
            .block_proposal_leader_wait_ms
            .with_label_values(&[leader_authority])
            .inc_by(
                Instant::now()
                    .saturating_duration_since(self.dag_state.read().threshold_clock_quorum_ts())
                    .as_millis() as u64,
            );
        self.context
            .metrics
            .node_metrics
            .block_proposal_leader_wait_count
            .with_label_values(&[leader_authority])
            .inc();

        self.context
            .metrics
            .node_metrics
            .proposed_block_ancestors
            .observe(ancestors.len() as f64);
        for ancestor in &ancestors {
            let authority = &self.context.committee.authority(ancestor.author()).hostname;
            self.context
                .metrics
                .node_metrics
                .proposed_block_ancestors_depth
                .with_label_values(&[authority])
                .observe(clock_round.saturating_sub(ancestor.round()).into());
        }

        let now = self.context.clock.timestamp_utc_ms();
        ancestors.iter().for_each(|block| {
            if block.timestamp_ms() > now {
                trace!("Ancestor block {:?} has timestamp {}, greater than current timestamp {now}. Proposing for round {}.", block, block.timestamp_ms(), clock_round);
                let authority = &self.context.committee.authority(block.author()).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .proposed_block_ancestors_timestamp_drift_ms
                    .with_label_values(&[authority])
                    .inc_by(block.timestamp_ms().saturating_sub(now));
            }
        });

        // Consume the next transactions to be included. Do not drop the guards yet as this would acknowledge
        // the inclusion of transactions. Just let this be done in the end of the method.
        let (transactions, ack_transactions, _limit_reached) = self.transaction_consumer.next();
        self.context
            .metrics
            .node_metrics
            .proposed_block_transactions
            .observe(transactions.len() as f64);

        // Consume the commit votes to be included.
        let commit_votes = self
            .dag_state
            .write()
            .take_commit_votes(MAX_COMMIT_VOTES_PER_BLOCK);

        let transaction_votes = if self.context.protocol_config.transaction_voting_enabled() {
            let new_causal_history = {
                let mut dag_state = self.dag_state.write();
                ancestors
                    .iter()
                    .flat_map(|ancestor| dag_state.link_causal_history(ancestor.reference()))
                    .collect()
            };
            self.transaction_vote_tracker
                .get_own_votes(new_causal_history)
        } else {
            vec![]
        };

        // Create the block.
        let block = if self.context.protocol_config.transaction_voting_enabled() {
            Block::V2(BlockV2::new(
                self.context.committee.epoch(),
                clock_round,
                self.context.own_index,
                now,
                ancestors.iter().map(|b| b.reference()).collect(),
                transactions,
                commit_votes,
                transaction_votes,
                vec![],
            ))
        } else {
            Block::V1(BlockV1::new(
                self.context.committee.epoch(),
                clock_round,
                self.context.own_index,
                now,
                ancestors.iter().map(|b| b.reference()).collect(),
                transactions,
                commit_votes,
                vec![],
            ))
        };
        let signed_block =
            SignedBlock::new(block, &self.block_signer).expect("Block signing failed.");
        let serialized = signed_block
            .serialize()
            .expect("Block serialization failed.");
        self.context
            .metrics
            .node_metrics
            .proposed_block_size
            .observe(serialized.len() as f64);
        // Own blocks are assumed to be valid.
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized);

        // Record the interval from last proposal.
        let last_proposed_block = self.last_proposed_block();
        if last_proposed_block.round() > 0 {
            self.context
                .metrics
                .node_metrics
                .block_proposal_interval
                .observe(
                    Duration::from_millis(
                        verified_block
                            .timestamp_ms()
                            .saturating_sub(last_proposed_block.timestamp_ms()),
                    )
                    .as_secs_f64(),
                );
        }

        // Add the block directly to DagState (skipping BlockManager since our own blocks cannot be suspended)
        self.dag_state.write().accept_block(verified_block.clone());

        // Update proposed state of blocks in local DAG.
        if self.context.protocol_config.transaction_voting_enabled() {
            self.transaction_vote_tracker
                .add_voted_blocks(vec![(verified_block.clone(), vec![])]);
            // Set the proposed block to be linked. Linked statuses of its ancestors have already been set.
            self.dag_state
                .write()
                .link_causal_history(verified_block.reference());
        }

        // Ensure the new block and its ancestors are persisted, before broadcasting it.
        self.dag_state.write().flush();

        // Now acknowledge the transactions for their inclusion to block
        ack_transactions(verified_block.reference());

        info!("Created block {verified_block:?} for round {clock_round}");

        self.context
            .metrics
            .node_metrics
            .proposed_blocks
            .with_label_values(&[&force.to_string()])
            .inc();

        let extended_block = ExtendedBlock {
            block: verified_block,
            excluded_ancestors,
        };

        // Update round tracker with our own highest accepted blocks
        self.round_tracker
            .write()
            .update_from_verified_block(&extended_block);

        Some(extended_block)
    }

    fn should_propose(&self) -> bool {
        let clock_round = self.dag_state.read().threshold_clock_round();
        let core_skipped_proposals = &self.context.metrics.node_metrics.core_skipped_proposals;

        if self.propagation_delay
            > self
                .context
                .parameters
                .propagation_delay_stop_proposal_threshold
        {
            debug!(
                "Skip proposing for round {clock_round}, high propagation delay {} > {}.",
                self.propagation_delay,
                self.context
                    .parameters
                    .propagation_delay_stop_proposal_threshold
            );
            core_skipped_proposals
                .with_label_values(&["high_propagation_delay"])
                .inc();
            return false;
        }

        let Some(last_known_proposed_round) = self.last_known_proposed_round else {
            debug!(
                "Skip proposing for round {clock_round}, last known proposed round has not been synced yet."
            );
            core_skipped_proposals
                .with_label_values(&["no_last_known_proposed_round"])
                .inc();
            return false;
        };
        if clock_round <= last_known_proposed_round {
            debug!(
                "Skip proposing for round {clock_round} as last known proposed round is {last_known_proposed_round}"
            );
            core_skipped_proposals
                .with_label_values(&["higher_last_known_proposed_round"])
                .inc();
            return false;
        }

        true
    }

    fn set_propagation_delay(&mut self, delay: Round) {
        self.propagation_delay = delay;
    }

    fn set_last_known_proposed_round(&mut self, round: Round) {
        self.last_known_proposed_round = Some(round);
    }

    fn get_last_known_proposed_round(&self) -> Option<Round> {
        self.last_known_proposed_round
    }

    fn last_proposed_round(&self) -> Round {
        self.dag_state
            .read()
            .get_last_proposed_block()
            .expect("A block should have been returned")
            .round()
    }

    fn last_proposed_block(&self) -> VerifiedBlock {
        self.dag_state
            .read()
            .get_last_proposed_block()
            .expect("A block should have been returned")
    }

    fn set_propagation_scores(&mut self, scores: crate::leader_scoring::ReputationScores) {
        self.ancestor_state_manager.set_propagation_scores(scores);
    }

    fn notify_own_blocks_committed(&self, block_refs: Vec<BlockRef>, gc_round: Round) {
        self.transaction_consumer
            .notify_own_blocks_status(block_refs, gc_round);
    }

    #[cfg(test)]
    fn round_tracker_for_tests(&self) -> Arc<RwLock<RoundTracker>> {
        self.round_tracker.clone()
    }
}

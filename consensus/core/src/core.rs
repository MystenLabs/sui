// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    iter,
    sync::Arc,
    time::Duration,
    vec,
};

#[cfg(test)]
use consensus_config::{local_committee_and_keys, Stake};
use consensus_config::{AuthorityIndex, ProtocolKeyPair};
use itertools::Itertools as _;
#[cfg(test)]
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_macros::fail_point;
use tokio::{
    sync::{broadcast, watch},
    time::Instant,
};
use tracing::{debug, info, trace, warn};

use crate::{
    ancestor::{AncestorState, AncestorStateManager},
    block::{
        Block, BlockAPI, BlockRef, BlockTimestampMs, BlockV1, BlockV2, ExtendedBlock, Round,
        SignedBlock, Slot, VerifiedBlock, GENESIS_ROUND,
    },
    block_manager::BlockManager,
    commit::{
        CertifiedCommit, CertifiedCommits, CommitAPI, CommittedSubDag, DecidedLeader, Decision,
    },
    commit_observer::CommitObserver,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    leader_schedule::LeaderSchedule,
    round_tracker::PeerRoundTracker,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    transaction::TransactionConsumer,
    transaction_certifier::TransactionCertifier,
    universal_committer::{
        universal_committer_builder::UniversalCommitterBuilder, UniversalCommitter,
    },
};
#[cfg(test)]
use crate::{
    block::CertifiedBlocksOutput, block_verifier::NoopBlockVerifier, storage::mem_store::MemStore,
    CommitConsumer, TransactionClient,
};

// Maximum number of commit votes to include in a block.
// TODO: Move to protocol config, and verify in BlockVerifier.
const MAX_COMMIT_VOTES_PER_BLOCK: usize = 100;

pub(crate) struct Core {
    context: Arc<Context>,
    /// The consumer to use in order to pull transactions to be included for the next proposals
    transaction_consumer: TransactionConsumer,
    /// This contains the reject votes on transactions which proposed blocks should include.
    transaction_certifier: TransactionCertifier,
    /// The block manager which is responsible for keeping track of the DAG dependencies when processing new blocks
    /// and accept them or suspend if we are missing their causal history
    block_manager: BlockManager,
    /// Whether there are subscribers waiting for new blocks proposed by this authority.
    /// Core stops proposing new blocks when there is no subscriber, because new proposed blocks
    /// will likely contain only stale info when they propagate to peers.
    subscriber_exists: bool,
    /// Estimated delay by round for propagating blocks to a quorum.
    /// Because of the nature of TCP and block streaming, propagation delay is expected to be
    /// 0 in most cases, even when the actual latency of broadcasting blocks is high.
    /// When this value is higher than the `propagation_delay_stop_proposal_threshold`,
    /// most likely this validator cannot broadcast  blocks to the network at all.
    /// Core stops proposing new blocks in this case.
    propagation_delay: Round,
    /// Used to make commit decisions for leader blocks in the dag.
    committer: UniversalCommitter,
    /// The last new round for which core has sent out a signal.
    last_signaled_round: Round,
    /// The blocks of the last included ancestors per authority. This vector is basically used as a
    /// watermark in order to include in the next block proposal only ancestors of higher rounds.
    /// By default, is initialised with `None` values.
    last_included_ancestors: Vec<Option<BlockRef>>,
    /// The last decided leader returned from the universal committer. Important to note
    /// that this does not signify that the leader has been persisted yet as it still has
    /// to go through CommitObserver and persist the commit in store. On recovery/restart
    /// the last_decided_leader will be set to the last_commit leader in dag state.
    last_decided_leader: Slot,
    /// The consensus leader schedule to be used to resolve the leader for a
    /// given round.
    leader_schedule: Arc<LeaderSchedule>,
    /// The commit observer is responsible for observing the commits and collecting
    /// + sending subdags over the consensus output channel.
    commit_observer: CommitObserver,
    /// Sender of outgoing signals from Core.
    signals: CoreSignals,
    /// The keypair to be used for block signing
    block_signer: ProtocolKeyPair,
    /// Keeping track of state of the DAG, including blocks, commits and last committed rounds.
    dag_state: Arc<RwLock<DagState>>,
    /// The last known round for which the node has proposed. Any proposal should be for a round > of this.
    /// This is currently being used to avoid equivocations during a node recovering from amnesia. When value is None it means that
    /// the last block sync mechanism is enabled, but it hasn't been initialised yet.
    last_known_proposed_round: Option<Round>,
    // The ancestor state manager will keep track of the quality of the authorities
    // based on the distribution of their blocks to the network. It will use this
    // information to decide whether to include that authority block in the next
    // proposal or not.
    ancestor_state_manager: AncestorStateManager,
    // The round tracker will keep track of the highest received and accepted rounds
    // from all authorities. It will use this information to then calculate the
    // quorum rounds periodically which is used across other components to make
    // decisions about block proposals.
    round_tracker: Arc<RwLock<PeerRoundTracker>>,
}

impl Core {
    pub(crate) fn new(
        context: Arc<Context>,
        leader_schedule: Arc<LeaderSchedule>,
        transaction_consumer: TransactionConsumer,
        transaction_certifier: TransactionCertifier,
        block_manager: BlockManager,
        subscriber_exists: bool,
        commit_observer: CommitObserver,
        signals: CoreSignals,
        block_signer: ProtocolKeyPair,
        dag_state: Arc<RwLock<DagState>>,
        sync_last_known_own_block: bool,
        round_tracker: Arc<RwLock<PeerRoundTracker>>,
    ) -> Self {
        let last_decided_leader = dag_state.read().last_commit_leader();
        let number_of_leaders = context
            .protocol_config
            .mysticeti_num_leaders_per_round()
            .unwrap_or(1);
        let committer = UniversalCommitterBuilder::new(
            context.clone(),
            leader_schedule.clone(),
            dag_state.clone(),
        )
        .with_number_of_leaders(number_of_leaders)
        .with_pipeline(true)
        .build();

        let last_proposed_block = dag_state.read().get_last_proposed_block();

        let last_signaled_round = last_proposed_block.round();

        // Recover the last included ancestor rounds based on the last proposed block. That will allow
        // to perform the next block proposal by using ancestor blocks of higher rounds and avoid
        // re-including blocks that have been already included in the last (or earlier) block proposal.
        // This is only strongly guaranteed for a quorum of ancestors. It is still possible to re-include
        // a block from an authority which hadn't been added as part of the last proposal hence its
        // latest included ancestor is not accurately captured here. This is considered a small deficiency,
        // and it mostly matters just for this next proposal without any actual penalties in performance
        // or block proposal.
        let mut last_included_ancestors = vec![None; context.committee.size()];
        for ancestor in last_proposed_block.ancestors() {
            last_included_ancestors[ancestor.author] = Some(*ancestor);
        }

        let min_propose_round = if sync_last_known_own_block {
            None
        } else {
            // if the sync is disabled then we practically don't want to impose any restriction.
            Some(0)
        };

        let propagation_scores = leader_schedule
            .leader_swap_table
            .read()
            .reputation_scores
            .clone();
        let mut ancestor_state_manager =
            AncestorStateManager::new(context.clone(), dag_state.clone());
        ancestor_state_manager.set_propagation_scores(propagation_scores);

        Self {
            context,
            last_signaled_round,
            last_included_ancestors,
            last_decided_leader,
            leader_schedule,
            transaction_consumer,
            transaction_certifier,
            block_manager,
            subscriber_exists,
            propagation_delay: 0,
            committer,
            commit_observer,
            signals,
            block_signer,
            dag_state,
            last_known_proposed_round: min_propose_round,
            ancestor_state_manager,
            round_tracker,
        }
        .recover()
    }

    fn recover(mut self) -> Self {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::recover"])
            .start_timer();
        // Ensure local time is after max ancestor timestamp.
        let ancestor_blocks = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX);

        let max_ancestor_timestamp = ancestor_blocks
            .iter()
            .fold(0, |ts, (b, _)| ts.max(b.timestamp_ms()));
        let wait_ms = max_ancestor_timestamp.saturating_sub(self.context.clock.timestamp_utc_ms());
        if self
            .context
            .protocol_config
            .consensus_median_based_commit_timestamp()
        {
            info!("Median based timestamp is enabled. Will not wait for {} ms while recovering ancestors from storage", wait_ms);
        } else if wait_ms > 0 {
            warn!(
                "Waiting for {} ms while recovering ancestors from storage",
                wait_ms
            );
            std::thread::sleep(Duration::from_millis(wait_ms));
        }

        // Try to commit and propose, since they may not have run after the last storage write.
        self.try_commit(vec![]).unwrap();

        let last_proposed_block = if let Some(last_proposed_block) = self.try_propose(true).unwrap()
        {
            last_proposed_block
        } else {
            let last_proposed_block = self.dag_state.read().get_last_proposed_block();

            if self.should_propose() {
                assert!(last_proposed_block.round() > GENESIS_ROUND, "At minimum a block of round higher than genesis should have been produced during recovery");
            }

            // if no new block proposed then just re-broadcast the last proposed one to ensure liveness.
            self.signals
                .new_block(ExtendedBlock {
                    block: last_proposed_block.clone(),
                    excluded_ancestors: vec![],
                })
                .unwrap();
            last_proposed_block
        };

        // Try to set up leader timeout if needed.
        // This needs to be called after try_commit() and try_propose(), which may
        // have advanced the threshold clock round.
        self.try_signal_new_round();

        info!(
            "Core recovery completed with last proposed block {:?}",
            last_proposed_block
        );

        self
    }

    /// Processes the provided blocks and accepts them if possible when their causal history exists.
    /// The method returns:
    /// - The references of ancestors missing their block
    #[tracing::instrument(skip_all)]
    pub(crate) fn add_blocks(
        &mut self,
        blocks: Vec<VerifiedBlock>,
    ) -> ConsensusResult<BTreeSet<BlockRef>> {
        let _scope = monitored_scope("Core::add_blocks");
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::add_blocks"])
            .start_timer();
        self.context
            .metrics
            .node_metrics
            .core_add_blocks_batch_size
            .observe(blocks.len() as f64);

        let (accepted_blocks, missing_block_refs) = self.block_manager.try_accept_blocks(blocks);

        if !accepted_blocks.is_empty() {
            debug!(
                "Accepted blocks: {}",
                accepted_blocks
                    .iter()
                    .map(|b| b.reference().to_string())
                    .join(",")
            );

            // Try to commit the new blocks if possible.
            self.try_commit(vec![])?;

            // Try to propose now since there are new blocks accepted.
            self.try_propose(false)?;

            // Now set up leader timeout if needed.
            // This needs to be called after try_commit() and try_propose(), which may
            // have advanced the threshold clock round.
            self.try_signal_new_round();
        };

        if !missing_block_refs.is_empty() {
            trace!(
                "Missing block refs: {}",
                missing_block_refs.iter().map(|b| b.to_string()).join(", ")
            );
        }

        Ok(missing_block_refs)
    }

    // Adds the certified commits that have been synced via the commit syncer. We are using the commit info in order to skip running the decision
    // rule and immediately commit the corresponding leaders and sub dags. Pay attention that no block acceptance is happening here, but rather
    // internally in the `try_commit` method which ensures that everytime only the blocks corresponding to the certified commits that are about to
    // be committed are accepted.
    #[tracing::instrument(skip_all)]
    pub(crate) fn add_certified_commits(
        &mut self,
        certified_commits: CertifiedCommits,
    ) -> ConsensusResult<BTreeSet<BlockRef>> {
        let _scope = monitored_scope("Core::add_certified_commits");

        // We want to enable the commit process logic when GC is enabled.
        if self.dag_state.read().gc_enabled() {
            let votes = certified_commits.votes().to_vec();
            let commits = self
                .filter_new_commits(certified_commits.commits().to_vec())
                .expect("Certified commits validation failed");

            // Try to accept the certified commit votes.
            // Even if they may not be part of a future commit, these blocks are useful for certifying
            // commits when helping peers sync commits.
            let (_, missing_block_refs) = self.block_manager.try_accept_blocks(votes);

            // Try to commit the new blocks. Take into account the trusted commit that has been provided.
            self.try_commit(commits)?;

            // Try to propose now since there are new blocks accepted.
            self.try_propose(false)?;

            // Now set up leader timeout if needed.
            // This needs to be called after try_commit() and try_propose(), which may
            // have advanced the threshold clock round.
            self.try_signal_new_round();

            return Ok(missing_block_refs);
        }

        // If GC is not enabled then process blocks as usual.
        let blocks = certified_commits
            .commits()
            .iter()
            .flat_map(|commit| commit.blocks())
            .cloned()
            .collect::<Vec<_>>();

        self.add_blocks(blocks)
    }

    /// Checks if provided block refs have been accepted. If not, missing block refs are kept for synchronizations.
    /// Returns the references of missing blocks among the input blocks.
    pub(crate) fn check_block_refs(
        &mut self,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<BTreeSet<BlockRef>> {
        let _scope = monitored_scope("Core::check_block_refs");
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::check_block_refs"])
            .start_timer();
        self.context
            .metrics
            .node_metrics
            .core_check_block_refs_batch_size
            .observe(block_refs.len() as f64);

        // Try to find them via the block manager
        let missing_block_refs = self.block_manager.try_find_blocks(block_refs);

        if !missing_block_refs.is_empty() {
            trace!(
                "Missing block refs: {}",
                missing_block_refs.iter().map(|b| b.to_string()).join(", ")
            );
        }
        Ok(missing_block_refs)
    }

    /// If needed, signals a new clock round and sets up leader timeout.
    fn try_signal_new_round(&mut self) {
        // Signal only when the threshold clock round is more advanced than the last signaled round.
        //
        // NOTE: a signal is still sent even when a block has been proposed at the new round.
        // We can consider changing this in the future.
        let new_clock_round = self.dag_state.read().threshold_clock_round();
        if new_clock_round <= self.last_signaled_round {
            return;
        }
        // Then send a signal to set up leader timeout.
        self.signals.new_round(new_clock_round);
        self.last_signaled_round = new_clock_round;

        // Report the threshold clock round
        self.context
            .metrics
            .node_metrics
            .threshold_clock_round
            .set(new_clock_round as i64);
    }

    /// Creating a new block for the dictated round. This is used when a leader timeout occurs, either
    /// when the min timeout expires or max. When `force = true` , then any checks like previous round
    /// leader existence will get skipped.
    pub(crate) fn new_block(
        &mut self,
        round: Round,
        force: bool,
    ) -> ConsensusResult<Option<VerifiedBlock>> {
        let _scope = monitored_scope("Core::new_block");
        if self.last_proposed_round() < round {
            self.context
                .metrics
                .node_metrics
                .leader_timeout_total
                .with_label_values(&[&format!("{force}")])
                .inc();
            let result = self.try_propose(force);
            // The threshold clock round may have advanced, so a signal needs to be sent.
            self.try_signal_new_round();
            return result;
        }
        Ok(None)
    }

    /// Keeps only the certified commits that have a commit index > last commit index.
    /// It also ensures that the first commit in the list is the next one in line, otherwise it panics.
    fn filter_new_commits(
        &mut self,
        commits: Vec<CertifiedCommit>,
    ) -> ConsensusResult<Vec<CertifiedCommit>> {
        // Filter out the commits that have been already locally committed and keep only anything that is above the last committed index.
        let last_commit_index = self.dag_state.read().last_commit_index();
        let commits = commits
            .iter()
            .filter(|commit| {
                if commit.index() > last_commit_index {
                    true
                } else {
                    tracing::debug!(
                        "Skip commit for index {} as it is already committed with last commit index {}",
                        commit.index(),
                        last_commit_index
                    );
                    false
                }
            })
            .cloned()
            .collect::<Vec<_>>();

        // Make sure that the first commit we find is the next one in line and there is no gap.
        if let Some(commit) = commits.first() {
            if commit.index() != last_commit_index + 1 {
                return Err(ConsensusError::UnexpectedCertifiedCommitIndex {
                    expected_commit_index: last_commit_index + 1,
                    commit_index: commit.index(),
                });
            }
        }

        Ok(commits)
    }

    // Attempts to create a new block, persist and propose it to all peers.
    // When force is true, ignore if leader from the last round exists among ancestors and if
    // the minimum round delay has passed.
    fn try_propose(&mut self, force: bool) -> ConsensusResult<Option<VerifiedBlock>> {
        if !self.should_propose() {
            return Ok(None);
        }
        if let Some(extended_block) = self.try_new_block(force) {
            self.signals.new_block(extended_block.clone())?;

            fail_point!("consensus-after-propose");

            // The new block may help commit.
            self.try_commit(vec![])?;
            return Ok(Some(extended_block.block));
        }
        Ok(None)
    }

    /// Attempts to propose a new block for the next round. If a block has already proposed for latest
    /// or earlier round, then no block is created and None is returned.
    fn try_new_block(&mut self, force: bool) -> Option<ExtendedBlock> {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::try_new_block"])
            .start_timer();

        // Ensure the new block has a higher round than the last proposed block.
        let clock_round = {
            let dag_state = self.dag_state.read();
            let clock_round = dag_state.threshold_clock_round();
            if clock_round <= dag_state.get_last_proposed_block().round() {
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
            if self.context.protocol_config.consensus_median_based_commit_timestamp() {
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
            } else {
                // Ensure ancestor timestamps are not more advanced than the current time.
                // Also catch the issue if system's clock go backwards.
                assert!(
                    block.timestamp_ms() <= now,
                    "Violation: ancestor block {:?} has timestamp {}, greater than current timestamp {now}. Proposing for round {}.",
                    block, block.timestamp_ms(), clock_round
                );
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

        let transaction_votes = if self.context.protocol_config.mysticeti_fastpath() {
            let hard_linked_ancestors = {
                let mut dag_state = self.dag_state.write();
                ancestors
                    .iter()
                    .flat_map(|ancestor| dag_state.link_causal_history(ancestor.reference()))
                    .collect()
            };
            self.transaction_certifier
                .get_own_votes(hard_linked_ancestors)
        } else {
            vec![]
        };

        // Create the block and insert to storage.
        let block = if self.context.protocol_config.mysticeti_fastpath() {
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

        // Record the interval from last proposal, before accepting the proposed block.
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

        // Accept the block into BlockManager and DagState.
        let (accepted_blocks, missing) = self
            .block_manager
            .try_accept_blocks(vec![verified_block.clone()]);
        assert_eq!(accepted_blocks.len(), 1);
        assert!(missing.is_empty());

        // Add the block to the transaction certifier.
        if self.context.protocol_config.mysticeti_fastpath() {
            self.transaction_certifier
                .add_voted_blocks(vec![(verified_block.clone(), vec![])]);
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
            .update_from_accepted_block(&extended_block);

        Some(extended_block)
    }

    /// Runs commit rule to attempt to commit additional blocks from the DAG. If any `certified_commits` are provided, then
    /// it will attempt to commit those first before trying to commit any further leaders.
    fn try_commit(
        &mut self,
        mut certified_commits: Vec<CertifiedCommit>,
    ) -> ConsensusResult<Vec<CommittedSubDag>> {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::try_commit"])
            .start_timer();

        let mut certified_commits_map = BTreeMap::new();
        for c in &certified_commits {
            certified_commits_map.insert(c.index(), c.reference());
        }

        if !certified_commits.is_empty() {
            info!(
                "Will try to commit synced commits first : {:?}",
                certified_commits
                    .iter()
                    .map(|c| (c.index(), c.leader()))
                    .collect::<Vec<_>>()
            );
        }

        let mut committed_sub_dags = Vec::new();
        // TODO: Add optimization to abort early without quorum for a round.
        loop {
            // LeaderSchedule has a limit to how many sequenced leaders can be committed
            // before a change is triggered. Calling into leader schedule will get you
            // how many commits till next leader change. We will loop back and recalculate
            // any discarded leaders with the new schedule.
            let mut commits_until_update = self
                .leader_schedule
                .commits_until_leader_schedule_update(self.dag_state.clone());

            if commits_until_update == 0 {
                let last_commit_index = self.dag_state.read().last_commit_index();

                tracing::info!(
                    "Leader schedule change triggered at commit index {last_commit_index}"
                );

                self.leader_schedule
                    .update_leader_schedule_v2(&self.dag_state);

                let propagation_scores = self
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .reputation_scores
                    .clone();
                self.ancestor_state_manager
                    .set_propagation_scores(propagation_scores);

                commits_until_update = self
                    .leader_schedule
                    .commits_until_leader_schedule_update(self.dag_state.clone());

                fail_point!("consensus-after-leader-schedule-change");
            }
            assert!(commits_until_update > 0);

            // Always try to process the synced commits first. If there are certified commits to process then the decided leaders and the commits will be returned.
            let (mut decided_leaders, decided_certified_commits): (
                Vec<DecidedLeader>,
                Vec<CertifiedCommit>,
            ) = self
                .try_decide_certified(&mut certified_commits, commits_until_update)
                .into_iter()
                .unzip();

            // Only accept blocks for the certified commits that we are certain to sequence.
            // This ensures that only blocks corresponding to committed certified commits are flushed to disk.
            // Blocks from non-committed certified commits will not be flushed, preventing issues during crash-recovery.
            // This avoids scenarios where accepting and flushing blocks of non-committed certified commits could lead to
            // premature commit rule execution. Due to GC, this could cause a panic if the commit rule tries to access
            // missing causal history from blocks of certified commits.
            let blocks = decided_certified_commits
                .iter()
                .flat_map(|c| c.blocks())
                .cloned()
                .collect::<Vec<_>>();
            self.block_manager.try_accept_committed_blocks(blocks);

            // If there is no certified commit to process, run the decision rule.
            if decided_leaders.is_empty() {
                // TODO: limit commits by commits_until_update, which may be needed when leader schedule length is reduced.
                decided_leaders = self.committer.try_decide(self.last_decided_leader);

                // Truncate the decided leaders to fit the commit schedule limit.
                if decided_leaders.len() >= commits_until_update {
                    let _ = decided_leaders.split_off(commits_until_update);
                }
            }

            // If the decided leaders list is empty then just break the loop.
            let Some(last_decided) = decided_leaders.last().cloned() else {
                break;
            };

            self.last_decided_leader = last_decided.slot();
            self.context
                .metrics
                .node_metrics
                .last_decided_leader_round
                .set(self.last_decided_leader.round as i64);

            let sequenced_leaders = decided_leaders
                .into_iter()
                .filter_map(|leader| leader.into_committed_block())
                .collect::<Vec<_>>();
            // It's possible to reach this point as the decided leaders might all of them be "Skip" decisions. In this case there is no
            // leader to commit and we should break the loop.
            if sequenced_leaders.is_empty() {
                break;
            }
            tracing::info!(
                "Committing {} leaders: {}; {} commits before next leader schedule change",
                sequenced_leaders.len(),
                sequenced_leaders
                    .iter()
                    .map(|(b, _)| b.reference().to_string())
                    .join(","),
                commits_until_update,
            );

            // TODO: refcount subdags
            let subdags = self.commit_observer.handle_commit(sequenced_leaders)?;

            // Try to unsuspend blocks if gc_round has advanced.
            self.block_manager
                .try_unsuspend_blocks_for_latest_gc_round();

            committed_sub_dags.extend(subdags);

            fail_point!("consensus-after-handle-commit");
        }

        // Sanity check: for commits that have been linearized using the certified commits, ensure that the same sub dag has been committed.
        for sub_dag in &committed_sub_dags {
            if let Some(commit_ref) = certified_commits_map.remove(&sub_dag.commit_ref.index) {
                assert_eq!(
                    commit_ref, sub_dag.commit_ref,
                    "Certified commit has different reference than the committed sub dag"
                );
            }
        }

        // Notify about our own committed blocks
        let committed_block_refs = committed_sub_dags
            .iter()
            .flat_map(|sub_dag| sub_dag.blocks.iter())
            .filter_map(|block| {
                (block.author() == self.context.own_index).then_some(block.reference())
            })
            .collect::<Vec<_>>();
        self.transaction_consumer
            .notify_own_blocks_status(committed_block_refs, self.dag_state.read().gc_round());

        Ok(committed_sub_dags)
    }

    pub(crate) fn get_missing_blocks(&self) -> BTreeSet<BlockRef> {
        let _scope = monitored_scope("Core::get_missing_blocks");
        self.block_manager.missing_blocks()
    }

    /// Sets if there is consumer available to consume blocks produced by the core.
    pub(crate) fn set_subscriber_exists(&mut self, exists: bool) {
        info!("Block subscriber exists: {exists}");
        self.subscriber_exists = exists;
    }

    /// Sets the delay by round for propagating blocks to a quorum.
    pub(crate) fn set_propagation_delay(&mut self, delay: Round) {
        info!("Propagation round delay set to: {delay}");
        self.propagation_delay = delay;
    }

    /// Sets the min propose round for the proposer allowing to propose blocks only for round numbers
    /// `> last_known_proposed_round`. At the moment is allowed to call the method only once leading to a panic
    /// if attempt to do multiple times.
    pub(crate) fn set_last_known_proposed_round(&mut self, round: Round) {
        if self.last_known_proposed_round.is_some() {
            panic!("Should not attempt to set the last known proposed round if that has been already set");
        }
        self.last_known_proposed_round = Some(round);
        info!("Last known proposed round set to {round}");
    }

    /// Whether the core should propose new blocks.
    pub(crate) fn should_propose(&self) -> bool {
        let clock_round = self.dag_state.read().threshold_clock_round();
        let core_skipped_proposals = &self.context.metrics.node_metrics.core_skipped_proposals;

        if !self.subscriber_exists {
            debug!("Skip proposing for round {clock_round}, no subscriber exists.");
            core_skipped_proposals
                .with_label_values(&["no_subscriber"])
                .inc();
            return false;
        }

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
            debug!("Skip proposing for round {clock_round}, last known proposed round has not been synced yet.");
            core_skipped_proposals
                .with_label_values(&["no_last_known_proposed_round"])
                .inc();
            return false;
        };
        if clock_round <= last_known_proposed_round {
            debug!("Skip proposing for round {clock_round} as last known proposed round is {last_known_proposed_round}");
            core_skipped_proposals
                .with_label_values(&["higher_last_known_proposed_round"])
                .inc();
            return false;
        }

        true
    }

    // Try to decide which of the certified commits will have to be committed next respecting the `limit`.
    // If provided `limit` is zero, it will panic.
    // The function returns a list of commit leaders and certified commits. If empty vector is returned, it means that
    // there are no certified commits to be committed as `certified_commits` is either empty or all of the certified
    // commits are already committed.
    #[tracing::instrument(skip_all)]
    fn try_decide_certified(
        &mut self,
        certified_commits: &mut Vec<CertifiedCommit>,
        limit: usize,
    ) -> Vec<(DecidedLeader, CertifiedCommit)> {
        // If GC is disabled then should not run any of this logic.
        if !self.dag_state.read().gc_enabled() {
            return Vec::new();
        }

        assert!(limit > 0, "limit should be greater than 0");

        let to_commit = if certified_commits.len() >= limit {
            // We keep only the number of leaders as dictated by the `limit`
            certified_commits.drain(..limit).collect::<Vec<_>>()
        } else {
            // Otherwise just take all of them and leave the `synced_commits` empty.
            std::mem::take(certified_commits)
        };

        tracing::debug!(
            "Decided {} certified leaders: {}",
            to_commit.len(),
            to_commit.iter().map(|c| c.leader().to_string()).join(",")
        );

        let sequenced_leaders = to_commit
            .into_iter()
            .map(|commit| {
                let leader = commit.blocks().last().expect("Certified commit should have at least one block");
                assert_eq!(leader.reference(), commit.leader(), "Last block of the committed sub dag should have the same digest as the leader of the commit");
                let leader = DecidedLeader::Commit(leader.clone(), /* direct */ false);
                UniversalCommitter::update_metrics(&self.context, &leader, Decision::Certified);
                (leader, commit)
            })
            .collect::<Vec<_>>();

        sequenced_leaders
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
            .with_label_values(&["Core::smart_ancestors_to_propose"])
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
        let included_ancestors = iter::once(self.last_proposed_block().clone())
            .chain(
                all_ancestors
                    .into_iter()
                    .flat_map(|(ancestor, equivocating_ancestors)| {
                        if ancestor.author() == self.context.own_index {
                            return None;
                        }
                        if let Some(last_block_ref) =
                            self.last_included_ancestors[ancestor.author()]
                        {
                            if last_block_ref.round >= ancestor.round() {
                                return None;
                            }
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
            debug!("Only found {} stake of good ancestors to include for round {clock_round}, will wait for more.", parent_round_quorum.stake());
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
                debug!("Including temporarily excluded parent round ancestor {ancestor} with score {score} to propose for round {clock_round}");
                parent_round_quorum.add(ancestor.author(), &self.context.committee);
                ancestors_to_propose.push(ancestor);
                node_metrics
                    .included_excluded_proposal_ancestors_count_by_authority
                    .with_label_values(&[block_hostname, "timeout"])
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
                trace!("Excluded low score ancestor {} with score {score} to propose for round {clock_round}: last included round {last_included_round} >= accepted low quorum round {accepted_low_quorum_round}", ancestor.reference());
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
                trace!("Excluded low score ancestor {} with score {score} to propose for round {clock_round}: ancestor round {} > accepted low quorum round {accepted_low_quorum_round} ", ancestor.reference(), ancestor.round());
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
            trace!("Included low scoring ancestor {} with score {score} seen at accepted low quorum round {accepted_low_quorum_round} to propose for round {clock_round}", ancestor.reference());
            node_metrics
                .included_excluded_proposal_ancestors_count_by_authority
                .with_label_values(&[block_hostname, "quorum"])
                .inc();
        }

        assert!(parent_round_quorum.reached_threshold(&self.context.committee), "Fatal error, quorum not reached for parent round when proposing for round {clock_round}. Possible mismatch between DagState and Core.");

        debug!(
            "Included {} ancestors & excluded {} low performing or equivocating ancestors for proposal in round {clock_round}",
            ancestors_to_propose.len(),
            excluded_and_equivocating_ancestors.len()
        );

        (ancestors_to_propose, excluded_and_equivocating_ancestors)
    }

    /// Checks whether all the leaders of the round exist.
    /// TODO: we can leverage some additional signal here in order to more cleverly manipulate later the leader timeout
    /// Ex if we already have one leader - the first in order - we might don't want to wait as much.
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

    /// Returns the leaders of the provided round.
    fn leaders(&self, round: Round) -> Vec<Slot> {
        self.committer
            .get_leaders(round)
            .into_iter()
            .map(|authority_index| Slot::new(round, authority_index))
            .collect()
    }

    /// Returns the 1st leader of the round.
    fn first_leader(&self, round: Round) -> AuthorityIndex {
        self.leaders(round).first().unwrap().authority
    }

    fn last_proposed_timestamp_ms(&self) -> BlockTimestampMs {
        self.last_proposed_block().timestamp_ms()
    }

    fn last_proposed_round(&self) -> Round {
        self.last_proposed_block().round()
    }

    fn last_proposed_block(&self) -> VerifiedBlock {
        self.dag_state.read().get_last_proposed_block()
    }
}

/// Senders of signals from Core, for outputs and events (ex new block produced).
pub(crate) struct CoreSignals {
    tx_block_broadcast: broadcast::Sender<ExtendedBlock>,
    new_round_sender: watch::Sender<Round>,
    context: Arc<Context>,
}

impl CoreSignals {
    pub fn new(context: Arc<Context>) -> (Self, CoreSignalsReceivers) {
        // Blocks buffered in broadcast channel should be roughly equal to thosed cached in dag state,
        // since the underlying blocks are ref counted so a lower buffer here will not reduce memory
        // usage significantly.
        let (tx_block_broadcast, rx_block_broadcast) = broadcast::channel::<ExtendedBlock>(
            context.parameters.dag_state_cached_rounds as usize,
        );
        let (new_round_sender, new_round_receiver) = watch::channel(0);

        let me = Self {
            tx_block_broadcast,
            new_round_sender,
            context,
        };

        let receivers = CoreSignalsReceivers {
            rx_block_broadcast,
            new_round_receiver,
        };

        (me, receivers)
    }

    /// Sends a signal to all the waiters that a new block has been produced. The method will return
    /// true if block has reached even one subscriber, false otherwise.
    pub(crate) fn new_block(&self, extended_block: ExtendedBlock) -> ConsensusResult<()> {
        // When there is only one authority in committee, it is unnecessary to broadcast
        // the block which will fail anyway without subscribers to the signal.
        if self.context.committee.size() > 1 {
            if extended_block.block.round() == GENESIS_ROUND {
                debug!("Ignoring broadcasting genesis block to peers");
                return Ok(());
            }

            if let Err(err) = self.tx_block_broadcast.send(extended_block) {
                warn!("Couldn't broadcast the block to any receiver: {err}");
                return Err(ConsensusError::Shutdown);
            }
        } else {
            debug!(
                "Did not broadcast block {extended_block:?} to receivers as committee size is <= 1"
            );
        }
        Ok(())
    }

    /// Sends a signal that threshold clock has advanced to new round. The `round_number` is the round at which the
    /// threshold clock has advanced to.
    pub(crate) fn new_round(&mut self, round_number: Round) {
        let _ = self.new_round_sender.send_replace(round_number);
    }
}

/// Receivers of signals from Core.
/// Intentionally un-clonable. Comonents should only subscribe to channels they need.
pub(crate) struct CoreSignalsReceivers {
    rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
    new_round_receiver: watch::Receiver<Round>,
}

impl CoreSignalsReceivers {
    pub(crate) fn block_broadcast_receiver(&self) -> broadcast::Receiver<ExtendedBlock> {
        self.rx_block_broadcast.resubscribe()
    }

    pub(crate) fn new_round_receiver(&self) -> watch::Receiver<Round> {
        self.new_round_receiver.clone()
    }
}

/// Creates cores for the specified number of authorities for their corresponding stakes. The method returns the
/// cores and their respective signal receivers are returned in `AuthorityIndex` order asc.
#[cfg(test)]
pub(crate) fn create_cores(context: Context, authorities: Vec<Stake>) -> Vec<CoreTextFixture> {
    let mut cores = Vec::new();

    for index in 0..authorities.len() {
        let own_index = AuthorityIndex::new_for_test(index as u32);
        let core = CoreTextFixture::new(context.clone(), authorities.clone(), own_index, false);
        cores.push(core);
    }
    cores
}

#[cfg(test)]
pub(crate) struct CoreTextFixture {
    pub(crate) core: Core,
    pub(crate) transaction_certifier: TransactionCertifier,
    pub(crate) signal_receivers: CoreSignalsReceivers,
    pub(crate) block_receiver: broadcast::Receiver<ExtendedBlock>,
    pub(crate) _commit_output_receiver: UnboundedReceiver<CommittedSubDag>,
    pub(crate) _blocks_output_receiver: UnboundedReceiver<CertifiedBlocksOutput>,
    pub(crate) dag_state: Arc<RwLock<DagState>>,
    pub(crate) store: Arc<MemStore>,
}

#[cfg(test)]
impl CoreTextFixture {
    fn new(
        context: Context,
        authorities: Vec<Stake>,
        own_index: AuthorityIndex,
        sync_last_known_own_block: bool,
    ) -> Self {
        let (committee, mut signers) = local_committee_and_keys(0, authorities.clone());
        let mut context = context.clone();
        context = context
            .with_committee(committee)
            .with_authority_index(own_index);
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);

        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(
            LeaderSchedule::from_store(context.clone(), dag_state.clone())
                .with_num_commits_per_schedule(10),
        );
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (blocks_sender, _blocks_receiver) =
            mysten_metrics::monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        transaction_certifier.recover(&NoopBlockVerifier);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, commit_output_receiver, blocks_output_receiver) =
            CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let block_signer = signers.remove(own_index.value()).1;

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let core = Core::new(
            context,
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            block_signer,
            dag_state.clone(),
            sync_last_known_own_block,
            round_tracker,
        );

        Self {
            core,
            transaction_certifier,
            signal_receivers,
            block_receiver,
            _commit_output_receiver: commit_output_receiver,
            _blocks_output_receiver: blocks_output_receiver,
            dag_state,
            store,
        }
    }

    pub(crate) fn add_blocks(
        &mut self,
        blocks: Vec<VerifiedBlock>,
    ) -> ConsensusResult<BTreeSet<BlockRef>> {
        self.transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        self.core.add_blocks(blocks)
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeSet, time::Duration};

    use consensus_config::{AuthorityIndex, Parameters};
    use futures::{stream::FuturesUnordered, StreamExt};
    use mysten_metrics::monitored_mpsc;
    use rstest::rstest;
    use sui_protocol_config::ProtocolConfig;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        block::{genesis_blocks, TestBlock},
        block_verifier::NoopBlockVerifier,
        commit::CommitAPI,
        leader_scoring::ReputationScores,
        storage::{mem_store::MemStore, Store, WriteBatch},
        test_dag_builder::DagBuilder,
        test_dag_parser::parse_dag,
        transaction::{BlockStatus, TransactionClient},
        CommitConsumer, CommitIndex, TransactionIndex,
    };

    /// Recover Core and continue proposing from the last round which forms a quorum.
    #[tokio::test]
    async fn test_core_recover_from_store_for_full_round() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let mut block_status_subscriptions = FuturesUnordered::new();

        // Create test blocks for all the authorities for 4 rounds and populate them in store
        let mut last_round_blocks = genesis_blocks(&context);
        let mut all_blocks: Vec<VerifiedBlock> = last_round_blocks.clone();
        for round in 1..=4 {
            let mut this_round_blocks = Vec::new();
            for (index, _authority) in context.committee.authorities() {
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, index.value() as u32)
                        .set_ancestors(last_round_blocks.iter().map(|b| b.reference()).collect())
                        .build(),
                );

                // If it's round 1, that one will be committed later on, and it's our "own" block, then subscribe to listen for the block status.
                if round == 1 && index == context.own_index {
                    let subscription =
                        transaction_consumer.subscribe_for_block_status_testing(block.reference());
                    block_status_subscriptions.push(subscription);
                }

                this_round_blocks.push(block);
            }
            all_blocks.extend(this_round_blocks.clone());
            last_round_blocks = this_round_blocks;
        }
        // write them in store
        store
            .write(WriteBatch::default().blocks(all_blocks))
            .expect("Storage error");

        // create dag state after all blocks have been written to store
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state or store.
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        transaction_certifier.recover(&NoopBlockVerifier);
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let _core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker,
        );

        // New round should be 5
        let mut new_round = signal_receivers.new_round_receiver();
        assert_eq!(*new_round.borrow_and_update(), 5);

        // Block for round 5 should have been proposed.
        let proposed_block = block_receiver
            .recv()
            .await
            .expect("A block should have been created");
        assert_eq!(proposed_block.block.round(), 5);
        let ancestors = proposed_block.block.ancestors();

        // Only ancestors of round 4 should be included.
        assert_eq!(ancestors.len(), 4);
        for ancestor in ancestors {
            assert_eq!(ancestor.round, 4);
        }

        let last_commit = store
            .read_last_commit()
            .unwrap()
            .expect("last commit should be set");

        // There were no commits prior to the core starting up but there was completed
        // rounds up to and including round 4. So we should commit leaders in round 1 & 2
        // as soon as the new block for round 5 is proposed.
        assert_eq!(last_commit.index(), 2);
        assert_eq!(dag_state.read().last_commit_index(), 2);
        let all_stored_commits = store.scan_commits((0..=CommitIndex::MAX).into()).unwrap();
        assert_eq!(all_stored_commits.len(), 2);

        // And ensure that our "own" block 1 sent to TransactionConsumer as notification alongside with gc_round
        while let Some(result) = block_status_subscriptions.next().await {
            let status = result.unwrap();
            assert!(matches!(status, BlockStatus::Sequenced(_)));
        }
    }

    /// Recover Core and continue proposing when having a partial last round which doesn't form a quorum and we haven't
    /// proposed for that round yet.
    #[tokio::test]
    async fn test_core_recover_from_store_for_partial_round() {
        telemetry_subscribers::init_for_testing();

        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // Create test blocks for all authorities except our's (index = 0).
        let mut last_round_blocks = genesis_blocks(&context);
        let mut all_blocks = last_round_blocks.clone();
        for round in 1..=4 {
            let mut this_round_blocks = Vec::new();

            // For round 4 only produce f+1 blocks. Skip our validator 0 and that of position 1 from creating blocks.
            let authorities_to_skip = if round == 4 {
                context.committee.validity_threshold() as usize
            } else {
                // otherwise always skip creating a block for our authority
                1
            };

            for (index, _authority) in context.committee.authorities().skip(authorities_to_skip) {
                let block = TestBlock::new(round, index.value() as u32)
                    .set_ancestors(last_round_blocks.iter().map(|b| b.reference()).collect())
                    .build();
                this_round_blocks.push(VerifiedBlock::new_for_test(block));
            }
            all_blocks.extend(this_round_blocks.clone());
            last_round_blocks = this_round_blocks;
        }

        // write them in store
        store
            .write(WriteBatch::default().blocks(all_blocks))
            .expect("Storage error");

        // create dag state after all blocks have been written to store
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state & store
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        transaction_certifier.recover(&NoopBlockVerifier);
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker,
        );

        // Clock round should have advanced to 5 during recovery because
        // a quorum has formed in round 4.
        let mut new_round = signal_receivers.new_round_receiver();
        assert_eq!(*new_round.borrow_and_update(), 5);

        // During recovery, round 4 block should have been proposed.
        let proposed_block = block_receiver
            .recv()
            .await
            .expect("A block should have been created");
        assert_eq!(proposed_block.block.round(), 4);
        let ancestors = proposed_block.block.ancestors();

        assert_eq!(ancestors.len(), 4);
        for ancestor in ancestors {
            if ancestor.author == context.own_index {
                assert_eq!(ancestor.round, 0);
            } else {
                assert_eq!(ancestor.round, 3);
            }
        }

        // Run commit rule.
        core.try_commit(vec![]).ok();
        let last_commit = store
            .read_last_commit()
            .unwrap()
            .expect("last commit should be set");

        // There were no commits prior to the core starting up but there was completed
        // rounds up to round 4. So we should commit leaders in round 1 & 2 as soon
        // as the new block for round 4 is proposed.
        assert_eq!(last_commit.index(), 2);
        assert_eq!(dag_state.read().last_commit_index(), 2);
        let all_stored_commits = store.scan_commits((0..=CommitIndex::MAX).into()).unwrap();
        assert_eq!(all_stored_commits.len(), 2);
    }

    #[tokio::test]
    async fn test_core_propose_after_genesis() {
        telemetry_subscribers::init_for_testing();
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(2_000);
            config.set_consensus_max_transactions_in_block_bytes_for_testing(2_000);
            config
        });

        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let (transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker,
        );

        // Send some transactions
        let mut total = 0;
        let mut index = 0;
        loop {
            let transaction =
                bcs::to_bytes(&format!("Transaction {index}")).expect("Shouldn't fail");
            total += transaction.len();
            index += 1;
            let _w = transaction_client
                .submit_no_wait(vec![transaction])
                .await
                .unwrap();

            // Create total size of transactions up to 1KB
            if total >= 1_000 {
                break;
            }
        }

        // a new block should have been created during recovery.
        let extended_block = block_receiver
            .recv()
            .await
            .expect("A new block should have been created");

        // A new block created - assert the details
        assert_eq!(extended_block.block.round(), 1);
        assert_eq!(extended_block.block.author().value(), 0);
        assert_eq!(extended_block.block.ancestors().len(), 4);

        let mut total = 0;
        for (i, transaction) in extended_block.block.transactions().iter().enumerate() {
            total += transaction.data().len() as u64;
            let transaction: String = bcs::from_bytes(transaction.data()).unwrap();
            assert_eq!(format!("Transaction {i}"), transaction);
        }
        assert!(total <= context.protocol_config.max_transactions_in_block_bytes());

        // genesis blocks should be referenced
        let all_genesis = genesis_blocks(&context);

        for ancestor in extended_block.block.ancestors() {
            all_genesis
                .iter()
                .find(|block| block.reference() == *ancestor)
                .expect("Block should be found amongst genesis blocks");
        }

        // Try to propose again - with or without ignore leaders check, it will not return any block
        assert!(core.try_propose(false).unwrap().is_none());
        assert!(core.try_propose(true).unwrap().is_none());

        // Check no commits have been persisted to dag_state & store
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);
    }

    #[tokio::test]
    async fn test_core_propose_once_receiving_a_quorum() {
        telemetry_subscribers::init_for_testing();
        let (context, _key_pairs) = Context::new_for_test(4);
        let mut core_fixture = CoreTextFixture::new(
            context.clone(),
            vec![1, 1, 1, 1],
            AuthorityIndex::new_for_test(0),
            false,
        );
        let transaction_certifier = &core_fixture.transaction_certifier;
        let store = &core_fixture.store;
        let dag_state = &core_fixture.dag_state;
        let core = &mut core_fixture.core;

        let mut expected_ancestors = BTreeSet::new();

        // Adding one block now will trigger the creation of new block for round 1
        let block_1 = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        expected_ancestors.insert(block_1.reference());
        // Wait for min round delay to allow blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // add blocks to trigger proposal.
        transaction_certifier.add_voted_blocks(vec![(block_1.clone(), vec![])]);
        _ = core.add_blocks(vec![block_1]);

        assert_eq!(core.last_proposed_round(), 1);
        expected_ancestors.insert(core.last_proposed_block().reference());
        // attempt to create a block - none will be produced.
        assert!(core.try_propose(false).unwrap().is_none());

        // Adding another block now forms a quorum for round 1, so block at round 2 will proposed
        let block_2 = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        expected_ancestors.insert(block_2.reference());
        // Wait for min round delay to allow blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // add blocks to trigger proposal.
        transaction_certifier.add_voted_blocks(vec![(block_2.clone(), vec![1, 4])]);
        _ = core.add_blocks(vec![block_2.clone()]);

        assert_eq!(core.last_proposed_round(), 2);

        let proposed_block = core.last_proposed_block();
        assert_eq!(proposed_block.round(), 2);
        assert_eq!(proposed_block.author(), context.own_index);
        assert_eq!(proposed_block.ancestors().len(), 3);
        let ancestors = proposed_block.ancestors();
        let ancestors = ancestors.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(ancestors, expected_ancestors);

        let transaction_votes = proposed_block.transaction_votes();
        assert_eq!(transaction_votes.len(), 1);
        let transaction_vote = transaction_votes.first().unwrap();
        assert_eq!(transaction_vote.block_ref, block_2.reference());
        assert_eq!(transaction_vote.rejects, vec![1, 4]);

        // Check no commits have been persisted to dag_state & store
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_commit_and_notify_for_block_status(#[values(0, 2)] gc_depth: u32) {
        telemetry_subscribers::init_for_testing();
        let (mut context, mut key_pairs) = Context::new_for_test(4);

        if gc_depth > 0 {
            context
                .protocol_config
                .set_consensus_gc_depth_for_testing(gc_depth);
        }

        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let mut block_status_subscriptions = FuturesUnordered::new();

        let dag_str = "DAG {
            Round 0 : { 4 },
            Round 1 : { * },
            Round 2 : { * },
            Round 3 : {
                A -> [*],
                B -> [-A2],
                C -> [-A2],
                D -> [-A2],
            },
            Round 4 : { 
                B -> [-A3],
                C -> [-A3],
                D -> [-A3],
            },
            Round 5 : { 
                A -> [A3, B4, C4, D4]
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 6 : { * },
            Round 7 : { * },
            Round 8 : { * },
        }";

        let (_, dag_builder) = parse_dag(dag_str).expect("Invalid dag");
        dag_builder.print();

        // Subscribe to all created "own" blocks. We know that for our node (A) we'll be able to commit up to round 5.
        for block in dag_builder.blocks(1..=5) {
            if block.author() == context.own_index {
                let subscription =
                    transaction_consumer.subscribe_for_block_status_testing(block.reference());
                block_status_subscriptions.push(subscription);
            }
        }

        // write them in store
        store
            .write(WriteBatch::default().blocks(dag_builder.blocks(1..=8)))
            .expect("Storage error");

        // create dag state after all blocks have been written to store
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state or store.
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        transaction_certifier.recover(&NoopBlockVerifier);
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();
        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let _core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker,
        );

        let last_commit = store
            .read_last_commit()
            .unwrap()
            .expect("last commit should be set");

        assert_eq!(last_commit.index(), 5);

        while let Some(result) = block_status_subscriptions.next().await {
            let status = result.unwrap();

            // If gc is enabled, then we expect some blocks to be garbage collected.
            if gc_depth > 0 {
                match status {
                    BlockStatus::Sequenced(block_ref) => {
                        assert!(block_ref.round == 1 || block_ref.round == 5);
                    }
                    BlockStatus::GarbageCollected(block_ref) => {
                        assert!(block_ref.round == 2 || block_ref.round == 3);
                    }
                }
            } else {
                // otherwise all of them should be committed
                assert!(matches!(status, BlockStatus::Sequenced(_)));
            }
        }
    }

    // Tests that the threshold clock advances when blocks get unsuspended due to GC'ed blocks and newly created blocks are always higher
    // than the last advanced gc round.
    #[tokio::test]
    async fn test_multiple_commits_advance_threshold_clock() {
        telemetry_subscribers::init_for_testing();
        let (mut context, mut key_pairs) = Context::new_for_test(4);
        const GC_DEPTH: u32 = 2;

        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(GC_DEPTH);

        let context = Arc::new(context);

        let store = Arc::new(MemStore::new());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // On round 1 we do produce the block for authority D but we do not link it until round 6. This is making round 6 unable to get processed
        // until leader of round 3 is committed where round 1 gets garbage collected.
        // Then we add more rounds so we can trigger a commit for leader of round 9 which will move the gc round to 7.
        let dag_str = "DAG {
            Round 0 : { 4 },
            Round 1 : { * },
            Round 2 : { 
                B -> [-D1],
                C -> [-D1],
                D -> [-D1],
            },
            Round 3 : {
                B -> [*],
                C -> [*]
                D -> [*],
            },
            Round 4 : { 
                A -> [*],
                B -> [*],
                C -> [*]
                D -> [*],
            },
            Round 5 : { 
                A -> [*],
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 6 : { 
                B -> [A5, B5, C5, D1],
                C -> [A5, B5, C5, D1],
                D -> [A5, B5, C5, D1],
            },
            Round 7 : { 
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 8 : { 
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 9 : { 
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 10 : { 
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 11 : { 
                B -> [*],
                C -> [*],
                D -> [*],
            },
        }";

        let (_, dag_builder) = parse_dag(dag_str).expect("Invalid dag");
        dag_builder.print();

        // create dag state after all blocks have been written to store
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state or store.
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();
        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            true,
            round_tracker,
        );
        // We set the last known round to 4 so we avoid creating new blocks until then - otherwise it will crash as the already created DAG contains blocks for this
        // authority.
        core.set_last_known_proposed_round(4);

        // We add all the blocks except D1. The only ones we can immediately accept are the ones up to round 5 as they don't have a dependency on D1. Rest of blocks do have causal dependency
        // to D1 so they can't be processed until the leader of round 3 can get committed and gc round moves to 1. That will make all the blocks that depend to D1 get accepted.
        // However, our threshold clock is now at round 6 as the last quorum that we managed to process was the round 5.
        // As commits happen blocks of later rounds get accepted and more leaders get committed. Eventually the leader of round 9 gets committed and gc is moved to 9 - 2 = 7.
        // If our node attempts to produce a block for the threshold clock 6, that will make the acceptance checks fail as now gc has moved far past this round.
        let mut all_blocks = dag_builder.blocks(1..=11);
        all_blocks.sort_by_key(|b| b.round());
        let voted_blocks: Vec<(VerifiedBlock, Vec<TransactionIndex>)> =
            all_blocks.iter().map(|b| (b.clone(), vec![])).collect();
        transaction_certifier.add_voted_blocks(voted_blocks);
        let blocks: Vec<VerifiedBlock> = all_blocks
            .into_iter()
            .filter(|b| !(b.round() == 1 && b.author() == AuthorityIndex::new_for_test(3)))
            .collect();
        core.add_blocks(blocks).expect("Should not fail");

        assert_eq!(core.last_proposed_round(), 12);
    }

    #[tokio::test]
    async fn test_core_set_min_propose_round() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        }));

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            true,
            round_tracker,
        );

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // Trying to explicitly propose a block will not produce anything
        assert!(core.try_propose(true).unwrap().is_none());

        // Create blocks for the whole network - even "our" node in order to replicate an "amnesia" recovery.
        let mut builder = DagBuilder::new(context.clone());
        builder.layers(1..=10).build();

        let blocks = builder.blocks.values().cloned().collect::<Vec<_>>();

        // Process all the blocks
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());

        core.round_tracker.write().update_from_probe(
            vec![
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
            ],
            vec![
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
                vec![10, 10, 10, 10],
            ],
        );

        // Try to propose - no block should be produced.
        assert!(core.try_propose(true).unwrap().is_none());

        // Now set the last known proposed round which is the highest round for which the network informed
        // us that we do have proposed a block about.
        core.set_last_known_proposed_round(10);

        let block = core.try_propose(true).expect("No error").unwrap();
        assert_eq!(block.round(), 11);
        assert_eq!(block.ancestors().len(), 4);

        let our_ancestor_included = block.ancestors()[0];
        assert_eq!(our_ancestor_included.author, context.own_index);
        assert_eq!(our_ancestor_included.round, 10);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_core_try_new_block_leader_timeout() {
        telemetry_subscribers::init_for_testing();

        // Since we run the test with started_paused = true, any time-dependent operations using Tokio's time
        // facilities, such as tokio::time::sleep or tokio::time::Instant, will not advance. So practically each
        // Core's clock will have initialised potentially with different values but it never advances.
        // To ensure that blocks won't get rejected by cores we'll need to manually wait for the time
        // diff before processing them. By calling the `tokio::time::sleep` we implicitly also advance the
        // tokio clock.
        async fn wait_blocks(blocks: &[VerifiedBlock], context: &Context) {
            // Simulate the time wait before processing a block to ensure that block.timestamp <= now
            let now = context.clock.timestamp_utc_ms();
            let max_timestamp = blocks
                .iter()
                .max_by_key(|block| block.timestamp_ms() as BlockTimestampMs)
                .map(|block| block.timestamp_ms())
                .unwrap_or(0);

            let wait_time = Duration::from_millis(max_timestamp.saturating_sub(now));
            sleep(wait_time).await;
        }

        let (context, _) = Context::new_for_test(4);
        // Create the cores for all authorities
        let mut all_cores = create_cores(context, vec![1, 1, 1, 1]);

        // Create blocks for rounds 1..=3 from all Cores except last Core of authority 3, so we miss the block from it. As
        // it will be the leader of round 3 then no-one will be able to progress to round 4 unless we explicitly trigger
        // the block creation.
        // create the cores and their signals for all the authorities
        let (_last_core, cores) = all_cores.split_last_mut().unwrap();

        // Now iterate over a few rounds and ensure the corresponding signals are created while network advances
        let mut last_round_blocks = Vec::<VerifiedBlock>::new();
        for round in 1..=3 {
            let mut this_round_blocks = Vec::new();

            for core_fixture in cores.iter_mut() {
                wait_blocks(&last_round_blocks, &core_fixture.core.context).await;

                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                // Only when round > 1 and using non-genesis parents.
                if let Some(r) = last_round_blocks.first().map(|b| b.round()) {
                    assert_eq!(round - 1, r);
                    if core_fixture.core.last_proposed_round() == r {
                        // Force propose new block regardless of min round delay.
                        core_fixture
                            .core
                            .try_propose(true)
                            .unwrap()
                            .unwrap_or_else(|| {
                                panic!("Block should have been proposed for round {}", round)
                            });
                    }
                }

                assert_eq!(core_fixture.core.last_proposed_round(), round);

                this_round_blocks.push(core_fixture.core.last_proposed_block());
            }

            last_round_blocks = this_round_blocks;
        }

        // Try to create the blocks for round 4 by calling the try_propose() method. No block should be created as the
        // leader - authority 3 - hasn't proposed any block.
        for core_fixture in cores.iter_mut() {
            wait_blocks(&last_round_blocks, &core_fixture.core.context).await;

            core_fixture.add_blocks(last_round_blocks.clone()).unwrap();
            assert!(core_fixture.core.try_propose(false).unwrap().is_none());
        }

        // Now try to create the blocks for round 4 via the leader timeout method which should
        // ignore any leader checks or min round delay.
        for core_fixture in cores.iter_mut() {
            assert!(core_fixture.core.new_block(4, true).unwrap().is_some());
            assert_eq!(core_fixture.core.last_proposed_round(), 4);

            // Check commits have been persisted to store
            let last_commit = core_fixture
                .store
                .read_last_commit()
                .unwrap()
                .expect("last commit should be set");
            // There are 1 leader rounds with rounds completed up to and including
            // round 4
            assert_eq!(last_commit.index(), 1);
            let all_stored_commits = core_fixture
                .store
                .scan_commits((0..=CommitIndex::MAX).into())
                .unwrap();
            assert_eq!(all_stored_commits.len(), 1);
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_core_try_new_block_with_leader_timeout_and_low_scoring_authority() {
        telemetry_subscribers::init_for_testing();

        // Since we run the test with started_paused = true, any time-dependent operations using Tokio's time
        // facilities, such as tokio::time::sleep or tokio::time::Instant, will not advance. So practically each
        // Core's clock will have initialised potentially with different values but it never advances.
        // To ensure that blocks won't get rejected by cores we'll need to manually wait for the time
        // diff before processing them. By calling the `tokio::time::sleep` we implicitly also advance the
        // tokio clock.
        async fn wait_blocks(blocks: &[VerifiedBlock], context: &Context) {
            // Simulate the time wait before processing a block to ensure that block.timestamp <= now
            let now = context.clock.timestamp_utc_ms();
            let max_timestamp = blocks
                .iter()
                .max_by_key(|block| block.timestamp_ms() as BlockTimestampMs)
                .map(|block| block.timestamp_ms())
                .unwrap_or(0);

            let wait_time = Duration::from_millis(max_timestamp.saturating_sub(now));
            sleep(wait_time).await;
        }

        let (mut context, _) = Context::new_for_test(5);
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);

        // Create the cores for all authorities
        let mut all_cores = create_cores(context, vec![1, 1, 1, 1, 1]);
        let (_last_core, cores) = all_cores.split_last_mut().unwrap();

        // Create blocks for rounds 1..=30 from all Cores except last Core of authority 4.
        let mut last_round_blocks = Vec::<VerifiedBlock>::new();
        for round in 1..=30 {
            let mut this_round_blocks = Vec::new();

            for core_fixture in cores.iter_mut() {
                wait_blocks(&last_round_blocks, &core_fixture.core.context).await;

                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![0, 0, 0, 0, 0],
                    ],
                    vec![
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![0, 0, 0, 0, 0],
                    ],
                );

                // Only when round > 1 and using non-genesis parents.
                if let Some(r) = last_round_blocks.first().map(|b| b.round()) {
                    assert_eq!(round - 1, r);
                    if core_fixture.core.last_proposed_round() == r {
                        // Force propose new block regardless of min round delay.
                        core_fixture
                            .core
                            .try_propose(true)
                            .unwrap()
                            .unwrap_or_else(|| {
                                panic!("Block should have been proposed for round {}", round)
                            });
                    }
                }

                assert_eq!(core_fixture.core.last_proposed_round(), round);

                this_round_blocks.push(core_fixture.core.last_proposed_block().clone());
            }

            last_round_blocks = this_round_blocks;
        }

        // Now produce blocks for all Cores
        for round in 31..=40 {
            let mut this_round_blocks = Vec::new();

            for core_fixture in all_cores.iter_mut() {
                wait_blocks(&last_round_blocks, &core_fixture.core.context).await;

                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                // Don't update probed rounds for authority 3 so it will remain
                // excluded
                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![0, 0, 0, 0, 0],
                    ],
                    vec![
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![round, round, round, round, 0],
                        vec![0, 0, 0, 0, 0],
                    ],
                );

                // Only when round > 1 and using non-genesis parents.
                if let Some(r) = last_round_blocks.first().map(|b| b.round()) {
                    assert_eq!(round - 1, r);
                    if core_fixture.core.last_proposed_round() == r {
                        // Force propose new block regardless of min round delay.
                        core_fixture
                            .core
                            .try_propose(true)
                            .unwrap()
                            .unwrap_or_else(|| {
                                panic!("Block should have been proposed for round {}", round)
                            });
                    }
                }

                this_round_blocks.push(core_fixture.core.last_proposed_block().clone());

                for block in this_round_blocks.iter() {
                    if block.author() != AuthorityIndex::new_for_test(4) {
                        // Assert blocks created include only 4 ancestors per block as one
                        // should be excluded
                        assert_eq!(block.ancestors().len(), 4);
                    } else {
                        // Authority 3 is the low scoring authority so it will still include
                        // its own blocks.
                        assert_eq!(block.ancestors().len(), 5);
                    }
                }
            }

            last_round_blocks = this_round_blocks;
        }
    }

    #[tokio::test]
    async fn test_smart_ancestor_selection() {
        telemetry_subscribers::init_for_testing();
        let (mut context, mut key_pairs) = Context::new_for_test(7);
        context
            .protocol_config
            .set_consensus_bad_nodes_stake_threshold_for_testing(33);
        let context = Arc::new(context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        }));

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(
            LeaderSchedule::from_store(context.clone(), dag_state.clone())
                .with_num_commits_per_schedule(10),
        );

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            true,
            round_tracker.clone(),
        );

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // Trying to explicitly propose a block will not produce anything
        assert!(core.try_propose(true).unwrap().is_none());

        // Create blocks for the whole network but not for authority 1
        let mut builder = DagBuilder::new(context.clone());
        builder
            .layers(1..=12)
            .authorities(vec![AuthorityIndex::new_for_test(1)])
            .skip_block()
            .build();
        let blocks = builder.blocks(1..=12);
        // Process all the blocks
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        core.set_last_known_proposed_round(12);

        round_tracker.write().update_from_probe(
            vec![
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![0, 0, 0, 0, 0, 0, 0],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
            ],
            vec![
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![0, 0, 0, 0, 0, 0, 0],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
                vec![12, 12, 12, 12, 12, 12, 12],
            ],
        );

        let block = core.try_propose(true).expect("No error").unwrap();
        assert_eq!(block.round(), 13);
        assert_eq!(block.ancestors().len(), 7);

        // Build blocks for rest of the network other than own index
        builder
            .layers(13..=14)
            .authorities(vec![AuthorityIndex::new_for_test(0)])
            .skip_block()
            .build();
        let blocks = builder.blocks(13..=14);
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());

        // We now have triggered a leader schedule change so we should have
        // one EXCLUDE authority (1) when we go to select ancestors for the next proposal
        let block = core.try_propose(true).expect("No error").unwrap();
        assert_eq!(block.round(), 15);
        assert_eq!(block.ancestors().len(), 6);

        // Build blocks for a quorum of the network including the EXCLUDE authority (1)
        // which will trigger smart select and we will not propose a block
        let round_14_ancestors = builder.last_ancestors.clone();
        builder
            .layer(15)
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(5),
                AuthorityIndex::new_for_test(6),
            ])
            .skip_block()
            .build();
        let blocks = builder.blocks(15..=15);
        let authority_1_excluded_block_reference = blocks
            .iter()
            .find(|block| block.author() == AuthorityIndex::new_for_test(1))
            .unwrap()
            .reference();
        // Wait for min round delay to allow blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // Smart select should be triggered and no block should be proposed.
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        assert_eq!(core.last_proposed_block().round(), 15);

        builder
            .layer(15)
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(1),
                AuthorityIndex::new_for_test(2),
                AuthorityIndex::new_for_test(3),
                AuthorityIndex::new_for_test(4),
            ])
            .skip_block()
            .override_last_ancestors(round_14_ancestors)
            .build();
        let blocks = builder.blocks(15..=15);
        let round_15_ancestors: Vec<BlockRef> = blocks
            .iter()
            .filter(|block| block.round() == 15)
            .map(|block| block.reference())
            .collect();
        let included_block_references = iter::once(&core.last_proposed_block())
            .chain(blocks.iter())
            .filter(|block| block.author() != AuthorityIndex::new_for_test(1))
            .map(|block| block.reference())
            .collect::<Vec<_>>();

        // Have enough ancestor blocks to propose now.
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        assert_eq!(core.last_proposed_block().round(), 16);

        // Check that a new block has been proposed & signaled.
        let extended_block = loop {
            let extended_block =
                tokio::time::timeout(Duration::from_secs(1), block_receiver.recv())
                    .await
                    .unwrap()
                    .unwrap();
            if extended_block.block.round() == 16 {
                break extended_block;
            }
        };
        assert_eq!(extended_block.block.round(), 16);
        assert_eq!(extended_block.block.author(), core.context.own_index);
        assert_eq!(extended_block.block.ancestors().len(), 6);
        assert_eq!(extended_block.block.ancestors(), included_block_references);
        assert_eq!(extended_block.excluded_ancestors.len(), 1);
        assert_eq!(
            extended_block.excluded_ancestors[0],
            authority_1_excluded_block_reference
        );

        // Build blocks for a quorum of the network including the EXCLUDE ancestor
        // which will trigger smart select and we will not propose a block.
        // This time we will force propose by hitting the leader timeout after which
        // should cause us to include this EXCLUDE ancestor.
        builder
            .layer(16)
            .authorities(vec![
                AuthorityIndex::new_for_test(0),
                AuthorityIndex::new_for_test(5),
                AuthorityIndex::new_for_test(6),
            ])
            .skip_block()
            .override_last_ancestors(round_15_ancestors)
            .build();
        let blocks = builder.blocks(16..=16);
        // Wait for leader timeout to force blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // Smart select should be triggered and no block should be proposed.
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        assert_eq!(core.last_proposed_block().round(), 16);

        // Simulate a leader timeout and a force proposal where we will include
        // one EXCLUDE ancestor when we go to select ancestors for the next proposal
        let block = core.try_propose(true).expect("No error").unwrap();
        assert_eq!(block.round(), 17);
        assert_eq!(block.ancestors().len(), 5);

        // Check that a new block has been proposed & signaled.
        let extended_block = tokio::time::timeout(Duration::from_secs(1), block_receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(extended_block.block.round(), 17);
        assert_eq!(extended_block.block.author(), core.context.own_index);
        assert_eq!(extended_block.block.ancestors().len(), 5);
        assert_eq!(extended_block.excluded_ancestors.len(), 0);

        // Excluded authority is locked until round 20, simulate enough rounds to
        // unlock
        builder
            .layers(17..=22)
            .authorities(vec![AuthorityIndex::new_for_test(0)])
            .skip_block()
            .build();
        let blocks = builder.blocks(17..=22);

        // Simulate updating received and accepted rounds from prober.
        // New quorum rounds for authority can then be computed which will unlock
        // the Excluded authority (1) and then we should be able to create a new
        // layer of blocks which will then all be included as ancestors for the
        // next proposal
        round_tracker.write().update_from_probe(
            vec![
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
            ],
            vec![
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
                vec![22, 22, 22, 22, 22, 22, 22],
            ],
        );

        let included_block_references = iter::once(&core.last_proposed_block())
            .chain(blocks.iter())
            .filter(|block| block.round() == 22 || block.author() == core.context.own_index)
            .map(|block| block.reference())
            .collect::<Vec<_>>();

        // Have enough ancestor blocks to propose now.
        sleep(context.parameters.min_round_delay).await;
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        assert_eq!(core.last_proposed_block().round(), 23);

        // Check that a new block has been proposed & signaled.
        let extended_block = tokio::time::timeout(Duration::from_secs(1), block_receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(extended_block.block.round(), 23);
        assert_eq!(extended_block.block.author(), core.context.own_index);
        assert_eq!(extended_block.block.ancestors().len(), 7);
        assert_eq!(extended_block.block.ancestors(), included_block_references);
        assert_eq!(extended_block.excluded_ancestors.len(), 0);
    }

    #[tokio::test]
    async fn test_excluded_ancestor_limit() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        }));

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(
            LeaderSchedule::from_store(context.clone(), dag_state.clone())
                .with_num_commits_per_schedule(10),
        );

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            true,
            round_tracker,
        );

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // Create blocks for the whole network
        let mut builder = DagBuilder::new(context.clone());
        builder.layers(1..=3).build();

        // This will equivocate 9 blocks for authority 1 which will be excluded on
        // the proposal but because of the limits set will be dropped and not included
        // as part of the ExtendedBlock structure sent to the rest of the network
        builder
            .layer(4)
            .authorities(vec![AuthorityIndex::new_for_test(1)])
            .equivocate(9)
            .build();
        let blocks = builder.blocks(1..=4);

        // Process all the blocks
        transaction_certifier
            .add_voted_blocks(blocks.iter().map(|b| (b.clone(), vec![])).collect());
        assert!(core.add_blocks(blocks).unwrap().is_empty());
        core.set_last_known_proposed_round(3);

        let block = core.try_propose(true).expect("No error").unwrap();
        assert_eq!(block.round(), 5);
        assert_eq!(block.ancestors().len(), 4);

        // Check that a new block has been proposed & signaled.
        let extended_block = tokio::time::timeout(Duration::from_secs(1), block_receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(extended_block.block.round(), 5);
        assert_eq!(extended_block.block.author(), core.context.own_index);
        assert_eq!(extended_block.block.ancestors().len(), 4);
        assert_eq!(extended_block.excluded_ancestors.len(), 8);
    }

    #[tokio::test]
    async fn test_core_set_subscriber_exists() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            // Set to no subscriber exists initially.
            false,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker,
        );

        // There is no proposal during recovery because there is no subscriber.
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // There is no proposal even with forced proposing.
        assert!(core.try_propose(true).unwrap().is_none());

        // Let Core know subscriber exists.
        core.set_subscriber_exists(true);

        // Proposing now would succeed.
        assert!(core.try_propose(true).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_core_set_propagation_delay_per_authority() {
        // TODO: create helper to avoid the duplicated code here.
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            // Set to no subscriber exists initially.
            false,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
            round_tracker.clone(),
        );

        // There is no proposal during recovery because there is no subscriber.
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // Use a large propagation delay to disable proposing.
        // This is done by accepting an own block at round 1000 to dag state and
        // then simulating updating round tracker received rounds from probe where
        // low quorum round for own index should get calculated to round 0.
        let test_block = VerifiedBlock::new_for_test(TestBlock::new(1000, 0).build());
        transaction_certifier.add_voted_blocks(vec![(test_block.clone(), vec![])]);
        // Force accepting the block to dag state because its causal history is incomplete.
        dag_state.write().accept_block(test_block);

        round_tracker.write().update_from_probe(
            vec![
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
            ],
            vec![
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
                vec![0, 0, 0, 0],
            ],
        );

        // Make propagation delay the only reason for not proposing.
        core.set_subscriber_exists(true);

        // There is no proposal even with forced proposing.
        assert!(core.try_propose(true).unwrap().is_none());

        // Let Core know there is no propagation delay.
        // This is done by simulating updating round tracker recieved rounds from probe
        // where low quorum round for own index should get calculated to round 1000.
        round_tracker.write().update_from_probe(
            vec![
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
            ],
            vec![
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
                vec![1000, 1000, 1000, 1000],
            ],
        );

        // Also add the necessary blocks from round 1000 so core will propose for
        // round 1001
        for author in 1..4 {
            let block = VerifiedBlock::new_for_test(TestBlock::new(1000, author).build());
            transaction_certifier.add_voted_blocks(vec![(block.clone(), vec![])]);
            // Force accepting the block to dag state because its causal history is incomplete.
            dag_state.write().accept_block(block);
        }

        // Proposing now would succeed.
        assert!(core.try_propose(true).unwrap().is_some());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_leader_schedule_change() {
        telemetry_subscribers::init_for_testing();
        let default_params = Parameters::default();

        let (context, _) = Context::new_for_test(4);
        // create the cores and their signals for all the authorities
        let mut cores = create_cores(context, vec![1, 1, 1, 1]);

        // Now iterate over a few rounds and ensure the corresponding signals are created while network advances
        let mut last_round_blocks = Vec::new();
        for round in 1..=30 {
            let mut this_round_blocks = Vec::new();

            // Wait for min round delay to allow blocks to be proposed.
            sleep(default_params.min_round_delay).await;

            for core_fixture in &mut cores {
                // add the blocks from last round
                // this will trigger a block creation for the round and a signal should be emitted
                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                );

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let extended_block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(extended_block.block.round(), round);
                assert_eq!(
                    extended_block.block.author(),
                    core_fixture.core.context.own_index
                );

                // append the new block to this round blocks
                this_round_blocks.push(core_fixture.core.last_proposed_block().clone());

                let block = core_fixture.core.last_proposed_block();

                // ensure that produced block is referring to the blocks of last_round
                assert_eq!(
                    block.ancestors().len(),
                    core_fixture.core.context.committee.size()
                );
                for ancestor in block.ancestors() {
                    if block.round() > 1 {
                        // don't bother with round 1 block which just contains the genesis blocks.
                        assert!(
                            last_round_blocks
                                .iter()
                                .any(|block| block.reference() == *ancestor),
                            "Reference from previous round should be added"
                        );
                    }
                }
            }

            last_round_blocks = this_round_blocks;
        }

        for core_fixture in cores {
            // Check commits have been persisted to store
            let last_commit = core_fixture
                .store
                .read_last_commit()
                .unwrap()
                .expect("last commit should be set");
            // There are 28 leader rounds with rounds completed up to and including
            // round 29. Round 30 blocks will only include their own blocks, so the
            // 28th leader will not be committed.
            assert_eq!(last_commit.index(), 27);
            let all_stored_commits = core_fixture
                .store
                .scan_commits((0..=CommitIndex::MAX).into())
                .unwrap();
            assert_eq!(all_stored_commits.len(), 27);
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .bad_nodes
                    .len(),
                1
            );
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .good_nodes
                    .len(),
                1
            );
            let expected_reputation_scores =
                ReputationScores::new((11..=20).into(), vec![29, 29, 29, 29]);
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .reputation_scores,
                expected_reputation_scores
            );
        }
    }

    #[tokio::test]
    async fn test_filter_new_commits() {
        telemetry_subscribers::init_for_testing();

        let (context, _key_pairs) = Context::new_for_test(4);
        let context = context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        });

        let authority_index = AuthorityIndex::new_for_test(0);
        let core = CoreTextFixture::new(context, vec![1, 1, 1, 1], authority_index, true);
        let mut core = core.core;

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // create a DAG of 12 rounds
        let mut dag_builder = DagBuilder::new(core.context.clone());
        dag_builder.layers(1..=12).build();

        // Store all blocks up to round 6 which should be enough to decide up to leader 4
        dag_builder.print();
        let blocks = dag_builder.blocks(1..=6);

        for block in blocks {
            core.dag_state.write().accept_block(block);
        }

        // Get all the committed sub dags up to round 10
        let sub_dags_and_commits = dag_builder.get_sub_dag_and_certified_commits(1..=10);

        // Now try to commit up to the latest leader (round = 4). Do not provide any certified commits.
        let committed_sub_dags = core.try_commit(vec![]).unwrap();

        // We should have committed up to round 4
        assert_eq!(committed_sub_dags.len(), 4);

        // Now validate the certified commits. We'll try 3 different scenarios:
        println!("Case 1. Provide certified commits that are all before the last committed round.");

        // Highest certified commit should be for leader of round 4.
        let certified_commits = sub_dags_and_commits
            .iter()
            .take(4)
            .map(|(_, c)| c)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            certified_commits.last().unwrap().index()
                <= committed_sub_dags.last().unwrap().commit_ref.index,
            "Highest certified commit should older than the highest committed index."
        );

        let certified_commits = core.filter_new_commits(certified_commits).unwrap();

        // No commits should be processed
        assert!(certified_commits.is_empty());

        println!("Case 2. Provide certified commits that are all after the last committed round.");

        // Highest certified commit should be for leader of round 4.
        let certified_commits = sub_dags_and_commits
            .iter()
            .take(5)
            .map(|(_, c)| c.clone())
            .collect::<Vec<_>>();

        let certified_commits = core.filter_new_commits(certified_commits.clone()).unwrap();

        // The certified commit of index 5 should be processed.
        assert_eq!(certified_commits.len(), 1);
        assert_eq!(certified_commits.first().unwrap().reference().index, 5);

        println!("Case 3. Provide certified commits where the first certified commit index is not the last_commited_index + 1.");

        // Highest certified commit should be for leader of round 4.
        let certified_commits = sub_dags_and_commits
            .iter()
            .skip(5)
            .take(1)
            .map(|(_, c)| c.clone())
            .collect::<Vec<_>>();

        let err = core
            .filter_new_commits(certified_commits.clone())
            .unwrap_err();
        match err {
            ConsensusError::UnexpectedCertifiedCommitIndex {
                expected_commit_index: 5,
                commit_index: 6,
            } => (),
            _ => panic!("Unexpected error: {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_add_certified_commits() {
        telemetry_subscribers::init_for_testing();

        let (context, _key_pairs) = Context::new_for_test(4);
        let context = context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        });

        let authority_index = AuthorityIndex::new_for_test(0);
        let core = CoreTextFixture::new(context, vec![1, 1, 1, 1], authority_index, true);
        let store = core.store.clone();
        let mut core = core.core;

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // create a DAG of 12 rounds
        let mut dag_builder = DagBuilder::new(core.context.clone());
        dag_builder.layers(1..=12).build();

        // Store all blocks up to round 6 which should be enough to decide up to leader 4
        dag_builder.print();
        let blocks = dag_builder.blocks(1..=6);

        for block in blocks {
            core.dag_state.write().accept_block(block);
        }

        // Get all the committed sub dags up to round 10
        let sub_dags_and_commits = dag_builder.get_sub_dag_and_certified_commits(1..=10);

        // Now try to commit up to the latest leader (round = 4). Do not provide any certified commits.
        let committed_sub_dags = core.try_commit(vec![]).unwrap();

        // We should have committed up to round 4
        assert_eq!(committed_sub_dags.len(), 4);

        let last_commit = store
            .read_last_commit()
            .unwrap()
            .expect("Last commit should be set");
        assert_eq!(last_commit.reference().index, 4);

        println!("Case 1. Provide no certified commits. No commit should happen.");

        let last_commit = store
            .read_last_commit()
            .unwrap()
            .expect("Last commit should be set");
        assert_eq!(last_commit.reference().index, 4);

        println!("Case 2. Provide certified commits that before and after the last committed round and also there are additional blocks so can run the direct decide rule as well.");

        // The commits of leader rounds 5-8 should be committed via the certified commits.
        let certified_commits = sub_dags_and_commits
            .iter()
            .skip(3)
            .take(5)
            .map(|(_, c)| c.clone())
            .collect::<Vec<_>>();

        // Now only add the blocks of rounds 8..=12. The blocks up to round 7 should be accepted via the certified commits processing.
        let blocks = dag_builder.blocks(8..=12);
        for block in blocks {
            core.dag_state.write().accept_block(block);
        }

        // The corresponding blocks of the certified commits should be accepted and stored before linearizing and committing the DAG.
        core.add_certified_commits(CertifiedCommits::new(certified_commits.clone(), vec![]))
            .expect("Should not fail");

        let commits = store.scan_commits((6..=10).into()).unwrap();

        // We expect all the sub dags up to leader round 10 to be committed.
        assert_eq!(commits.len(), 5);

        for i in 6..=10 {
            let commit = &commits[i - 6];
            assert_eq!(commit.reference().index, i as u32);
        }
    }

    #[tokio::test]
    async fn try_commit_with_certified_commits_gced_blocks() {
        const GC_DEPTH: u32 = 3;
        telemetry_subscribers::init_for_testing();

        let (mut context, mut key_pairs) = Context::new_for_test(5);
        context
            .protocol_config
            .set_consensus_gc_depth_for_testing(GC_DEPTH);
        //context.protocol_config.set_narwhal_new_leader_election_schedule_for_testing(val);
        let context = Arc::new(context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        }));

        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_manager = BlockManager::new(
            context.clone(),
            dag_state.clone(),
            Arc::new(NoopBlockVerifier),
        );
        let leader_schedule = Arc::new(
            LeaderSchedule::from_store(context.clone(), dag_state.clone())
                .with_num_commits_per_schedule(10),
        );

        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_consumer, _commit_receiver, _transaction_receiver) = CommitConsumer::new(0);
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        let round_tracker = Arc::new(RwLock::new(PeerRoundTracker::new(context.clone())));
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            transaction_certifier.clone(),
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            true,
            round_tracker,
        );

        // No new block should have been produced
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        let dag_str = "DAG {
            Round 0 : { 5 },
            Round 1 : { * },
            Round 2 : { 
                A -> [-E1],
                B -> [-E1],
                C -> [-E1],
                D -> [-E1],
            },
            Round 3 : {
                A -> [*],
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 4 : { 
                A -> [*],
                B -> [*],
                C -> [*],
                D -> [*],
            },
            Round 5 : { 
                A -> [*],
                B -> [*],
                C -> [*],
                D -> [*],
                E -> [A4, B4, C4, D4, E1]
            },
            Round 6 : { * },
            Round 7 : { * },
        }";

        let (_, mut dag_builder) = parse_dag(dag_str).expect("Invalid dag");
        dag_builder.print();

        // Now get all the committed sub dags from the DagBuilder
        let (_sub_dags, certified_commits): (Vec<_>, Vec<_>) = dag_builder
            .get_sub_dag_and_certified_commits(1..=5)
            .into_iter()
            .unzip();

        // Now try to commit up to the latest leader (round = 5) with the provided certified commits. Not that we have not accepted any
        // blocks. That should happen during the commit process.
        let committed_sub_dags = core.try_commit(certified_commits).unwrap();

        // We should have committed up to round 4
        assert_eq!(committed_sub_dags.len(), 4);
        for (index, committed_sub_dag) in committed_sub_dags.iter().enumerate() {
            assert_eq!(committed_sub_dag.commit_ref.index as usize, index + 1);

            // ensure that block from E1 node has not been committed
            for block in committed_sub_dag.blocks.iter() {
                if block.round() == 1 && block.author() == AuthorityIndex::new_for_test(5) {
                    panic!("Did not expect to commit block E1");
                }
            }
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_commit_on_leader_schedule_change_boundary_without_multileader() {
        parameterized_test_commit_on_leader_schedule_change_boundary(Some(1)).await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_commit_on_leader_schedule_change_boundary_with_multileader() {
        parameterized_test_commit_on_leader_schedule_change_boundary(None).await;
    }

    async fn parameterized_test_commit_on_leader_schedule_change_boundary(
        num_leaders_per_round: Option<usize>,
    ) {
        telemetry_subscribers::init_for_testing();
        let default_params = Parameters::default();

        let (mut context, _) = Context::new_for_test(6);
        context
            .protocol_config
            .set_mysticeti_num_leaders_per_round_for_testing(num_leaders_per_round);
        // create the cores and their signals for all the authorities
        let mut cores = create_cores(context, vec![1, 1, 1, 1, 1, 1]);

        // Now iterate over a few rounds and ensure the corresponding signals are created while network advances
        let mut last_round_blocks: Vec<VerifiedBlock> = Vec::new();
        for round in 1..=33 {
            let mut this_round_blocks = Vec::new();

            // Wait for min round delay to allow blocks to be proposed.
            sleep(default_params.min_round_delay).await;

            for core_fixture in &mut cores {
                // add the blocks from last round
                // this will trigger a block creation for the round and a signal should be emitted
                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                    ],
                    vec![
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                        vec![round, round, round, round, round, round],
                    ],
                );

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let extended_block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(extended_block.block.round(), round);
                assert_eq!(
                    extended_block.block.author(),
                    core_fixture.core.context.own_index
                );

                // append the new block to this round blocks
                this_round_blocks.push(core_fixture.core.last_proposed_block().clone());

                let block = core_fixture.core.last_proposed_block();

                // ensure that produced block is referring to the blocks of last_round
                assert_eq!(
                    block.ancestors().len(),
                    core_fixture.core.context.committee.size()
                );
                for ancestor in block.ancestors() {
                    if block.round() > 1 {
                        // don't bother with round 1 block which just contains the genesis blocks.
                        assert!(
                            last_round_blocks
                                .iter()
                                .any(|block| block.reference() == *ancestor),
                            "Reference from previous round should be added"
                        );
                    }
                }
            }

            last_round_blocks = this_round_blocks;
        }

        for core_fixture in cores {
            // Check commits have been persisted to store
            let last_commit = core_fixture
                .store
                .read_last_commit()
                .unwrap()
                .expect("last commit should be set");
            // There are 31 leader rounds with rounds completed up to and including
            // round 33. Round 33 blocks will only include their own blocks, so there
            // should only be 30 commits.
            // However on a leader schedule change boundary its is possible for a
            // new leader to get selected for the same round if the leader elected
            // gets swapped allowing for multiple leaders to be committed at a round.
            // Meaning with multi leader per round explicitly set to 1 we will have 30,
            // otherwise 31.
            // NOTE: We used 31 leader rounds to specifically trigger the scenario
            // where the leader schedule boundary occurred AND we had a swap to a new
            // leader for the same round
            let expected_commit_count = match num_leaders_per_round {
                Some(1) => 30,
                _ => 31,
            };
            assert_eq!(last_commit.index(), expected_commit_count);
            let all_stored_commits = core_fixture
                .store
                .scan_commits((0..=CommitIndex::MAX).into())
                .unwrap();
            assert_eq!(all_stored_commits.len(), expected_commit_count as usize);
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .bad_nodes
                    .len(),
                1
            );
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .good_nodes
                    .len(),
                1
            );
            let expected_reputation_scores =
                ReputationScores::new((21..=30).into(), vec![43, 43, 43, 43, 43, 43]);
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .reputation_scores,
                expected_reputation_scores
            );
        }
    }

    #[tokio::test]
    async fn test_core_signals() {
        telemetry_subscribers::init_for_testing();
        let default_params = Parameters::default();

        let (context, _) = Context::new_for_test(4);
        // create the cores and their signals for all the authorities
        let mut cores = create_cores(context, vec![1, 1, 1, 1]);

        // Now iterate over a few rounds and ensure the corresponding signals are created while network advances
        let mut last_round_blocks = Vec::new();
        for round in 1..=10 {
            let mut this_round_blocks = Vec::new();

            // Wait for min round delay to allow blocks to be proposed.
            sleep(default_params.min_round_delay).await;

            for core_fixture in &mut cores {
                // add the blocks from last round
                // this will trigger a block creation for the round and a signal should be emitted
                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();

                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                );

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let extended_block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(extended_block.block.round(), round);
                assert_eq!(
                    extended_block.block.author(),
                    core_fixture.core.context.own_index
                );

                // append the new block to this round blocks
                this_round_blocks.push(core_fixture.core.last_proposed_block().clone());

                let block = core_fixture.core.last_proposed_block();

                // ensure that produced block is referring to the blocks of last_round
                assert_eq!(
                    block.ancestors().len(),
                    core_fixture.core.context.committee.size()
                );
                for ancestor in block.ancestors() {
                    if block.round() > 1 {
                        // don't bother with round 1 block which just contains the genesis blocks.
                        assert!(
                            last_round_blocks
                                .iter()
                                .any(|block| block.reference() == *ancestor),
                            "Reference from previous round should be added"
                        );
                    }
                }
            }

            last_round_blocks = this_round_blocks;
        }

        for core_fixture in cores {
            // Check commits have been persisted to store
            let last_commit = core_fixture
                .store
                .read_last_commit()
                .unwrap()
                .expect("last commit should be set");
            // There are 8 leader rounds with rounds completed up to and including
            // round 9. Round 10 blocks will only include their own blocks, so the
            // 8th leader will not be committed.
            assert_eq!(last_commit.index(), 7);
            let all_stored_commits = core_fixture
                .store
                .scan_commits((0..=CommitIndex::MAX).into())
                .unwrap();
            assert_eq!(all_stored_commits.len(), 7);
        }
    }

    #[tokio::test]
    async fn test_core_compress_proposal_references() {
        telemetry_subscribers::init_for_testing();
        let default_params = Parameters::default();

        let (context, _) = Context::new_for_test(4);
        // create the cores and their signals for all the authorities
        let mut cores = create_cores(context, vec![1, 1, 1, 1]);

        let mut last_round_blocks = Vec::new();
        let mut all_blocks = Vec::new();

        let excluded_authority = AuthorityIndex::new_for_test(3);

        for round in 1..=10 {
            let mut this_round_blocks = Vec::new();

            for core_fixture in &mut cores {
                // do not produce any block for authority 3
                if core_fixture.core.context.own_index == excluded_authority {
                    continue;
                }

                // try to propose to ensure that we are covering the case where we miss the leader authority 3
                core_fixture.add_blocks(last_round_blocks.clone()).unwrap();
                core_fixture.core.round_tracker.write().update_from_probe(
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                    vec![
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                        vec![round, round, round, round],
                    ],
                );
                core_fixture.core.new_block(round, true).unwrap();

                let block = core_fixture.core.last_proposed_block();
                assert_eq!(block.round(), round);

                // append the new block to this round blocks
                this_round_blocks.push(block.clone());
            }

            last_round_blocks = this_round_blocks.clone();
            all_blocks.extend(this_round_blocks);
        }

        // Now send all the produced blocks to core of authority 3. It should produce a new block. If no compression would
        // be applied the we should expect all the previous blocks to be referenced from round 0..=10. However, since compression
        // is applied only the last round's (10) blocks should be referenced + the authority's block of round 0.
        let core_fixture = &mut cores[excluded_authority];
        // Wait for min round delay to allow blocks to be proposed.
        sleep(default_params.min_round_delay).await;
        // add blocks to trigger proposal.
        core_fixture.add_blocks(all_blocks).unwrap();

        // Assert that a block has been created for round 11 and it references to blocks of round 10 for the other peers, and
        // to round 1 for its own block (created after recovery).
        let block = core_fixture.core.last_proposed_block();
        assert_eq!(block.round(), 11);
        assert_eq!(block.ancestors().len(), 4);
        for block_ref in block.ancestors() {
            if block_ref.author == excluded_authority {
                assert_eq!(block_ref.round, 1);
            } else {
                assert_eq!(block_ref.round, 10);
            }
        }

        // Check commits have been persisted to store
        let last_commit = core_fixture
            .store
            .read_last_commit()
            .unwrap()
            .expect("last commit should be set");
        // There are 8 leader rounds with rounds completed up to and including
        // round 10. However because there were no blocks produced for authority 3
        // 2 leader rounds will be skipped.
        assert_eq!(last_commit.index(), 6);
        let all_stored_commits = core_fixture
            .store
            .scan_commits((0..=CommitIndex::MAX).into())
            .unwrap();
        assert_eq!(all_stored_commits.len(), 6);
    }

    #[tokio::test]
    async fn try_decide_certified() {
        // GIVEN
        telemetry_subscribers::init_for_testing();

        let (context, _) = Context::new_for_test(4);

        let authority_index = AuthorityIndex::new_for_test(0);
        let core = CoreTextFixture::new(context.clone(), vec![1, 1, 1, 1], authority_index, true);
        let mut core = core.core;

        let mut dag_builder = DagBuilder::new(Arc::new(context.clone()));
        dag_builder.layers(1..=12).build();

        let limit = 2;

        let blocks = dag_builder.blocks(1..=12);

        for block in blocks {
            core.dag_state.write().accept_block(block);
        }

        // WHEN
        let sub_dags_and_commits = dag_builder.get_sub_dag_and_certified_commits(1..=4);
        let mut certified_commits = sub_dags_and_commits
            .into_iter()
            .map(|(_, commit)| commit)
            .collect::<Vec<_>>();

        let leaders = core.try_decide_certified(&mut certified_commits, limit);

        // THEN
        assert_eq!(leaders.len(), 2);
        assert_eq!(certified_commits.len(), 2);
    }

    pub(crate) async fn receive<T: Copy>(timeout: Duration, mut receiver: watch::Receiver<T>) -> T {
        tokio::time::timeout(timeout, receiver.changed())
            .await
            .expect("Timeout while waiting to read from receiver")
            .expect("Signal receive channel shouldn't be closed");
        *receiver.borrow_and_update()
    }
}

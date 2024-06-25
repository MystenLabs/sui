// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, iter, sync::Arc, time::Duration, vec};

#[cfg(test)]
use consensus_config::{local_committee_and_keys, Stake};
use consensus_config::{AuthorityIndex, ProtocolKeyPair};
use itertools::Itertools as _;
#[cfg(test)]
use mysten_metrics::monitored_mpsc::{unbounded_channel, UnboundedReceiver};
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_macros::fail_point;
use tokio::{
    sync::{broadcast, watch},
    time::Instant,
};
use tracing::{debug, info, warn};

use crate::{
    block::{
        Block, BlockAPI, BlockRef, BlockTimestampMs, BlockV1, Round, SignedBlock, Slot,
        VerifiedBlock, GENESIS_ROUND,
    },
    block_manager::BlockManager,
    commit::CommittedSubDag,
    commit_observer::CommitObserver,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    leader_schedule::LeaderSchedule,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    threshold_clock::ThresholdClock,
    transaction::TransactionConsumer,
    universal_committer::{
        universal_committer_builder::UniversalCommitterBuilder, UniversalCommitter,
    },
};
#[cfg(test)]
use crate::{
    block_verifier::NoopBlockVerifier, storage::mem_store::MemStore, CommitConsumer,
    TransactionClient,
};

// Maximum number of commit votes to include in a block.
// TODO: Move to protocol config, and verify in BlockVerifier.
const MAX_COMMIT_VOTES_PER_BLOCK: usize = 100;

pub(crate) struct Core {
    context: Arc<Context>,
    /// The threshold clock that is used to keep track of the current round
    threshold_clock: ThresholdClock,
    /// The consumer to use in order to pull transactions to be included for the next proposals
    transaction_consumer: TransactionConsumer,
    /// The block manager which is responsible for keeping track of the DAG dependencies when processing new blocks
    /// and accept them or suspend if we are missing their causal history
    block_manager: BlockManager,
    /// Whether there are consumers waiting to consume blocks produced by the core.
    consumer_availability: bool,
    /// Used to make commit decisions for leader blocks in the dag.
    committer: UniversalCommitter,
    /// The last produced block
    last_proposed_block: VerifiedBlock,
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
}

impl Core {
    pub(crate) fn new(
        context: Arc<Context>,
        leader_schedule: Arc<LeaderSchedule>,
        transaction_consumer: TransactionConsumer,
        block_manager: BlockManager,
        consumer_availability: bool,
        commit_observer: CommitObserver,
        signals: CoreSignals,
        block_signer: ProtocolKeyPair,
        dag_state: Arc<RwLock<DagState>>,
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

        // Recover the last proposed block
        let last_proposed_block = dag_state
            .read()
            .get_last_block_for_authority(context.own_index);

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

        Self {
            context: context.clone(),
            threshold_clock: ThresholdClock::new(0, context.clone()),
            last_proposed_block,
            last_included_ancestors,
            last_decided_leader,
            leader_schedule,
            transaction_consumer,
            block_manager,
            consumer_availability,
            committer,
            commit_observer,
            signals,
            block_signer,
            dag_state,
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
            .fold(0, |ts, b| ts.max(b.timestamp_ms()));
        let wait_ms = max_ancestor_timestamp.saturating_sub(self.context.clock.timestamp_utc_ms());
        if wait_ms > 0 {
            warn!(
                "Waiting for {} ms while recovering ancestors from storage",
                wait_ms
            );
            println!(
                "Waiting for {} ms while recovering ancestors from storage",
                wait_ms
            );
            std::thread::sleep(Duration::from_millis(wait_ms));
        }
        // Recover the last available quorum to correctly advance the threshold clock.
        let last_quorum = self.dag_state.read().last_quorum();
        self.add_accepted_blocks(last_quorum);
        // Try to commit and propose, since they may not have run after the last storage write.
        self.try_commit().unwrap();
        if self.try_propose(true).unwrap().is_none() {
            if self.should_propose() {
                assert!(self.last_proposed_block.round() > GENESIS_ROUND, "At minimum a block of round higher that genesis should have been produced during recovery");
            }

            // if no new block proposed then just re-broadcast the last proposed one to ensure liveness.
            self.signals
                .new_block(self.last_proposed_block.clone())
                .unwrap();
        }

        self
    }

    /// Processes the provided blocks and accepts them if possible when their causal history exists.
    /// The method returns the references of parents that are unknown and need to be fetched.
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

        // Try to accept them via the block manager
        let (accepted_blocks, missing_blocks) = self.block_manager.try_accept_blocks(blocks);

        if !accepted_blocks.is_empty() {
            debug!(
                "Accepted blocks: {}",
                accepted_blocks
                    .iter()
                    .map(|b| b.reference().to_string())
                    .join(",")
            );

            // Now add accepted blocks to the threshold clock and pending ancestors list.
            self.add_accepted_blocks(accepted_blocks);

            self.try_commit()?;

            // Try to propose now since there are new blocks accepted.
            self.try_propose(false)?;
        }

        if !missing_blocks.is_empty() {
            debug!("Missing blocks: {:?}", missing_blocks);
        }

        Ok(missing_blocks)
    }

    /// Adds/processed all the newly `accepted_blocks`. We basically try to move the threshold clock and add them to the
    /// pending ancestors list.
    fn add_accepted_blocks(&mut self, accepted_blocks: Vec<VerifiedBlock>) {
        // Advance the threshold clock. If advanced to a new round then send a signal that a new quorum has been received.
        if let Some(new_round) = self
            .threshold_clock
            .add_blocks(accepted_blocks.iter().map(|b| b.reference()).collect())
        {
            // notify that threshold clock advanced to new round
            self.signals.new_round(new_round);
        }

        // Report the threshold clock round
        self.context
            .metrics
            .node_metrics
            .threshold_clock_round
            .set(self.threshold_clock.get_round() as i64);
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
            return self.try_propose(force);
        }
        Ok(None)
    }

    // Attempts to create a new block, persist and propose it to all peers.
    // When force is true, ignore if leader from the last round exists among ancestors and if
    // the minimum round delay has passed.
    fn try_propose(&mut self, force: bool) -> ConsensusResult<Option<VerifiedBlock>> {
        if !self.should_propose() {
            return Ok(None);
        }
        if let Some(block) = self.try_new_block(force) {
            self.signals.new_block(block.clone())?;

            fail_point!("consensus-after-propose");

            // The new block may help commit.
            self.try_commit()?;
            return Ok(Some(block));
        }
        Ok(None)
    }

    /// Attempts to propose a new block for the next round. If a block has already proposed for latest
    /// or earlier round, then no block is created and None is returned.
    fn try_new_block(&mut self, force: bool) -> Option<VerifiedBlock> {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::try_new_block"])
            .start_timer();

        let clock_round = self.threshold_clock.get_round();
        if clock_round <= self.last_proposed_round() {
            return None;
        }

        // There must be a quorum of blocks from the previous round.
        let quorum_round = self.threshold_clock.get_round().saturating_sub(1);

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
                    .saturating_duration_since(self.threshold_clock.get_quorum_ts())
                    .as_millis() as u64,
            );

        self.context
            .metrics
            .node_metrics
            .block_proposal_leader_wait_count
            .with_label_values(&[leader_authority])
            .inc();

        // TODO: produce the block for the clock_round. As the threshold clock can advance many rounds at once (ex
        // because we synchronized a bulk of blocks) we can decide here whether we want to produce blocks per round
        // or just the latest one. From earlier experiments I saw only benefit on proposing for the penultimate round
        // only when the validator was supposed to be the leader of the round - so we bring down the missed leaders.
        // Probably proposing for all the intermediate rounds might not make much sense.

        // Determine the ancestors to be included in proposal
        let ancestors = self.ancestors_to_propose(clock_round);
        self.context
            .metrics
            .node_metrics
            .block_ancestors
            .observe(ancestors.len() as f64);

        // Ensure ancestor timestamps are not more advanced than the current time.
        // Also catch the issue if system's clock go backwards.
        let now = self.context.clock.timestamp_utc_ms();
        ancestors.iter().for_each(|block| {
            assert!(
                block.timestamp_ms() <= now,
                "Violation: ancestor block {:?} has timestamp {}, greater than current timestamp {now}. Proposing for round {}.",
                block, block.timestamp_ms(), clock_round
            );
        });

        // Consume the next transactions to be included. Do not drop the guards yet as this would acknowledge
        // the inclusion of transactions. Just let this be done in the end of the method.
        let (transactions, ack_transactions) = self.transaction_consumer.next();

        // Consume the commit votes to be included.
        let commit_votes = self
            .dag_state
            .write()
            .take_commit_votes(MAX_COMMIT_VOTES_PER_BLOCK);

        // Create the block and insert to storage.
        let block = Block::V1(BlockV1::new(
            self.context.committee.epoch(),
            clock_round,
            self.context.own_index,
            now,
            ancestors.iter().map(|b| b.reference()).collect(),
            transactions,
            commit_votes,
            vec![],
        ));
        let signed_block =
            SignedBlock::new(block, &self.block_signer).expect("Block signing failed.");
        let serialized = signed_block
            .serialize()
            .expect("Block serialization failed.");
        self.context
            .metrics
            .node_metrics
            .block_size
            .observe(serialized.len() as f64);
        // Unnecessary to verify own blocks.
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized);

        // Accept the block into BlockManager and DagState.
        let (accepted_blocks, missing) = self
            .block_manager
            .try_accept_blocks(vec![verified_block.clone()]);
        assert_eq!(accepted_blocks.len(), 1);
        assert!(missing.is_empty());

        // Internally accept the block to move the threshold clock etc
        self.add_accepted_blocks(vec![verified_block.clone()]);

        // Ensure the new block and its ancestors are persisted, before broadcasting it.
        self.dag_state.write().flush();

        // Update internal state.
        self.last_proposed_block = verified_block.clone();

        // Now acknowledge the transactions for their inclusion to block
        ack_transactions(verified_block.reference());

        info!("Created block {:?}", verified_block);

        self.context
            .metrics
            .node_metrics
            .proposed_blocks
            .with_label_values(&[&force.to_string()])
            .inc();

        Some(verified_block)
    }

    /// Runs commit rule to attempt to commit additional blocks from the DAG.
    fn try_commit(&mut self) -> ConsensusResult<Vec<CommittedSubDag>> {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["Core::try_commit"])
            .start_timer();

        if !self
            .context
            .protocol_config
            .mysticeti_leader_scoring_and_schedule()
        {
            let decided_leaders = self.committer.try_decide(self.last_decided_leader);
            if let Some(last) = decided_leaders.last() {
                self.last_decided_leader = last.slot();
                self.context
                    .metrics
                    .node_metrics
                    .last_decided_leader_round
                    .set(self.last_decided_leader.round as i64);
            }

            let committed_leaders = decided_leaders
                .into_iter()
                .filter_map(|leader| leader.into_committed_block())
                .collect::<Vec<_>>();
            if !committed_leaders.is_empty() {
                debug!(
                    "Committing leaders: {}",
                    committed_leaders
                        .iter()
                        .map(|b| b.reference().to_string())
                        .join(",")
                );
            }
            self.commit_observer.handle_commit(committed_leaders)
        } else {
            let mut committed_subdags = Vec::new();
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
                    self.leader_schedule.update_leader_schedule(&self.dag_state);
                    commits_until_update = self
                        .leader_schedule
                        .commits_until_leader_schedule_update(self.dag_state.clone());

                    fail_point!("consensus-after-leader-schedule-change");
                }
                assert!(commits_until_update > 0);

                // TODO: limit commits by commits_until_update, which may be needed when leader schedule length
                // is reduced.
                let decided_leaders = self.committer.try_decide(self.last_decided_leader);

                let Some(last_decided) = decided_leaders.last().cloned() else {
                    break;
                };
                tracing::info!("Decided {} leaders and {commits_until_update} commits can be made before next leader schedule change", decided_leaders.len());

                let mut sequenced_leaders = decided_leaders
                    .into_iter()
                    .filter_map(|leader| leader.into_committed_block())
                    .collect::<Vec<_>>();

                // If the sequenced leaders are truncated to fit the leader schedule, use the last sequenced leader
                // as the last decided leader. Otherwise, use the last decided leader from try_commit().
                let sequenced_leaders = if sequenced_leaders.len() >= commits_until_update {
                    let _ = sequenced_leaders.split_off(commits_until_update);
                    self.last_decided_leader = sequenced_leaders.last().unwrap().slot();
                    sequenced_leaders
                } else {
                    self.last_decided_leader = last_decided.slot();
                    sequenced_leaders
                };

                self.context
                    .metrics
                    .node_metrics
                    .last_decided_leader_round
                    .set(self.last_decided_leader.round as i64);

                if sequenced_leaders.is_empty() {
                    break;
                }
                tracing::info!(
                    "Committing {} leaders: {}",
                    sequenced_leaders.len(),
                    sequenced_leaders
                        .iter()
                        .map(|b| b.reference().to_string())
                        .join(",")
                );

                // TODO: refcount subdags
                let subdags = self.commit_observer.handle_commit(sequenced_leaders)?;
                self.dag_state
                    .write()
                    .add_unscored_committed_subdags(subdags.clone());
                committed_subdags.extend(subdags);
            }

            Ok(committed_subdags)
        }
    }

    pub(crate) fn get_missing_blocks(&self) -> BTreeSet<BlockRef> {
        let _scope = monitored_scope("Core::get_missing_blocks");
        self.block_manager.missing_blocks()
    }

    /// Sets if there is consumer available to consume blocks produced by the core.
    pub(crate) fn set_consumer_availability(&mut self, allow: bool) {
        info!("Block consumer availability set to: {allow}");
        self.consumer_availability = allow;
    }

    /// Whether the core should propose new blocks.
    fn should_propose(&self) -> bool {
        self.consumer_availability
    }

    /// Retrieves the next ancestors to propose to form a block at `clock_round` round.
    fn ancestors_to_propose(&mut self, clock_round: Round) -> Vec<VerifiedBlock> {
        // Now take the ancestors before the clock_round (excluded) for each authority.
        let ancestors = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(clock_round);
        assert_eq!(
            ancestors.len(),
            self.context.committee.size(),
            "Fatal error, number of returned ancestors don't match committee size."
        );

        // Propose only ancestors of higher rounds than what has already been proposed.
        // And always include own last proposed block first among ancestors.
        let ancestors = iter::once(self.last_proposed_block.clone())
            .chain(
                ancestors
                    .into_iter()
                    .filter(|block| block.author() != self.context.own_index)
                    .flat_map(|block| {
                        if let Some(last_block_ref) = self.last_included_ancestors[block.author()] {
                            return (last_block_ref.round < block.round()).then_some(block);
                        }
                        Some(block)
                    }),
            )
            .collect::<Vec<_>>();

        // Update the last included ancestor block refs
        for ancestor in &ancestors {
            self.last_included_ancestors[ancestor.author()] = Some(ancestor.reference());
        }

        // TODO: this is for temporary sanity check - we might want to remove later on
        let mut quorum = StakeAggregator::<QuorumThreshold>::new();
        for ancestor in ancestors
            .iter()
            .filter(|block| block.round() == clock_round - 1)
        {
            quorum.add(ancestor.author(), &self.context.committee);
        }
        assert!(quorum.reached_threshold(&self.context.committee), "Fatal error, quorum not reached for parent round when proposing for round {}. Possible mismatch between DagState and Core.", clock_round);

        ancestors
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
        self.last_proposed_block.timestamp_ms()
    }

    fn last_proposed_round(&self) -> Round {
        self.last_proposed_block.round()
    }

    #[cfg(test)]
    fn last_proposed_block(&self) -> &VerifiedBlock {
        &self.last_proposed_block
    }
}

/// Senders of signals from Core, for outputs and events (ex new block produced).
pub(crate) struct CoreSignals {
    tx_block_broadcast: broadcast::Sender<VerifiedBlock>,
    new_round_sender: watch::Sender<Round>,
    context: Arc<Context>,
}

impl CoreSignals {
    pub fn new(context: Arc<Context>) -> (Self, CoreSignalsReceivers) {
        // Blocks buffered in broadcast channel should be roughly equal to thosed cached in dag state,
        // since the underlying blocks are ref counted so a lower buffer here will not reduce memory
        // usage significantly.
        let (tx_block_broadcast, rx_block_broadcast) = broadcast::channel::<VerifiedBlock>(
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
    pub(crate) fn new_block(&self, block: VerifiedBlock) -> ConsensusResult<()> {
        // When there is only one authority in committee, it is unnecessary to broadcast
        // the block which will fail anyway without subscribers to the signal.
        if self.context.committee.size() > 1 {
            if let Err(err) = self.tx_block_broadcast.send(block) {
                warn!("Couldn't broadcast the block to any receiver: {err}");
                return Err(ConsensusError::Shutdown);
            }
        } else {
            debug!("Did not broadcast block {block:?} to receivers as committee size is <= 1");
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
    rx_block_broadcast: broadcast::Receiver<VerifiedBlock>,
    new_round_receiver: watch::Receiver<Round>,
}

impl CoreSignalsReceivers {
    pub(crate) fn block_broadcast_receiver(&self) -> broadcast::Receiver<VerifiedBlock> {
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
        let core = CoreTextFixture::new(context.clone(), authorities.clone(), own_index);
        cores.push(core);
    }
    cores
}

#[cfg(test)]
pub(crate) struct CoreTextFixture {
    pub core: Core,
    pub signal_receivers: CoreSignalsReceivers,
    pub block_receiver: broadcast::Receiver<VerifiedBlock>,
    #[allow(unused)]
    pub commit_receiver: UnboundedReceiver<CommittedSubDag>,
    pub store: Arc<MemStore>,
}

#[cfg(test)]
impl CoreTextFixture {
    fn new(context: Context, authorities: Vec<Stake>, own_index: AuthorityIndex) -> Self {
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
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let block_receiver = signal_receivers.block_broadcast_receiver();

        let (commit_sender, commit_receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(commit_sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        let block_signer = signers.remove(own_index.value()).1;

        let core = Core::new(
            context,
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            block_signer,
            dag_state,
        );

        Self {
            core,
            signal_receivers,
            block_receiver,
            commit_receiver,
            store,
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeSet, time::Duration};

    use consensus_config::{AuthorityIndex, Parameters};
    use mysten_metrics::monitored_mpsc::unbounded_channel;
    use sui_protocol_config::ProtocolConfig;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        block::{genesis_blocks, TestBlock},
        block_verifier::NoopBlockVerifier,
        commit::{CommitAPI as _, CommitRange},
        leader_scoring::ReputationScores,
        storage::{mem_store::MemStore, Store, WriteBatch},
        transaction::TransactionClient,
        CommitConsumer, CommitIndex,
    };

    /// Recover Core and continue proposing from the last round which forms a quorum.
    #[tokio::test]
    async fn test_core_recover_from_store_for_full_round() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // Create test blocks for all the authorities for 4 rounds and populate them in store
        let mut last_round_blocks = genesis_blocks(context.clone());
        let mut all_blocks: Vec<VerifiedBlock> = last_round_blocks.clone();
        for round in 1..=4 {
            let mut this_round_blocks = Vec::new();
            for (index, _authority) in context.committee.authorities() {
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, index.value() as u32)
                        .set_ancestors(last_round_blocks.iter().map(|b| b.reference()).collect())
                        .build(),
                );

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

        let (sender, _receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state or store.
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
        );

        // New round should be 5
        let mut new_round = signal_receivers.new_round_receiver();
        assert_eq!(*new_round.borrow_and_update(), 5);

        // Block for round 5 should have been proposed.
        let proposed_block = block_receiver
            .recv()
            .await
            .expect("A block should have been created");
        assert_eq!(proposed_block.round(), 5);
        let ancestors = proposed_block.ancestors();

        // Only ancestors of round 4 should be included.
        assert_eq!(ancestors.len(), 4);
        for ancestor in ancestors {
            assert_eq!(ancestor.round, 4);
        }

        // Run commit rule.
        core.try_commit().ok();
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
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // Create test blocks for all authorities except our's (index = 0).
        let mut last_round_blocks = genesis_blocks(context.clone());
        let mut all_blocks = last_round_blocks.clone();
        for round in 1..=4 {
            let mut this_round_blocks = Vec::new();

            // For round 4 only produce f+1 blocks only skip our validator and that of position 1 from creating blocks.
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

        let (sender, _receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        // Check no commits have been persisted to dag_state & store
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);

        // Now spin up core
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
        );

        // New round should be 4
        let mut new_round = signal_receivers.new_round_receiver();
        assert_eq!(*new_round.borrow_and_update(), 4);

        // When trying to propose now we should propose block for round 4
        let proposed_block = block_receiver
            .recv()
            .await
            .expect("A block should have been created");
        assert_eq!(proposed_block.round(), 4);
        let ancestors = proposed_block.ancestors();

        assert_eq!(ancestors.len(), 4);
        for ancestor in ancestors {
            if ancestor.author == context.own_index {
                assert_eq!(ancestor.round, 0);
            } else {
                assert_eq!(ancestor.round, 3);
            }
        }

        // Run commit rule.
        core.try_commit().ok();
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
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let mut block_receiver = signal_receivers.block_broadcast_receiver();
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let (sender, _receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
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
        let block = block_receiver
            .recv()
            .await
            .expect("A new block should have been created");

        // A new block created - assert the details
        assert_eq!(block.round(), 1);
        assert_eq!(block.author().value(), 0);
        assert_eq!(block.ancestors().len(), 4);

        let mut total = 0;
        for (i, transaction) in block.transactions().iter().enumerate() {
            total += transaction.data().len() as u64;
            let transaction: String = bcs::from_bytes(transaction.data()).unwrap();
            assert_eq!(format!("Transaction {i}"), transaction);
        }
        assert!(
            total
                <= context
                    .protocol_config
                    .consensus_max_transactions_in_block_bytes()
        );

        // genesis blocks should be referenced
        let all_genesis = genesis_blocks(context);

        for ancestor in block.ancestors() {
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
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (sender, _receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
        );

        let mut expected_ancestors = BTreeSet::new();

        // Adding one block now will trigger the creation of new block for round 1
        let block_1 = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        expected_ancestors.insert(block_1.reference());
        // Wait for min round delay to allow blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // add blocks to trigger proposal.
        _ = core.add_blocks(vec![block_1]);

        assert_eq!(core.last_proposed_round(), 1);
        expected_ancestors.insert(core.last_proposed_block().reference());
        // attempt to create a block - none will be produced.
        assert!(core.try_propose(false).unwrap().is_none());

        // Adding another block now forms a quorum for round 1, so block at round 2 will proposed
        let block_3 = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        expected_ancestors.insert(block_3.reference());
        // Wait for min round delay to allow blocks to be proposed.
        sleep(context.parameters.min_round_delay).await;
        // add blocks to trigger proposal.
        _ = core.add_blocks(vec![block_3]);

        assert_eq!(core.last_proposed_round(), 2);

        let proposed_block = core.last_proposed_block();
        assert_eq!(proposed_block.round(), 2);
        assert_eq!(proposed_block.author(), context.own_index);
        assert_eq!(proposed_block.ancestors().len(), 3);
        let ancestors = proposed_block.ancestors();
        let ancestors = ancestors.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(ancestors, expected_ancestors);

        // Check no commits have been persisted to dag_state & store
        let last_commit = store.read_last_commit().unwrap();
        assert!(last_commit.is_none());
        assert_eq!(dag_state.read().last_commit_index(), 0);
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

                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();

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

                this_round_blocks.push(core_fixture.core.last_proposed_block.clone());
            }

            last_round_blocks = this_round_blocks;
        }

        // Try to create the blocks for round 4 by calling the try_propose() method. No block should be created as the
        // leader - authority 3 - hasn't proposed any block.
        for core_fixture in cores.iter_mut() {
            wait_blocks(&last_round_blocks, &core_fixture.core.context).await;

            core_fixture
                .core
                .add_blocks(last_round_blocks.clone())
                .unwrap();
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

    #[tokio::test]
    async fn test_core_set_consumer_availability() {
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
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        // Need at least one subscriber to the block broadcast channel.
        let _block_receiver = signal_receivers.block_broadcast_receiver();

        let (sender, _receiver) = unbounded_channel("consensus_output");
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0, 0),
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        let mut core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            false,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
        );

        // No proposal during recovery.
        assert_eq!(
            core.last_proposed_round(),
            GENESIS_ROUND,
            "No block should have been created other than genesis"
        );

        // No proposal even with forced proposing.
        assert!(core.try_propose(true).unwrap().is_none());

        // Update core when consumer is available.
        core.set_consumer_availability(true);

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
                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(block.round(), round);
                assert_eq!(block.author(), core_fixture.core.context.own_index);

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
                ReputationScores::new((11..=20).into(), vec![9, 8, 8, 8]);
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

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_no_leader_schedule_change() {
        telemetry_subscribers::init_for_testing();
        let default_params = Parameters::default();

        let (mut context, _) = Context::new_for_test(4);
        context
            .protocol_config
            .set_mysticeti_leader_scoring_and_schedule_for_testing(false);
        // create the cores and their signals for all the authorities
        let mut cores = create_cores(context, vec![1, 1, 1, 1]);

        // Now iterate over a few rounds and ensure the corresponding signals are created while network advances
        let mut last_round_blocks = Vec::new();
        for round in 1..=30 {
            let mut this_round_blocks = Vec::new();

            for core_fixture in &mut cores {
                // Wait for min round delay to allow blocks to be proposed.
                sleep(default_params.min_round_delay).await;
                // add the blocks from last round
                // this will trigger a block creation for the round and a signal should be emitted
                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(block.round(), round);
                assert_eq!(block.author(), core_fixture.core.context.own_index);

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
                0
            );
            assert_eq!(
                core_fixture
                    .core
                    .leader_schedule
                    .leader_swap_table
                    .read()
                    .good_nodes
                    .len(),
                0
            );
            let expected_reputation_scores = ReputationScores::new(CommitRange::default(), vec![]);
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
        let mut last_round_blocks = Vec::new();
        for round in 1..=63 {
            let mut this_round_blocks = Vec::new();

            // Wait for min round delay to allow blocks to be proposed.
            sleep(default_params.min_round_delay).await;

            for core_fixture in &mut cores {
                // add the blocks from last round
                // this will trigger a block creation for the round and a signal should be emitted
                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(block.round(), round);
                assert_eq!(block.author(), core_fixture.core.context.own_index);

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
            // There are 61 leader rounds with rounds completed up to and including
            // round 63. Round 63 blocks will only include their own blocks, so there
            // should only be 60 commits.
            // However on a leader schedule change boundary its is possible for a
            // new leader to get selected for the same round if the leader elected
            // gets swapped allowing for multiple leaders to be committed at a round.
            // Meaning with multi leader per round explicitly set to 1 we will have 60,
            // otherwise 61.
            // NOTE: We used 61 leader rounds to specifically trigger the scenario
            // where the leader schedule boundary occurred AND we had a swap to a new
            // leader for the same round
            let expected_commit_count = match num_leaders_per_round {
                Some(1) => 60,
                _ => 61,
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
                ReputationScores::new((51..=60).into(), vec![8, 8, 9, 8, 8, 8]);
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
                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();

                // A "new round" signal should be received given that all the blocks of previous round have been processed
                let new_round = receive(
                    Duration::from_secs(1),
                    core_fixture.signal_receivers.new_round_receiver(),
                )
                .await;
                assert_eq!(new_round, round);

                // Check that a new block has been proposed.
                let block = tokio::time::timeout(
                    Duration::from_secs(1),
                    core_fixture.block_receiver.recv(),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(block.round(), round);
                assert_eq!(block.author(), core_fixture.core.context.own_index);

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
                core_fixture
                    .core
                    .add_blocks(last_round_blocks.clone())
                    .unwrap();
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
        core_fixture.core.add_blocks(all_blocks).unwrap();

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

    pub(crate) async fn receive<T: Copy>(timeout: Duration, mut receiver: watch::Receiver<T>) -> T {
        tokio::time::timeout(timeout, receiver.changed())
            .await
            .expect("Timeout while waiting to read from receiver")
            .expect("Signal receive channel shouldn't be closed");
        *receiver.borrow_and_update()
    }
}

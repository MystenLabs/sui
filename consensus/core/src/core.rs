// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashSet, VecDeque},
    mem,
    sync::Arc,
};

use crate::{
    block::{
        timestamp_utc_ms, Block, BlockAPI, BlockRef, BlockTimestampMs, BlockV1, Round, SignedBlock,
        Transaction, VerifiedBlock,
    },
    block_manager::BlockManager,
    context::Context,
    threshold_clock::ThresholdClock,
    transactions_client::TransactionsConsumer,
};

use consensus_config::AuthorityIndex;
use mysten_metrics::monitored_scope;
use tokio::sync::watch;

/// The maximum transaction payload defined in bytes.
const MAX_BLOCK_PAYLOAD_SIZE_BYTES: usize = 100_000; // 100 KB

#[allow(dead_code)]
pub(crate) struct Core {
    context: Arc<Context>,
    /// The threshold clock that is used to keep track of the current round
    threshold_clock: ThresholdClock,
    /// The last produced block
    last_own_block: VerifiedBlock,
    /// The consumer to use in order to pull transactions to be included for the next proposals
    transactions_consumer: TransactionsConsumer,
    /// The transactions that haven't been proposed yet and are to be included in the next proposal
    pending_transactions: Vec<Transaction>,
    /// The pending blocks refs to be included as ancestors to the next block. Every block ref that is included in this list
    /// we assume that has already been sanitised according to the timestamp thresholds during block receive and their
    /// timestamps are already <= now().
    pending_ancestors: VecDeque<(BlockRef, BlockTimestampMs)>,
    /// The block manager which is responsible for keeping track of the DAG dependencies when processing new blocks
    /// and accept them or suspend if we are missing their causal history
    block_manager: BlockManager,
    /// Signals that the component emits
    signals: CoreSignals,
}

#[allow(dead_code)]
impl Core {
    pub(crate) fn new(
        context: Arc<Context>,
        transactions_consumer: TransactionsConsumer,
        block_manager: BlockManager,
        signals: CoreSignals,
    ) -> Self {
        // TODO: restore the threshold clock round based on the last quorum data in storage when crash/recover
        let mut threshold_clock = ThresholdClock::new(0, context.clone());

        // TODO: restore based on DagState, for now we just init via the genesis
        let (genesis_my, genesis_others) = Block::genesis(context.clone());
        let pending_ancestors = genesis_others
            .into_iter()
            .map(|block| (block.reference(), block.timestamp_ms()))
            .collect::<VecDeque<_>>();

        // populate the threshold clock to properly advance the round
        for (ancestor, _) in &pending_ancestors {
            threshold_clock.add_block(*ancestor);
        }
        threshold_clock.add_block(genesis_my.reference());

        Self {
            context,
            threshold_clock,
            last_own_block: genesis_my,
            transactions_consumer,
            pending_transactions: Vec::new(),
            pending_ancestors,
            block_manager,
            signals,
        }
    }

    /// Processes the provided blocks and accepts them if possible when their causal history exists.
    /// The method returns the references of parents that are unknown and need to be fetched.
    pub(crate) fn add_blocks(&mut self, blocks: Vec<VerifiedBlock>) -> Vec<BlockRef> {
        let _scope = monitored_scope("Core::add_blocks");

        // Try to accept them via the block manager
        let accepted_blocks = self.block_manager.add_blocks(blocks);

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

        // Add the processed blocks to the list of pending
        for block in accepted_blocks {
            self.pending_ancestors
                .push_back((block.reference(), block.timestamp_ms()));
        }

        // Pull transactions to be proposed and append to the pending list
        for transaction in self.transactions_consumer.next() {
            self.pending_transactions.push(transaction);
        }

        // Attempt to create a new block
        let _ = self.try_new_block(false);

        // TODO: we don't deal for now with missed references, will address later.
        vec![]
    }

    /// Force creating a new block for the dictated round. This is used when a leader timeout occurs.
    pub(crate) fn try_new_block_leader_timeout(&mut self, round: Round) -> Option<VerifiedBlock> {
        if self.last_proposed_round() < round {
            self.context.metrics.node_metrics.leader_timeout_total.inc();
            self.try_new_block(true)
        } else {
            None
        }
    }

    /// Attempts to propose a new block for the next round. If a block has already proposed for latest
    /// or earlier round, then no block is created and None is returned.
    fn try_new_block(&mut self, ignore_leaders_check: bool) -> Option<VerifiedBlock> {
        let _scope = monitored_scope("Core::try_new_block");

        let clock_round = self.threshold_clock.get_round();
        if clock_round <= self.last_proposed_round() {
            return None;
        }

        // create a new block either because we want to "forcefully" propose a block due to a leader timeout,
        // or because we are actually ready to produce the block (leader exists)
        if ignore_leaders_check || self.last_quorum_leaders_exist() {
            // TODO: produce the block for the clock_round. As the threshold clock can advance many rounds at once (ex
            // because we synchronized a bulk of blocks) we can decide here whether we want to produce blocks per round
            // or just the latest one. From earlier experiments I saw only benefit on proposing for the penultimate round
            // only when the validator was supposed to be the leader of the round - so we bring down the missed leaders.
            // Probably proposing for all the intermediate rounds might not make much sense.
            let now = timestamp_utc_ms();
            let ancestors = self.ancestors_to_propose(clock_round, now);

            //2. consume the next transactions to be included. Split the vec and swap because we want to hold on the
            // `payload` variable the front of the queue (the older transactions).
            let payload = self.payload_to_propose();

            //3. create the block and insert to storage.
            // TODO: take a decision on whether we want to flush to disk at this point the DagState.

            // TODO: this will be refactored once the signing path/approach has been introduced. Adding as is for now
            // to keep things rolling in the implementation.
            let block = Block::V1(BlockV1::new(
                clock_round,
                self.context.own_index,
                now,
                ancestors,
                payload,
                self.context.committee.epoch(),
            ));
            let signed_block = SignedBlock::new(block);
            let verified_block = VerifiedBlock::new_verified_unserialized(signed_block)
                .expect("Fatal error, creating a verified block failed");

            //4. Add to the threshold clock
            self.threshold_clock.add_block(verified_block.reference());
            self.last_own_block = verified_block.clone();

            tracing::debug!("New block created {}", verified_block);

            //5. emit an event that a new block is ready
            self.signals.new_block_ready(verified_block.reference());

            return Some(verified_block);
        }

        None
    }

    /// Retrieves the next ancestors to propose to form a block at `clock_round` round. Also the `block_timestamp` is provided
    /// to sanity check that everything that goes into the proposal is ensured to have a timestamp < block_timestamp
    fn ancestors_to_propose(
        &mut self,
        clock_round: Round,
        block_timestamp: BlockTimestampMs,
    ) -> Vec<BlockRef> {
        // Now take all the ancestors up to the clock_round (excluded) and then filter only the ones with acceptable timestamp.
        let first_include_index = self
            .pending_ancestors
            .iter()
            .position(|(block_ref, timestamp)| {
                // We assume that our system's clock can't go backwards when we perform the check here (ex due to ntp corrections)
                assert!(*timestamp <= block_timestamp, "Violation, ancestor block timestamp {timestamp} greater than our timestamp {block_timestamp}");

                block_ref.round > clock_round
            })
            .unwrap_or(self.pending_ancestors.len());

        let mut taken = self.pending_ancestors.split_off(first_include_index);
        mem::swap(&mut taken, &mut self.pending_ancestors);

        // Compress the references in the block. We don't want to include an ancestors that already referenced by other blocks
        // we are about to include.
        let mut references_in_block: HashSet<BlockRef> = HashSet::new();
        let mut ancestors = taken
            .iter()
            .map(|(block_ref, _timestamp)| *block_ref)
            .collect::<Vec<_>>();

        // explicitly add our last produced block to ensure that is going to get referenced (assuming is not excluded by dag compression)
        ancestors.push(self.last_own_block.reference());

        for block in self.get_blocks(&ancestors).into_iter().flatten() {
            references_in_block.extend(block.ancestors());
        }

        let mut to_propose = vec![];
        for block_ref in ancestors.into_iter() {
            if !references_in_block.contains(&block_ref) {
                to_propose.push(block_ref);
            }
        }

        assert!(!to_propose.is_empty());

        to_propose
    }

    fn get_blocks(&self, _block_refs: &[BlockRef]) -> Vec<Option<VerifiedBlock>> {
        vec![]
    }

    /// Checks whether all the leaders of the previous quorum exist.
    /// TODO: we can leverage some additional signal here in order to more cleverly manipulate later the leader timeout
    /// Ex if we already have one leader - the first in order - we might don't want to wait as much.
    fn last_quorum_leaders_exist(&self) -> bool {
        // TODO: check that we are ready to produce a new block. This will mainly check that the leaders of the previous
        // quorum exist.
        let quorum_round = self.threshold_clock.get_round().saturating_sub(1);

        let leaders = self.leaders(quorum_round);
        let leaders_blocks = self.blocks_at_round(quorum_round, &leaders);

        // Search for all the leaders
        leaders.iter().all(|leader| {
            leaders_blocks
                .iter()
                .any(|block_ref| block_ref.author == *leader)
        })
    }

    /// Returns the blocks at the specified round for the given authorities. The method might return more than one block
    /// per authority in case of equivocation.
    fn blocks_at_round(&self, round: Round, authorities: &[AuthorityIndex]) -> Vec<&BlockRef> {
        // TODO: this is super dummy for now to make it work - it will be replaced by the corresponding DagState method
        self.pending_ancestors
            .iter()
            .filter_map(|(b, _)| {
                if b.round == round && authorities.contains(&b.author) {
                    Some(b)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the leaders of the provided round.
    fn leaders(&self, _round: Round) -> Vec<AuthorityIndex> {
        // TODO: this info will be retrieved from the base committers.
        vec![]
    }

    fn last_proposed_round(&self) -> Round {
        self.last_own_block.round()
    }

    fn last_proposed_block(&self) -> &VerifiedBlock {
        &self.last_own_block
    }

    fn payload_to_propose(&mut self) -> Vec<Transaction> {
        let mut total_payload_size = 0;
        let transactions_index = self
            .pending_transactions
            .iter()
            .position(|t| {
                total_payload_size += t.data().len();
                total_payload_size > MAX_BLOCK_PAYLOAD_SIZE_BYTES
            })
            .unwrap_or(self.pending_transactions.len());

        let mut payload = self.pending_transactions.split_off(transactions_index);
        mem::swap(&mut payload, &mut self.pending_transactions);

        payload
    }
}

/// Signals support a series of signals that are sent from Core when various events happen (ex new block produced).
#[allow(dead_code)]
pub(crate) struct CoreSignals {
    new_round_sender: watch::Sender<Round>,
    block_ready_sender: watch::Sender<Option<BlockRef>>,
}

impl CoreSignals {
    #[allow(dead_code)]
    pub(crate) fn new() -> (Self, CoreSignalsReceivers) {
        let (block_ready_sender, block_ready_receiver) = watch::channel(None);
        let (new_round_sender, new_round_receiver) = watch::channel(0);

        let me = Self {
            block_ready_sender,
            new_round_sender,
        };

        let receivers = CoreSignalsReceivers {
            block_ready_receiver,
            new_round_receiver,
        };

        (me, receivers)
    }

    /// Sends a signal to all the waiters that a new block has been produced.
    fn new_block_ready(&mut self, block: BlockRef) {
        self.block_ready_sender.send(Some(block)).ok();
    }

    /// Sends a signal that threshold clock has advanced to new round. The `round_number` is the round at which the
    /// threshold clock has advanced to.
    fn new_round(&mut self, round_number: Round) {
        self.new_round_sender.send(round_number).ok();
    }
}

#[allow(dead_code)]
pub(crate) struct CoreSignalsReceivers {
    block_ready_receiver: watch::Receiver<Option<BlockRef>>,
    new_round_receiver: watch::Receiver<Round>,
}

#[allow(dead_code)]
impl CoreSignalsReceivers {
    pub(crate) fn block_ready_receiver(&self) -> watch::Receiver<Option<BlockRef>> {
        self.block_ready_receiver.clone()
    }

    pub(crate) fn new_round_receiver(&self) -> watch::Receiver<Round> {
        self.new_round_receiver.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::block::TestBlock;
    use crate::transactions_client::TransactionsClient;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_core_propose_after_genesis() {
        let context = Arc::new(Context::new_for_test());
        let block_manager = BlockManager::new();
        let (_transactions_client, tx_receiver) = TransactionsClient::new(context.clone());
        let transactions_consumer = TransactionsConsumer::new(tx_receiver);
        let (signals, _signal_receivers) = CoreSignals::new();

        let mut core = Core::new(
            context.clone(),
            transactions_consumer,
            block_manager,
            signals,
        );

        // trigger the try_new_block - that should return now a new block
        let block = core
            .try_new_block(false)
            .expect("A new block should have been created");

        // A new block created - assert the details
        assert_eq!(block.round(), 1);
        assert_eq!(block.author().value(), 0);
        assert_eq!(block.ancestors().len(), 4);

        // genesis blocks should be referenced
        let (genesis_my, mut genesis_others) = Block::genesis(context);
        genesis_others.push(genesis_my);

        for ancestor in block.ancestors() {
            genesis_others
                .iter()
                .find(|block| block.reference() == *ancestor)
                .expect("Block should be found amongst genesis blocks");
        }

        // Try to propose again - with or without ignore leaders check, it will not return any block
        assert!(core.try_new_block(false).is_none());
        assert!(core.try_new_block(true).is_none());
    }

    #[tokio::test]
    async fn test_core_propose_once_receiving_a_quorum() {
        let context = Arc::new(Context::new_for_test());
        let block_manager = BlockManager::new();
        let (_transactions_client, tx_receiver) = TransactionsClient::new(context.clone());
        let transactions_consumer = TransactionsConsumer::new(tx_receiver);
        let (signals, _signal_receivers) = CoreSignals::new();

        let mut core = Core::new(
            context.clone(),
            transactions_consumer,
            block_manager,
            signals,
        );

        // Adding one block now will trigger the creation of new block for round 1
        let block_1 = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        _ = core.add_blocks(vec![block_1]);

        assert_eq!(core.last_proposed_round(), 1);
        // attempt to create a block - none will be produced.
        assert!(core.try_new_block(false).is_none());

        // Adding another block now forms a quorum for round 1, so block at round 2 will proposed
        let block_3 = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        _ = core.add_blocks(vec![block_3]);

        assert_eq!(core.last_proposed_round(), 2);

        let proposed_block = core.last_proposed_block();
        assert_eq!(proposed_block.round(), 2);
        assert_eq!(proposed_block.author(), context.own_index);
    }

    #[tokio::test]
    async fn test_core_signals() {
        let context = Arc::new(Context::new_for_test());
        let block_manager = BlockManager::new();
        let (_transactions_client, tx_receiver) = TransactionsClient::new(context.clone());
        let transactions_consumer = TransactionsConsumer::new(tx_receiver);
        let (signals, signal_receivers) = CoreSignals::new();

        let mut new_round_receiver = signal_receivers.new_round_receiver();
        let mut new_block_ready_receiver = signal_receivers.block_ready_receiver();

        let mut core = Core::new(
            context.clone(),
            transactions_consumer,
            block_manager,
            signals,
        );

        // Create multiple blocks per round to trigger block creation events.
        for round in 1..10 {
            for authority_index in 1..context.committee.size() {
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, authority_index as u32).build(),
                );

                // process them one by one to ensure that all the blocks created and signals are emitted per round.
                let _ = core.add_blocks(vec![block]);
            }

            // Check that round has advanced
            timeout(Duration::from_secs(1), new_round_receiver.changed())
                .await
                .expect("Timeout while waiting for new round be received")
                .expect("Signal receive channel shouldn't be closed");
            let new_round = new_round_receiver.borrow_and_update();
            assert_eq!(*new_round, round + 1);

            // Check that a new block has been proposed
            timeout(Duration::from_secs(1), new_block_ready_receiver.changed())
                .await
                .expect("Timeout while waiting for new block signal")
                .expect("Signal receive channel shouldn't be closed");
            let block_ref = new_block_ready_receiver.borrow_and_update();
            let block_ref = block_ref.unwrap();
            assert_eq!(block_ref.round, round + 1);
            assert_eq!(block_ref.author, context.own_index);
        }
    }
}

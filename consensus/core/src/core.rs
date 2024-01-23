// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block_manager::BlockManager;
use crate::transactions_client::TransactionsConsumer;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::watch;

use crate::{
    block::{Block, BlockAPI, BlockRef, BlockV1, Round, Transaction, VerifiedBlock},
    context::Context,
    threshold_clock::ThresholdClock,
};

use mysten_metrics::monitored_scope;

#[allow(dead_code)]
pub(crate) struct Core {
    context: Arc<Context>,
    /// The threshold clock that is used to keep track of the current round
    threshold_clock: ThresholdClock,
    /// The last produced block
    last_own_block: Block,
    /// The consumer to use in order to pull transactions to be included for the next proposals
    transactions_consumer: TransactionsConsumer,
    /// The transactions that haven't been proposed yet and are to be included in the next proposal
    pending_transactions: VecDeque<Transaction>,
    /// The pending blocks refs to be included as ancestors to the next block
    pending_ancestors: VecDeque<BlockRef>,
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
        let threshold_clock = ThresholdClock::new(0, context.clone());

        Self {
            context,
            threshold_clock,
            last_own_block: Block::V1(BlockV1::default()), // TODO: restore on crash/recovery
            transactions_consumer,
            pending_transactions: VecDeque::new(),
            pending_ancestors: VecDeque::new(),
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
            self.pending_ancestors.push_back(block.reference());
        }

        // Pull transactions to be proposed and append to the pending list
        for transaction in self.transactions_consumer.next() {
            self.pending_transactions.push_back(transaction);
        }

        // Attempt to create a new block
        let _ = self.try_new_block(false);

        // TODO: we don't deal for now with missed references, will address later.
        vec![]
    }

    /// Force creating a new block for the dictated round. This is used when a leader timeout occurs.
    pub fn force_new_block(&mut self, round: Round) -> Option<VerifiedBlock> {
        if self.last_proposed_round() < round {
            self.context.metrics.node_metrics.leader_timeout_total.inc();
            self.try_new_block(true)
        } else {
            None
        }
    }

    /// Attempts to propose a new block for the next round. If a block has already proposed for latest
    /// or earlier round, then no block is created and None is returned.
    pub(crate) fn try_new_block(&mut self, force_new_block: bool) -> Option<VerifiedBlock> {
        let _scope = monitored_scope("Core::try_new_block");

        let clock_round = self.threshold_clock.get_round();
        if clock_round <= self.last_proposed_round() {
            return None;
        }

        // create a new block either because we want to "forcefully" propose a block due to a leader timeout,
        // or because we are actually ready to produce the block (leader exists)
        if force_new_block || self.ready_new_block() {
            // TODO: produce the block for the clock_round. As the threshold clock can advance many rounds at once (ex
            // because we synchronized a bulk of blocks) we can decide here whether we want to produce blocks per round
            // or just the latest one. From earlier experiments I saw only benefit on proposing for the penultimate round
            // only when the validator was supposed to be the leader of the round - so we bring down the missed leaders.
            // Probably proposing for all the intermediate rounds might not make much sense.

            // consume the next transactions to be included

            // consume the next ancestors to be included

            // create the block and insert to storage.
            // TODO: take a decision on whether we want to flush to disk at this point the DagState.

            // emit an event that a new block is ready
            // TODO: replace the default with the actual block ref.
            self.signals.new_block_ready(BlockRef::default());
        }

        None
    }

    fn ready_new_block(&self) -> bool {
        // TODO: check that we are ready to produce a new block. This will mainly check that the leader of the previous
        // quorum exists.
        true
    }

    fn last_proposed_round(&self) -> Round {
        self.last_own_block.round()
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
    use crate::transactions_client::TransactionsClient;
    use consensus_config::AuthorityIndex;

    #[tokio::test]
    async fn test_core() {
        let context = Arc::new(Context::new_for_test());
        let block_manager = BlockManager::new();
        let (_transactions_client, tx_receiver) = TransactionsClient::new(context.clone());
        let transactions_consumer = TransactionsConsumer::new(tx_receiver);
        let (signals, signal_receivers) = CoreSignals::new();

        let mut new_round_receiver = signal_receivers.new_round_receiver();

        let mut core = Core::new(context, transactions_consumer, block_manager, signals);

        assert_eq!(core.last_proposed_round(), 0);

        // Add a few blocks which will get accepted
        let block_1 = BlockV1::new(1, AuthorityIndex::new_for_test(0), 0, vec![]);
        let block_2 = BlockV1::new(1, AuthorityIndex::new_for_test(1), 0, vec![]);
        let block_3 = BlockV1::new(1, AuthorityIndex::new_for_test(2), 0, vec![]);

        let blocks = vec![block_1, block_2, block_3]
            .into_iter()
            .map(|b| VerifiedBlock::new_for_test(Block::V1(b)))
            .collect();

        // Process them via Core
        _ = core.add_blocks(blocks);

        // Check that round has advanced
        assert!(new_round_receiver.changed().await.is_ok());
        let new_round = new_round_receiver.borrow_and_update();
        assert_eq!(*new_round, 2);

        // And that core is also on same round on its threshold clock
        assert_eq!(core.threshold_clock.get_round(), 2);
    }
}

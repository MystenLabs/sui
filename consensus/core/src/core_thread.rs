// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    fmt::Debug,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use mysten_metrics::{
    monitored_mpsc::{channel, Receiver, Sender, WeakSender},
    monitored_scope, spawn_logged_monitored_task,
};
use parking_lot::{Mutex, RwLock};
use thiserror::Error;
use tokio::sync::{oneshot, watch};
use tracing::warn;

use crate::{
    block::{BlockRef, Round, VerifiedBlock},
    context::Context,
    core::Core,
    core_thread::CoreError::Shutdown,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    BlockAPI as _,
};

const CORE_THREAD_COMMANDS_CHANNEL_SIZE: usize = 2000;

enum CoreThreadCommand {
    /// Add blocks to be processed and accepted
    AddBlocks(Vec<VerifiedBlock>, oneshot::Sender<BTreeSet<BlockRef>>),
    /// Called when the min round has passed or the leader timeout occurred and a block should be produced.
    /// When the command is called with `force = true`, then the block will be created for `round` skipping
    /// any checks (ex leader existence of previous round). More information can be found on the `Core` component.
    NewBlock(Round, oneshot::Sender<()>, bool),
    /// Request missing blocks that need to be synced.
    GetMissing(oneshot::Sender<BTreeSet<BlockRef>>),
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Core thread shutdown: {0}")]
    Shutdown(String),
}

/// The interface to dispatch commands to CoreThread and Core.
/// Also this allows the easier mocking during unit tests.
#[async_trait]
pub trait CoreThreadDispatcher: Sync + Send + 'static {
    async fn add_blocks(&self, blocks: Vec<VerifiedBlock>)
        -> Result<BTreeSet<BlockRef>, CoreError>;

    async fn new_block(&self, round: Round, force: bool) -> Result<(), CoreError>;

    async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError>;

    /// Informs the core whether consumer of produced blocks exists.
    /// This is only used by core to decide if it should propose new blocks.
    /// It is not a guarantee that produced blocks will be accepted by peers.
    fn set_subscriber_exists(&self, exists: bool) -> Result<(), CoreError>;

    /// Sets the estimated delay to propagate a block to a quorum of peers, in number of rounds.
    fn set_propagation_delay(&self, delay: Round) -> Result<(), CoreError>;

    fn set_last_known_proposed_round(&self, round: Round) -> Result<(), CoreError>;

    /// Returns the highest round received for each authority by Core.
    fn highest_received_rounds(&self) -> Vec<Round>;
}

pub(crate) struct CoreThreadHandle {
    sender: Sender<CoreThreadCommand>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl CoreThreadHandle {
    pub async fn stop(self) {
        // drop the sender, that will force all the other weak senders to not able to upgrade.
        drop(self.sender);
        self.join_handle.await.ok();
    }
}

struct CoreThread {
    core: Core,
    receiver: Receiver<CoreThreadCommand>,
    rx_subscriber_exists: watch::Receiver<bool>,
    rx_propagation_delay: watch::Receiver<Round>,
    rx_last_known_proposed_round: watch::Receiver<Round>,
    context: Arc<Context>,
}

impl CoreThread {
    pub async fn run(mut self) -> ConsensusResult<()> {
        tracing::debug!("Started core thread");

        loop {
            tokio::select! {
                command = self.receiver.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    self.context.metrics.node_metrics.core_lock_dequeued.inc();
                    match command {
                        CoreThreadCommand::AddBlocks(blocks, sender) => {
                            let _scope = monitored_scope("CoreThread::loop::add_blocks");
                            let missing_blocks = self.core.add_blocks(blocks)?;
                            sender.send(missing_blocks).ok();
                        }
                        CoreThreadCommand::NewBlock(round, sender, force) => {
                            let _scope = monitored_scope("CoreThread::loop::new_block");
                            self.core.new_block(round, force)?;
                            sender.send(()).ok();
                        }
                        CoreThreadCommand::GetMissing(sender) => {
                            let _scope = monitored_scope("CoreThread::loop::get_missing");
                            sender.send(self.core.get_missing_blocks()).ok();
                        }
                    }
                }
                _ = self.rx_last_known_proposed_round.changed() => {
                    let _scope = monitored_scope("CoreThread::loop::set_last_known_proposed_round");
                    let round = *self.rx_last_known_proposed_round.borrow();
                    self.core.set_last_known_proposed_round(round);
                    self.core.new_block(round + 1, true)?;
                }
                _ = self.rx_subscriber_exists.changed() => {
                    let _scope = monitored_scope("CoreThread::loop::set_subscriber_exists");
                    let should_propose_before = self.core.should_propose();
                    let exists = *self.rx_subscriber_exists.borrow();
                    self.core.set_subscriber_exists(exists);
                    if !should_propose_before && self.core.should_propose() {
                        // If core cannnot propose before but can propose now, try to produce a new block to ensure liveness,
                        // because block proposal could have been skipped.
                        self.core.new_block(Round::MAX, true)?;
                    }
                }
                _ = self.rx_propagation_delay.changed() => {
                    let _scope = monitored_scope("CoreThread::loop::set_propagation_delay");
                    let should_propose_before = self.core.should_propose();
                    let delay = *self.rx_propagation_delay.borrow();
                    self.core.set_propagation_delay(delay);
                    if !should_propose_before && self.core.should_propose() {
                        // If core cannnot propose before but can propose now, try to produce a new block to ensure liveness,
                        // because block proposal could have been skipped.
                        self.core.new_block(Round::MAX, true)?;
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct ChannelCoreThreadDispatcher {
    context: Arc<Context>,
    sender: WeakSender<CoreThreadCommand>,
    tx_subscriber_exists: Arc<watch::Sender<bool>>,
    tx_propagation_delay: Arc<watch::Sender<Round>>,
    tx_last_known_proposed_round: Arc<watch::Sender<Round>>,
    highest_received_rounds: Arc<Vec<AtomicU32>>,
}

impl ChannelCoreThreadDispatcher {
    pub(crate) fn start(
        context: Arc<Context>,
        dag_state: &RwLock<DagState>,
        core: Core,
    ) -> (Self, CoreThreadHandle) {
        // Initialize highest received rounds to last accepted rounds.
        let highest_received_rounds = {
            let dag_state = dag_state.read();
            context
                .committee
                .authorities()
                .map(|(index, _)| {
                    AtomicU32::new(dag_state.get_last_block_for_authority(index).round())
                })
                .collect()
        };

        let (sender, receiver) =
            channel("consensus_core_commands", CORE_THREAD_COMMANDS_CHANNEL_SIZE);
        let (tx_subscriber_exists, mut rx_subscriber_exists) = watch::channel(false);
        let (tx_propagation_delay, mut rx_propagation_delay) = watch::channel(0);
        let (tx_last_known_proposed_round, mut rx_last_known_proposed_round) = watch::channel(0);
        rx_subscriber_exists.mark_unchanged();
        rx_propagation_delay.mark_unchanged();
        rx_last_known_proposed_round.mark_unchanged();
        let core_thread = CoreThread {
            core,
            receiver,
            rx_subscriber_exists,
            rx_propagation_delay,
            rx_last_known_proposed_round,
            context: context.clone(),
        };

        let join_handle = spawn_logged_monitored_task!(
            async move {
                if let Err(err) = core_thread.run().await {
                    if !matches!(err, ConsensusError::Shutdown) {
                        panic!("Fatal error occurred: {err}");
                    }
                }
            },
            "ConsensusCoreThread"
        );

        // Explicitly using downgraded sender in order to allow sharing the CoreThreadDispatcher but
        // able to shutdown the CoreThread by dropping the original sender.
        let dispatcher = ChannelCoreThreadDispatcher {
            context,
            sender: sender.downgrade(),
            tx_subscriber_exists: Arc::new(tx_subscriber_exists),
            tx_propagation_delay: Arc::new(tx_propagation_delay),
            tx_last_known_proposed_round: Arc::new(tx_last_known_proposed_round),
            highest_received_rounds: Arc::new(highest_received_rounds),
        };
        let handle = CoreThreadHandle {
            join_handle,
            sender,
        };
        (dispatcher, handle)
    }

    async fn send(&self, command: CoreThreadCommand) {
        self.context.metrics.node_metrics.core_lock_enqueued.inc();
        if let Some(sender) = self.sender.upgrade() {
            if let Err(err) = sender.send(command).await {
                warn!(
                    "Couldn't send command to core thread, probably is shutting down: {}",
                    err
                );
            }
        }
    }
}

#[async_trait]
impl CoreThreadDispatcher for ChannelCoreThreadDispatcher {
    async fn add_blocks(
        &self,
        blocks: Vec<VerifiedBlock>,
    ) -> Result<BTreeSet<BlockRef>, CoreError> {
        for block in &blocks {
            self.highest_received_rounds[block.author()].fetch_max(block.round(), Ordering::AcqRel);
        }
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::AddBlocks(blocks, sender))
            .await;
        receiver.await.map_err(|e| Shutdown(e.to_string()))
    }

    async fn new_block(&self, round: Round, force: bool) -> Result<(), CoreError> {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::NewBlock(round, sender, force))
            .await;
        receiver.await.map_err(|e| Shutdown(e.to_string()))
    }

    async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::GetMissing(sender)).await;
        receiver.await.map_err(|e| Shutdown(e.to_string()))
    }

    fn set_subscriber_exists(&self, exists: bool) -> Result<(), CoreError> {
        self.tx_subscriber_exists
            .send(exists)
            .map_err(|e| Shutdown(e.to_string()))
    }

    fn set_propagation_delay(&self, delay: Round) -> Result<(), CoreError> {
        self.tx_propagation_delay
            .send(delay)
            .map_err(|e| Shutdown(e.to_string()))
    }

    fn set_last_known_proposed_round(&self, round: Round) -> Result<(), CoreError> {
        self.tx_last_known_proposed_round
            .send(round)
            .map_err(|e| Shutdown(e.to_string()))
    }

    fn highest_received_rounds(&self) -> Vec<Round> {
        self.highest_received_rounds
            .iter()
            .map(|round| round.load(Ordering::Relaxed))
            .collect()
    }
}

// TODO: complete the Mock for thread dispatcher to be used from several tests
#[derive(Default)]
pub(crate) struct MockCoreThreadDispatcher {
    add_blocks: Mutex<Vec<VerifiedBlock>>,
    missing_blocks: Mutex<BTreeSet<BlockRef>>,
    last_known_proposed_round: Mutex<Vec<Round>>,
}

impl MockCoreThreadDispatcher {
    #[cfg(test)]
    pub(crate) async fn get_add_blocks(&self) -> Vec<VerifiedBlock> {
        let mut add_blocks = self.add_blocks.lock();
        add_blocks.drain(0..).collect()
    }

    #[cfg(test)]
    pub(crate) async fn stub_missing_blocks(&self, block_refs: BTreeSet<BlockRef>) {
        let mut missing_blocks = self.missing_blocks.lock();
        missing_blocks.extend(block_refs);
    }

    #[cfg(test)]
    pub(crate) async fn get_last_own_proposed_round(&self) -> Vec<Round> {
        let last_known_proposed_round = self.last_known_proposed_round.lock();
        last_known_proposed_round.clone()
    }
}

#[async_trait]
impl CoreThreadDispatcher for MockCoreThreadDispatcher {
    async fn add_blocks(
        &self,
        blocks: Vec<VerifiedBlock>,
    ) -> Result<BTreeSet<BlockRef>, CoreError> {
        let mut add_blocks = self.add_blocks.lock();
        add_blocks.extend(blocks);
        Ok(BTreeSet::new())
    }

    async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
        Ok(())
    }

    async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
        let mut missing_blocks = self.missing_blocks.lock();
        let result = missing_blocks.clone();
        missing_blocks.clear();
        Ok(result)
    }

    fn set_subscriber_exists(&self, _exists: bool) -> Result<(), CoreError> {
        todo!()
    }

    fn set_propagation_delay(&self, _delay: Round) -> Result<(), CoreError> {
        todo!()
    }

    fn set_last_known_proposed_round(&self, round: Round) -> Result<(), CoreError> {
        let mut last_known_proposed_round = self.last_known_proposed_round.lock();
        last_known_proposed_round.push(round);
        Ok(())
    }

    fn highest_received_rounds(&self) -> Vec<Round> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use mysten_metrics::monitored_mpsc::unbounded_channel;
    use parking_lot::RwLock;

    use super::*;
    use crate::{
        block_manager::BlockManager,
        block_verifier::NoopBlockVerifier,
        commit_observer::CommitObserver,
        context::Context,
        core::CoreSignals,
        dag_state::DagState,
        leader_schedule::LeaderSchedule,
        storage::mem_store::MemStore,
        transaction::{TransactionClient, TransactionConsumer},
        CommitConsumer,
    };

    #[tokio::test]
    async fn test_core_thread() {
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
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let (signals, signal_receivers) = CoreSignals::new(context.clone());
        let _block_receiver = signal_receivers.block_broadcast_receiver();
        let (sender, _receiver) = unbounded_channel("consensus_output");
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let commit_observer = CommitObserver::new(
            context.clone(),
            CommitConsumer::new(sender.clone(), 0),
            dag_state.clone(),
            store,
            leader_schedule.clone(),
        );
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));
        let core = Core::new(
            context.clone(),
            leader_schedule,
            transaction_consumer,
            block_manager,
            true,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state.clone(),
            false,
        );

        let (core_dispatcher, handle) =
            ChannelCoreThreadDispatcher::start(context, &dag_state, core);

        // Now create some clones of the dispatcher
        let dispatcher_1 = core_dispatcher.clone();
        let dispatcher_2 = core_dispatcher.clone();

        // Try to send some commands
        assert!(dispatcher_1.add_blocks(vec![]).await.is_ok());
        assert!(dispatcher_2.add_blocks(vec![]).await.is_ok());

        // Now shutdown the dispatcher
        handle.stop().await;

        // Try to send some commands
        assert!(dispatcher_1.add_blocks(vec![]).await.is_err());
        assert!(dispatcher_2.add_blocks(vec![]).await.is_err());
    }
}

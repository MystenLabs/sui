// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fmt::Debug, sync::Arc, thread};

use async_trait::async_trait;
use mysten_metrics::{metered_channel, monitored_scope};
use thiserror::Error;
use tokio::sync::{oneshot, oneshot::error::RecvError};
use tracing::warn;

use crate::{
    block::{BlockRef, Round, VerifiedBlock},
    context::Context,
    core::Core,
    core_thread::CoreError::Shutdown,
};

const CORE_THREAD_COMMANDS_CHANNEL_SIZE: usize = 32;

enum CoreThreadCommand {
    /// Add blocks to be processed and accepted
    AddBlocks(Vec<VerifiedBlock>, oneshot::Sender<BTreeSet<BlockRef>>),
    /// Called when a leader timeout occurs and a block should be produced
    ForceNewBlock(Round, oneshot::Sender<()>),
    /// Request missing blocks that need to be synced.
    GetMissing(oneshot::Sender<BTreeSet<BlockRef>>),
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Core thread shutdown: {0}")]
    Shutdown(RecvError),
}

/// The interface to dispatch commands to CoreThread and Core.
/// Also this allows the easier mocking during unit tests.
#[async_trait]
pub trait CoreThreadDispatcher: Sync + Send + 'static {
    async fn add_blocks(&self, blocks: Vec<VerifiedBlock>)
        -> Result<BTreeSet<BlockRef>, CoreError>;

    async fn force_new_block(&self, round: Round) -> Result<(), CoreError>;

    async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError>;
}

#[allow(unused)]
pub(crate) struct CoreThreadHandle {
    sender: metered_channel::Sender<CoreThreadCommand>,
    join_handle: thread::JoinHandle<()>,
}

impl CoreThreadHandle {
    #[allow(unused)]
    pub fn stop(self) {
        // drop the sender, that will force all the other weak senders to not able to upgrade.
        drop(self.sender);
        self.join_handle.join().ok();
    }
}

#[allow(unused)]
struct CoreThread {
    core: Core,
    receiver: metered_channel::Receiver<CoreThreadCommand>,
    context: Arc<Context>,
}

impl CoreThread {
    pub fn run(mut self) {
        tracing::debug!("Started core thread");

        while let Some(command) = self.receiver.blocking_recv() {
            let _scope = monitored_scope("CoreThread::loop");
            self.context.metrics.node_metrics.core_lock_dequeued.inc();
            match command {
                CoreThreadCommand::AddBlocks(blocks, sender) => {
                    let missing_blocks = self.core.add_blocks(blocks);
                    sender.send(missing_blocks).ok();
                }
                CoreThreadCommand::ForceNewBlock(round, sender) => {
                    self.core.force_new_block(round);
                    sender.send(()).ok();
                }
                CoreThreadCommand::GetMissing(sender) => {
                    sender.send(self.core.get_missing_blocks()).ok();
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ChannelCoreThreadDispatcher {
    sender: metered_channel::WeakSender<CoreThreadCommand>,
    context: Arc<Context>,
}

impl ChannelCoreThreadDispatcher {
    pub(crate) fn start(core: Core, context: Arc<Context>) -> (Self, CoreThreadHandle) {
        let (sender, receiver) = metered_channel::channel_with_total(
            CORE_THREAD_COMMANDS_CHANNEL_SIZE,
            &context.metrics.channel_metrics.core_thread,
            &context.metrics.channel_metrics.core_thread_total,
        );
        let core_thread = CoreThread {
            core,
            receiver,
            context: context.clone(),
        };
        let join_handle = thread::Builder::new()
            .name("consensus-core".to_string())
            .spawn(move || core_thread.run())
            .unwrap();
        // Explicitly using downgraded sender in order to allow sharing the CoreThreadDispatcher but
        // able to shutdown the CoreThread by dropping the original sender.
        let dispatcher = ChannelCoreThreadDispatcher {
            sender: sender.downgrade(),
            context,
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
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::AddBlocks(blocks, sender))
            .await;
        receiver.await.map_err(Shutdown)
    }

    async fn force_new_block(&self, round: Round) -> Result<(), CoreError> {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::ForceNewBlock(round, sender))
            .await;
        receiver.await.map_err(Shutdown)
    }

    async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::GetMissing(sender)).await;
        receiver.await.map_err(Shutdown)
    }
}

#[cfg(test)]
mod test {
    use parking_lot::RwLock;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;
    use crate::{
        block_manager::BlockManager,
        commit_observer::CommitObserver,
        context::Context,
        core::CoreSignals,
        dag_state::DagState,
        storage::mem_store::MemStore,
        transaction::{TransactionClient, TransactionConsumer},
    };

    #[tokio::test]
    async fn test_core_thread() {
        telemetry_subscribers::init_for_testing();
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(context.clone(), dag_state.clone());
        let (_transaction_client, tx_receiver) = TransactionClient::new(context.clone());
        let transaction_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);
        let (signals, _signal_receivers) = CoreSignals::new();

        let (sender, _receiver) = unbounded_channel();
        let commit_observer = CommitObserver::new(
            context.clone(),
            sender.clone(),
            0, // last_processed_index
            dag_state.clone(),
            store.clone(),
        );
        let core = Core::new(
            context.clone(),
            transaction_consumer,
            block_manager,
            commit_observer,
            signals,
            key_pairs.remove(context.own_index.value()).1,
            dag_state,
            store,
        );

        let (core_dispatcher, handle) = ChannelCoreThreadDispatcher::start(core, context);

        // Now create some clones of the dispatcher
        let dispatcher_1 = core_dispatcher.clone();
        let dispatcher_2 = core_dispatcher.clone();

        // Try to send some commands
        assert!(dispatcher_1.add_blocks(vec![]).await.is_ok());
        assert!(dispatcher_2.add_blocks(vec![]).await.is_ok());

        // Now shutdown the dispatcher
        handle.stop();

        // Try to send some commands
        assert!(dispatcher_1.add_blocks(vec![]).await.is_err());
        assert!(dispatcher_2.add_blocks(vec![]).await.is_err());
    }
}

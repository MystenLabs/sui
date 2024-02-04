// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use mysten_metrics::{metered_channel, monitored_scope};
use std::{collections::HashSet, fmt::Debug, sync::Arc, thread};
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;
use tracing::warn;

use crate::{
    block::{BlockRef, Round, VerifiedBlock},
    context::Context,
    core::Core,
    core_thread::CoreError::Shutdown,
};

const CORE_THREAD_COMMANDS_CHANNEL_SIZE: usize = 32;

/// The interface to adhere the implementations of the core thread dispatcher. Also allows the easier mocking during unit tests.
#[async_trait]
pub(crate) trait CoreThreadDispatcherInterface: Sync + Send + 'static {
    async fn add_blocks(&self, blocks: Vec<VerifiedBlock>) -> Result<Vec<BlockRef>, CoreError>;

    async fn force_new_block(&self, round: Round) -> Result<(), CoreError>;

    async fn get_missing_blocks(&self) -> Result<Vec<HashSet<BlockRef>>, CoreError>;
}

#[allow(unused)]
pub(crate) struct CoreThreadDispatcherHandle {
    sender: metered_channel::Sender<CoreThreadCommand>,
    join_handle: thread::JoinHandle<()>,
}

impl CoreThreadDispatcherHandle {
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
                    // TODO: implement the logic to fetch the missing blocks.
                    sender.send(vec![]).ok();
                }
            }
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct CoreThreadDispatcher {
    sender: metered_channel::WeakSender<CoreThreadCommand>,
    context: Arc<Context>,
}

enum CoreThreadCommand {
    /// Add blocks to be processed and accepted
    AddBlocks(Vec<VerifiedBlock>, oneshot::Sender<Vec<BlockRef>>),
    /// Called when a leader timeout occurs and a block should be produced
    ForceNewBlock(Round, oneshot::Sender<()>),
    /// Request missing blocks that need to be synced.
    GetMissing(oneshot::Sender<Vec<HashSet<BlockRef>>>),
}

#[derive(Error, Debug)]
pub(crate) enum CoreError {
    #[error("Core thread shutdown: {0}")]
    Shutdown(RecvError),
}

impl CoreThreadDispatcher {
    pub fn start(core: Core, context: Arc<Context>) -> (Self, CoreThreadDispatcherHandle) {
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
        let dispatcher = CoreThreadDispatcher {
            sender: sender.downgrade(),
            context,
        };
        let handler = CoreThreadDispatcherHandle {
            join_handle,
            sender,
        };
        (dispatcher, handler)
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
#[allow(unused)]
impl CoreThreadDispatcherInterface for CoreThreadDispatcher {
    async fn add_blocks(&self, blocks: Vec<VerifiedBlock>) -> Result<Vec<BlockRef>, CoreError> {
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

    async fn get_missing_blocks(&self) -> Result<Vec<HashSet<BlockRef>>, CoreError> {
        let (sender, receiver) = oneshot::channel();
        self.send(CoreThreadCommand::GetMissing(sender)).await;
        receiver.await.map_err(Shutdown)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::block_manager::BlockManager;
    use crate::context::Context;
    use crate::core::CoreSignals;
    use crate::transactions_client::{TransactionsClient, TransactionsConsumer};

    #[tokio::test]
    async fn test_core_thread() {
        let (context, mut key_pairs) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_manager = BlockManager::new();
        let (_transactions_client, tx_receiver) = TransactionsClient::new(context.clone());
        let transactions_consumer = TransactionsConsumer::new(tx_receiver, context.clone(), None);
        let (signals, _signal_receivers) = CoreSignals::new();
        let core = Core::new(
            context.clone(),
            transactions_consumer,
            block_manager,
            signals,
            key_pairs.remove(context.own_index.value()).0,
        );

        let (core_dispatcher, handle) = CoreThreadDispatcher::start(core, context);

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

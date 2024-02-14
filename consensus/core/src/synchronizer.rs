// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::BlockRef;
use crate::context::Context;
use crate::core_thread::ChannelCoreThreadDispatcher;
use consensus_config::AuthorityIndex;
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinSet;

#[derive(Error, Debug)]
pub(crate) enum SynchronizerError {
    #[error("Synchronizer shutdown")]
    Shutdown,
    #[error("Synchronizer is full for peer with index: {0}")]
    SynchronizerFull(AuthorityIndex),
}

enum Command {
    FetchBlocks {
        missing_block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
        send_result: oneshot::Sender<Result<(), SynchronizerError>>,
    },
}

#[allow(dead_code)]
pub(crate) struct SynchronizerHandle {
    commands_sender: Sender<Command>,
    tasks: Mutex<JoinSet<()>>,
}

impl SynchronizerHandle {
    /// Explicitly asks from the synchronizer to fetch the blocks - provided the block_refs set - from
    /// the peer authority.
    pub async fn fetch_blocks(
        &self,
        block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
    ) -> Result<(), SynchronizerError> {
        let (sender, receiver) = oneshot::channel();
        self.commands_sender
            .send(Command::FetchBlocks {
                missing_block_refs: block_refs,
                peer_index,
                send_result: sender,
            })
            .await
            .map_err(|_err| SynchronizerError::Shutdown)?;
        receiver.await.map_err(|_err| SynchronizerError::Shutdown)?
    }

    pub async fn stop(&self) {
        let mut tasks = self.tasks.lock();
        tasks.abort_all();
    }
}

#[allow(dead_code)]
pub(crate) struct Synchronizer {
    context: Arc<Context>,
    core_thread_dispatcher: ChannelCoreThreadDispatcher,
    commands_receiver: Receiver<Command>,
    fetch_block_senders: Vec<Sender<BTreeSet<BlockRef>>>,
}

impl Synchronizer {
    pub fn start(
        context: Arc<Context>,
        core_thread_dispatcher: ChannelCoreThreadDispatcher,
    ) -> Arc<SynchronizerHandle> {
        let (commands_sender, commands_receiver) = channel(1_000);

        // Spawn the tasks to fetch the blocks from the others
        let mut fetch_block_senders = Vec::with_capacity(context.committee.size());
        let mut tasks = JoinSet::new();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            let (sender, receiver) = channel(5);
            tasks.spawn(Self::fetch_blocks_from_authority(receiver, index));
            fetch_block_senders[index.value()] = sender;
        }

        // Spawn the task to listen to the requests & periodic runs
        tasks.spawn(async {
            let mut s = Self {
                context,
                core_thread_dispatcher,
                commands_receiver,
                fetch_block_senders,
            };
            s.run().await;
        });

        Arc::new(SynchronizerHandle {
            commands_sender,
            tasks: Mutex::new(tasks),
        })
    }

    async fn fetch_blocks_from_authority(
        _receiver: Receiver<BTreeSet<BlockRef>>,
        _authority: AuthorityIndex,
    ) {
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(command) = self.commands_receiver.recv() => {
                    match command {
                        Command::FetchBlocks{ missing_block_refs, peer_index, send_result } => {
                            assert_ne!(peer_index, self.context.own_index, "We should never attempt to fetch blocks from our own node");

                            if let Err(err) = self.fetch_block_senders[peer_index].try_send(missing_block_refs) {
                                match err {
                                    TrySendError::Full(_) => send_result.send(Err(SynchronizerError::SynchronizerFull(peer_index))).ok(),
                                    TrySendError::Closed(_) => send_result.send(Err(SynchronizerError::Shutdown)).ok()
                                };
                            } else {
                                send_result.send(Ok(())).ok();
                            }
                        }
                    }
                }
            }
        }
    }
}

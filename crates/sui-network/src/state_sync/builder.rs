// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use sui_config::p2p::StateSyncConfig;
use sui_types::{messages_checkpoint::VerifiedCheckpoint, storage::ReadStore};
use tap::Pipe;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinSet,
};

use super::{
    server::Server, Handle, PeerHeights, StateSync, StateSyncEventLoop, StateSyncMessage,
    StateSyncServer,
};
use sui_types::storage::WriteStore;

pub struct Builder<S> {
    store: Option<S>,
    config: Option<StateSyncConfig>,
}

impl Builder<()> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            store: None,
            config: None,
        }
    }
}

impl<S> Builder<S> {
    pub fn store<NewStore>(self, store: NewStore) -> Builder<NewStore> {
        Builder {
            store: Some(store),
            config: self.config,
        }
    }

    pub fn config(mut self, config: StateSyncConfig) -> Self {
        self.config = Some(config);
        self
    }
}

impl<S> Builder<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    pub fn build(self) -> (UnstartedStateSync<S>, StateSyncServer<impl StateSync>) {
        let (builder, server) = self.build_internal();
        (builder, StateSyncServer::new(server))
    }

    pub(super) fn build_internal(self) -> (UnstartedStateSync<S>, Server<S>) {
        let Builder { store, config } = self;
        let store = store.unwrap();
        let config = config.unwrap_or_default();

        let (sender, mailbox) = mpsc::channel(config.mailbox_capacity());
        let (checkpoint_event_sender, _reciever) =
            broadcast::channel(config.synced_checkpoint_broadcast_channel_capacity());
        let weak_sender = sender.downgrade();
        let handle = Handle {
            sender,
            checkpoint_event_sender: checkpoint_event_sender.clone(),
        };
        let peer_heights = PeerHeights {
            heights: HashMap::new(),
            unprocessed_checkpoints: HashMap::new(),
            sequence_number_to_digest: HashMap::new(),
        }
        .pipe(RwLock::new)
        .pipe(Arc::new);

        let server = Server {
            store: store.clone(),
            peer_heights: peer_heights.clone(),
            sender: weak_sender,
        };

        (
            UnstartedStateSync {
                config,
                handle,
                mailbox,
                store,
                peer_heights,
                checkpoint_event_sender,
            },
            server,
        )
    }
}

pub struct UnstartedStateSync<S> {
    pub(super) config: StateSyncConfig,
    pub(super) handle: Handle,
    pub(super) mailbox: mpsc::Receiver<StateSyncMessage>,
    pub(super) store: S,
    pub(super) peer_heights: Arc<RwLock<PeerHeights>>,
    pub(super) checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
}

impl<S> UnstartedStateSync<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    pub(super) fn build(self, network: anemo::Network) -> (StateSyncEventLoop<S>, Handle) {
        let Self {
            config,
            handle,
            mailbox,
            store,
            peer_heights,
            checkpoint_event_sender,
        } = self;

        (
            StateSyncEventLoop {
                config,
                mailbox,
                weak_sender: handle.sender.downgrade(),
                tasks: JoinSet::new(),
                sync_checkpoint_summaries_task: None,
                sync_checkpoint_contents_task: None,
                store,
                peer_heights,
                checkpoint_event_sender,
                network,
            },
            handle,
        )
    }

    pub fn start(self, network: anemo::Network) -> Handle {
        let (event_loop, handle) = self.build(network);
        tokio::spawn(event_loop.start());

        handle
    }
}

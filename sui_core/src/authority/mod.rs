// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod authority_core; // TODO: Does only need to be public for tests.
mod authority_server;
pub mod authority_store; // TODO: Does only need to be public for tests.
mod temporary_store;

use async_trait::async_trait;
use authority_core::AuthorityState;
use authority_server::AuthorityServer;
use authority_store::AuthorityStore;
use bytes::Bytes;
use futures::SinkExt;
use move_binary_format::CompiledModule;
use network::receiver::{MessageHandler, Receiver as NetworkReceiver, Writer};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_types::base_types::TxContext;
use sui_types::committee::Committee;
use sui_types::crypto::KeyPair;
use sui_types::error::SuiError;
use sui_types::messages::{AuthorityToClientCoreMessage, ClientToAuthorityCoreMessage};
use sui_types::object::Object;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

/// The default size of inter-tasks channels.
pub(crate) const DEFAULT_CHANNEL_CAPACITY: usize = 1_000;

/// One-shot channel allowing the core to reply to core messages.
pub(crate) type Replier = oneshot::Sender<AuthorityToClientCoreMessage>;

/// Spawn a new authority.
pub async fn spawn_authority(
    key_pair: &KeyPair,
    committee: Committee,
    db_path: &str,
    mut address: SocketAddr,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    genesis_ctx: &mut TxContext,
    rx_consensus: Receiver<Bytes>,
) {
    let (tx_client_core_message, rx_client_core_message) = channel(DEFAULT_CHANNEL_CAPACITY);

    // Create or open a new persistent storage.
    let store = Arc::new(AuthorityStore::open(&db_path, None));

    // Make the authority state and preload the initial objects.
    let name = *key_pair.public_key_bytes();
    let state = AuthorityState::new(
        committee,
        name,
        /* secret */ Box::pin(key_pair.copy()),
        store,
        preload_modules,
        genesis_ctx,
    )
    .await;

    for object in preload_objects {
        state
            .init_transaction_lock(object.to_object_reference())
            .await;
        state.insert_object(object.clone()).await;
    }

    // Spawn a network receiver.
    address.set_ip("0.0.0.0".parse().unwrap());
    let handler = AuthorityHandler {
        tx_client_core_message,
    };
    NetworkReceiver::spawn(address, handler);
    log::info!(
        "Authority {:?} successfully booted on {}",
        name,
        address.ip()
    );

    // Spawn the authority server.
    AuthorityServer::spawn(state, rx_client_core_message, rx_consensus)
        .await
        .unwrap();
}

/// Define how the network receiver handles incoming messages.
#[derive(Clone)]
struct AuthorityHandler {
    tx_client_core_message: Sender<(ClientToAuthorityCoreMessage, Replier)>,
}

#[async_trait]
impl MessageHandler for AuthorityHandler {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        let (sender, receiver) = oneshot::channel();

        // Deserialize and parse the message.
        let message = bincode::deserialize(&serialized).map_err(SuiError::from)?;
        self.tx_client_core_message
            .send((message, sender))
            .await
            .expect("Failed to send message to core");

        // Reply to the sender.
        let reply = receiver.await.expect("Failed to receive message reply");
        let bytes = bincode::serialize(&reply).expect("Failed to serialize reply");
        writer.send(Bytes::from(bytes)).await?;
        Ok(())
    }
}

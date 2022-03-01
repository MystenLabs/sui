// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod authority_core;
mod authority_server;
mod authority_store;
mod temporary_store;

use authority_core::AuthorityState;
use authority_server::AuthorityServer;
use authority_store::AuthorityStore;
use bytes::Bytes;
use move_binary_format::CompiledModule;
use std::sync::Arc;
use sui_types::base_types::TxContext;
use sui_types::committee::Committee;
use sui_types::crypto::KeyPair;
use sui_types::error::SuiResult;
use sui_types::messages::{SyncReply, TransactionInfoResponse};
use sui_types::object::Object;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::sync::oneshot;

/// The default size of inter-tasks channels.
pub(crate) const DEFAULT_CHANNEL_CAPACITY: usize = 1_000;

/// One-shot channel allowing the core to reply to core messages and requests.
pub(crate) type CoreReplier = oneshot::Sender<SuiResult<TransactionInfoResponse>>;
pub(crate) type SyncReplier = oneshot::Sender<SuiResult<SyncReply>>;

/// Spawn a new authority.
pub async fn spawn_authority(
    key_pair: &KeyPair,
    committee: Committee,
    db_path: &str,
    preload_modules: Vec<Vec<CompiledModule>>,
    preload_objects: &[Object],
    genesis_ctx: &mut TxContext,
    rx_consensus: Receiver<Bytes>,
) {
    let (tx_client_core_message, rx_client_core_message) = channel(DEFAULT_CHANNEL_CAPACITY);
    let (tx_sync_request, rx_sync_request) = channel(DEFAULT_CHANNEL_CAPACITY);

    // Create or open a new persistent storage.
    let store = Arc::new(AuthorityStore::open(&db_path, None));

    // Make the authority state and preload the initial objects.
    let state = AuthorityState::new(
        committee,
        /* name */ *key_pair.public_key_bytes(),
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
    // TODO

    // Spawn the authority server.
    AuthorityServer::spawn(state, rx_client_core_message, rx_sync_request, rx_consensus)
        .await
        .unwrap();
}

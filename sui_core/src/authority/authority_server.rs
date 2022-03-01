// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_core::AuthorityState;
use crate::authority::{CoreReplier, SyncReplier};
use bytes::Bytes;
use sui_types::error::SuiError;
use sui_types::messages::{ClientToAuthorityCoreMessage, SyncReply, SyncRequest};
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

/// The `AuthorityServer` runs the main tokio task of the authority. It receives external
/// messages and drives the core. In essence, it gathers message from all kind of sources
/// and feed them to the core using a single task.
pub struct AuthorityServer {
    /// The state of the authority, where all the core logic is.
    state: AuthorityState,
    /// Receive core messages from the clients.
    rx_client_core_message: Receiver<(ClientToAuthorityCoreMessage, CoreReplier)>,
    /// Receive sync requests.
    rx_sync_request: Receiver<(SyncRequest, SyncReplier)>,
    /// Receives messages from consensus.
    rx_consensus: Receiver<Bytes>,
}

impl AuthorityServer {
    /// Create an `AuthorityServer` and spawn it in a new tokio task.
    pub fn spawn(
        state: AuthorityState,
        rx_client_core_message: Receiver<(ClientToAuthorityCoreMessage, CoreReplier)>,
        rx_sync_request: Receiver<(SyncRequest, SyncReplier)>,
        rx_consensus: Receiver<Bytes>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                state,
                rx_client_core_message,
                rx_sync_request,
                rx_consensus,
            }
            .run()
            .await;
        })
    }

    /// The main reactor loop of the authority. It receives messages from outside components (such
    /// as the network or the consensus) and drives the core.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Handle the core messages coming from a client. Core messages are transactions
                // or certificates (ie. messages processed by the core).
                Some((message, replier)) = self.rx_client_core_message.recv() => {
                    let reply = match message {
                        ClientToAuthorityCoreMessage::Transaction(tx) => self
                            .state
                            .handle_client_transaction(tx)
                            .await,
                        ClientToAuthorityCoreMessage::Certificate(certificate) => self
                            .state
                            .handle_client_certificate(certificate)
                            .await
                    };

                    // Log the errors that are our fault, such as storage failures.
                    // TODO: Are there other errors to log here?
                    match &reply {
                        Err(SuiError::StorageError(e)) => log::error!("{}", e),
                        _ => ()
                    }

                    // This is the reply that the network will send back to the client.
                    replier.send(reply).expect("Failed to reply to core message");
                },

                // Handle sync requests.
                // TODO: Those should probably not be mixed up with core (safety-critical) messages
                // within `AuthorityState`. It is probably better to keep the core as simple as possible.
                Some((message, replier)) = self.rx_sync_request.recv() => {
                    let reply = match message {
                        SyncRequest::AccountInfoRequest(request) => self
                            .state
                            .handle_account_info_request(request)
                            .await
                            .map(SyncReply::AccountInfoResponse),
                        SyncRequest::ObjectInfoRequest(request) => self
                            .state
                            .handle_object_info_request(request)
                            .await
                            .map(SyncReply::ObjectInfoResponse),
                        SyncRequest::TransactionInfoRequest(request) => self
                            .state
                            .handle_transaction_info_request(request)
                            .await
                            .map(SyncReply::TransactionInfoResponse),
                    };

                    // This is the reply that the network will send back to the sender of the request.
                    replier.send(reply).expect("Failed to reply to sync request");
                },

                // Handle the messages coming from consensus. Those are simply sequenced
                // certificates of transactions with shared objects.
                Some(bytes) = self.rx_consensus.recv() => {
                    let result = match bincode::deserialize(&bytes) {
                        Ok(certificate) => self.state.handle_sequenced_certificate(certificate),
                        result => result.map_err(SuiError::from).map(|_| ())
                    };

                    // The consensus sub-system did something wrong. This is serious but not
                    // the fault of the core.
                    if let Err(e) = result {
                        log::error!("{}", e);
                    }
                }
            }
        }
    }
}

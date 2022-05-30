// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::TryStream;
use std::fmt::Display;
use std::sync::Arc;

use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::{PendingSubscription, SubscriptionSink};
use jsonrpsee_proc_macros::rpc;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

use sui_core::gateway_types::{SuiCertifiedTransaction, SuiEvent};
use sui_types::base_types::SuiAddress;

#[rpc(server, client, namespace = "sui")]
pub trait EventApi {
    #[subscription(name = "subscribeMoveEvents", item = SuiEvent)]
    fn sub_move_event(&self, move_event_type: String);

    #[subscription(name = "subscribeTransactions", item = SuiCertifiedTransaction)]
    fn sub_transaction(&self, sender: SuiAddress);

    #[subscription(name = "subscribeNetworkEvent", item = SuiEvent)]
    fn sub_network_event(&self);
}

pub struct SuiEventManager {
    move_event_broadcast: Sender<SuiEvent>,
    transaction_broadcast: Sender<SuiCertifiedTransaction>,
}

impl Default for SuiEventManager {
    fn default() -> Self {
        Self {
            move_event_broadcast: broadcast::channel(16).0,
            transaction_broadcast: broadcast::channel(16).0,
        }
    }
}

impl SuiEventManager {
    pub fn broadcast_event(&self, event: SuiEvent) {
        if self.move_event_broadcast.receiver_count() > 0 {
            let event_type = event.type_.clone();
            match self.move_event_broadcast.send(event) {
                Ok(num) => {
                    debug!("Broadcast Move event [{event_type}] to {num} peers.")
                }
                Err(e) => {
                    warn!("Error broadcasting event [{event_type}]. Error: {e}")
                }
            }
        }
    }

    pub fn broadcast_transaction(&self, cert: SuiCertifiedTransaction) {
        if self.transaction_broadcast.receiver_count() > 0 {
            let tx_digest = cert.transaction_digest;
            match self.transaction_broadcast.send(cert) {
                Ok(num) => {
                    debug!("Broadcast transaction [{tx_digest:?}] to {num} peers.")
                }
                Err(e) => {
                    warn!("Error broadcasting transaction [{tx_digest:?}]. Error: {e}")
                }
            }
        }
    }
}

pub struct EventApiImpl {
    manager: Arc<SuiEventManager>,
}

impl EventApiImpl {
    pub fn new(manager: Arc<SuiEventManager>) -> Self {
        Self { manager }
    }
}

impl EventApiServer for EventApiImpl {
    fn sub_move_event(&self, pending: PendingSubscription, event_type: String) {
        if let Some(sink) = pending.accept() {
            let stream = BroadcastStream::new(self.manager.move_event_broadcast.subscribe());
            let stream =
                stream.filter(move |event| matches!(event, Ok(event) if event.type_ == event_type));
            spawn_subscript(sink, stream);
        }
    }

    fn sub_transaction(&self, pending: PendingSubscription, sender: SuiAddress) {
        if let Some(sink) = pending.accept() {
            let stream = BroadcastStream::new(self.manager.transaction_broadcast.subscribe());
            let stream = stream.filter(move |tx| matches!(tx, Ok(tx) if tx.data.sender == sender));
            spawn_subscript(sink, stream);
        }
    }

    fn sub_network_event(&self, _pending: PendingSubscription) {
        todo!()
    }
}

fn spawn_subscript<S, T, E>(mut sink: SubscriptionSink, rx: S)
where
    S: TryStream<Ok = T, Error = E> + Unpin + Send + 'static,
    T: Serialize,
    E: Display,
{
    tokio::spawn(async move {
        match sink.pipe_from_try_stream(rx).await {
            SubscriptionClosed::Success => {
                sink.close(SubscriptionClosed::Success);
            }
            SubscriptionClosed::RemotePeerAborted => (),
            SubscriptionClosed::Failed(err) => {
                sink.close(err);
            }
        };
    });
}

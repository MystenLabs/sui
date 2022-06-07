// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;
use std::sync::Arc;

use futures::{TryStream, TryStreamExt};
use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::{PendingSubscription, SubscriptionSink};
use jsonrpsee_proc_macros::rpc;
use serde::Serialize;

use sui_core::authority::AuthorityState;
use sui_core::event_handler::EventHandler;
use sui_core::gateway_types::SuiEvent;

#[rpc(server, client, namespace = "sui")]
pub trait EventApi {
    #[subscription(name = "subscribeMoveEvents", item = SuiEvent)]
    fn sub_move_event(&self, move_event_type: String);
}

pub struct EventApiImpl {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventApiImpl {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

impl EventApiServer for EventApiImpl {
    fn sub_move_event(&self, pending: PendingSubscription, _event_type: String) {
        if let Some(sink) = pending.accept() {
            let state = self.state.clone();
            let stream = self.event_handler.subscribe();
            let stream =
                stream.map_ok(move |e| SuiEvent::try_from(e.event, &state.module_cache).unwrap());
            spawn_subscript(sink, stream);
        }
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

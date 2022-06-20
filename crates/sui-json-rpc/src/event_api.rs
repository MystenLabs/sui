// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;
use std::sync::Arc;

use futures::{StreamExt, TryStream};
use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::{PendingSubscription, SubscriptionSink};
use serde::Serialize;
use tracing::warn;

use sui_core::authority::AuthorityState;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_api::rpc_types::{SuiEvent, SuiEventFilter};
use sui_json_rpc_api::EventApiServer;

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
    fn subscribe_event(&self, pending: PendingSubscription, filter: SuiEventFilter) {
        let filter = match filter.try_into() {
            Ok(filter) => filter,
            Err(e) => {
                let e: anyhow::Error = e;
                let e: jsonrpsee_core::Error = e.into();
                warn!(error = ?e, "Rejecting subscription request.");
                pending.reject(e);
                return;
            }
        };

        if let Some(sink) = pending.accept() {
            let state = self.state.clone();
            let stream = self.event_handler.subscribe(filter);
            let stream = stream.map(move |e| SuiEvent::try_from(e.event, &state.module_cache));
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
                warn!(error = ?err, "Event subscription closed.");
                sink.close(err);
            }
        };
    });
}

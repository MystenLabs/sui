// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::Arc;

use futures::{StreamExt, TryStream};
use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::{PendingSubscription, SubscriptionSink};
use move_core_types::parser::parse_struct_tag;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

use sui_core::authority::AuthorityState;
use sui_core::event_filter::EventFilter;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_api::rpc_types::SuiEvent;
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
    fn subscribe_move_event_by_type(
        &self,
        pending: PendingSubscription,
        event: String,
        field_filter: BTreeMap<String, Value>,
    ) {
        // parse_struct_tag converts StructTag string e.g. `0x2::DevNetNFT::MintNFTEvent` to StructTag object,
        let event_type = match parse_struct_tag(&event) {
            Ok(event) => event,
            Err(e) => {
                let e: jsonrpsee_core::Error = e.into();
                warn!(error = ?e, "Rejecting subscription request.");
                pending.reject(e);
                return;
            }
        };

        if let Some(sink) = pending.accept() {
            let state = self.state.clone();
            let type_filter = EventFilter::ByMoveEventType(event_type);
            let field_filter = EventFilter::ByMoveEventFields(field_filter);
            let stream = self.event_handler.subscribe(type_filter.and(field_filter));
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

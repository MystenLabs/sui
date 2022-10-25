// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use jsonrpsee_core::server::rpc_module::SubscriptionSink;
use tracing::warn;

use sui_core::authority::AuthorityState;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_types::{EventPage, SuiEvent, SuiEventEnvelope, SuiEventFilter};
use sui_open_rpc::Module;
use sui_types::event::{EventEnvelope, EventID};
use sui_types::query::{EventQuery, Ordering};

use crate::api::EventReadApiServer;
use crate::api::{cap_page_limit, EventStreamingApiServer};
use crate::streaming_api::spawn_subscription;
use crate::SuiRpcModule;

pub struct EventStreamingApiImpl {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventStreamingApiImpl {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

#[async_trait]
impl EventStreamingApiServer for EventStreamingApiImpl {
    fn subscribe_event(
        &self,
        mut sink: SubscriptionSink,
        filter: SuiEventFilter,
    ) -> SubscriptionResult {
        let filter = match filter.try_into() {
            Ok(filter) => filter,
            Err(e) => {
                let e = jsonrpsee_core::Error::from(e);
                warn!(error = ?e, "Rejecting subscription request.");
                return Ok(sink.reject(e)?);
            }
        };

        let state = self.state.clone();
        let stream = self.event_handler.subscribe(filter);
        let stream = stream.map(move |e: EventEnvelope| {
            let event = SuiEvent::try_from(e.event, state.module_cache.as_ref());
            event.map(|event| SuiEventEnvelope {
                // The id will not be serialised
                id: 0,
                timestamp: e.timestamp,
                tx_digest: e.tx_digest,
                event,
            })
        });
        spawn_subscription(sink, stream);
        Ok(())
    }
}

impl SuiRpcModule for EventStreamingApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EventStreamingApiOpenRpc::module_doc()
    }
}

#[allow(unused)]
pub struct EventReadApiImpl {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventReadApiImpl {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

#[allow(unused)]
#[async_trait]
impl EventReadApiServer for EventReadApiImpl {
    async fn get_events(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        limit: Option<usize>,
        order: Ordering,
    ) -> RpcResult<EventPage> {
        let descending = order == Ordering::Descending;
        let limit = cap_page_limit(limit)?;
        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .get_events(query, cursor, limit + 1, descending)
            .await?;
        let next_cursor = data.get(limit).map(|event| event.id);
        data.truncate(limit);
        Ok(EventPage { data, next_cursor })
    }
}

impl SuiRpcModule for EventReadApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EventReadApiOpenRpc::module_doc()
    }
}

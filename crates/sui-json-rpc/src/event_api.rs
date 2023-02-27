// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{StreamExt, TryStream};

use jsonrpsee::core::error::SubscriptionClosed;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};
use mysten_metrics::spawn_monitored_task;
use serde::Serialize;
use tracing::{debug, warn};

use sui_core::authority::AuthorityState;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_types::{EventPage, SuiEvent, SuiEventEnvelope, SuiEventFilter};
use sui_open_rpc::Module;
use sui_types::event::{EventEnvelope, EventID};
use sui_types::query::EventQuery;

use crate::api::cap_page_limit;
use crate::api::EventReadApiServer;
use crate::SuiRpcModule;

fn spawn_subscription<S, T, E>(mut sink: SubscriptionSink, rx: S)
where
    S: TryStream<Ok = T, Error = E> + Unpin + Send + 'static,
    T: Serialize,
    E: Display,
{
    spawn_monitored_task!(async move {
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

pub struct EventReadApi {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventReadApi {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

#[async_trait]
impl EventReadApiServer for EventReadApi {
    async fn get_events(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        debug!(
            ?query,
            ?cursor,
            ?limit,
            ?descending_order,
            "get_events query"
        );
        let descending = descending_order.unwrap_or_default();
        let limit = cap_page_limit(limit);
        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .get_events(query, cursor, limit + 1, descending)
            .await?;
        let next_cursor = data.get(limit).map(|(id, _)| id.clone());
        data.truncate(limit);
        let data = data.into_iter().map(|(_, event)| event).collect();
        Ok(EventPage { data, next_cursor })
    }

    fn subscribe_event(
        &self,
        mut sink: SubscriptionSink,
        filter: SuiEventFilter,
    ) -> SubscriptionResult {
        let filter = match filter.try_into() {
            Ok(filter) => filter,
            Err(e) => {
                let e = jsonrpsee::core::Error::from(e);
                warn!(error = ?e, "Rejecting subscription request.");
                return Ok(sink.reject(e)?);
            }
        };

        let state = self.state.clone();
        let stream = self.event_handler.subscribe(filter);
        let stream = stream.map(move |e: EventEnvelope| {
            let event = SuiEvent::try_from(e.event, state.module_cache.as_ref());
            event.map(|event| SuiEventEnvelope {
                timestamp: e.timestamp,
                tx_digest: e.tx_digest,
                id: EventID::from((e.tx_digest, e.event_num as i64)),
                event,
            })
        });
        spawn_subscription(sink, stream);
        Ok(())
    }
}

impl SuiRpcModule for EventReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EventReadApiOpenRpc::module_doc()
    }
}

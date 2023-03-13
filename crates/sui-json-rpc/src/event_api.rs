// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use jsonrpsee::core::error::SubscriptionClosed;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};
use serde::Serialize;
use tracing::{debug, warn};

use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{EventFilter, EventPage, SuiEvent};
use sui_open_rpc::Module;
use sui_types::digests::TransactionDigest;
use sui_types::event::EventID;
use sui_types::messages::TransactionEffectsAPI;

use crate::api::cap_page_limit;
use crate::api::EventReadApiServer;
use crate::error::Error;
use crate::SuiRpcModule;

pub fn spawn_subscription<S, T>(mut sink: SubscriptionSink, rx: S)
where
    S: Stream<Item = T> + Unpin + Send + 'static,
    T: Serialize,
{
    spawn_monitored_task!(async move {
        match sink.pipe_from_stream(rx).await {
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
}

impl EventReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl EventReadApiServer for EventReadApi {
    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        let store = self.state.load_epoch_store_one_call_per_task();
        let effect = self.state.get_executed_effects(transaction_digest).await?;
        let events = if let Some(event_digest) = effect.events_digest() {
            self.state
                .get_transaction_events(event_digest)
                .map_err(Error::SuiError)?
                .data
                .into_iter()
                .enumerate()
                .map(|(seq, e)| {
                    SuiEvent::try_from(
                        e,
                        *effect.transaction_digest(),
                        seq as u64,
                        None,
                        store.module_cache(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(Error::SuiError)?
        } else {
            vec![]
        };
        Ok(events)
    }

    async fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
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
            .query_events(query, cursor.clone(), limit + 1, descending)
            .await?;
        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor = data.last().map_or(cursor, |e| Some(e.id.clone()));
        Ok(EventPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    fn subscribe_event(&self, sink: SubscriptionSink, filter: EventFilter) -> SubscriptionResult {
        spawn_subscription(sink, self.state.event_handler.subscribe(filter));
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

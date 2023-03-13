// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::store::IndexerStore;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::types::{SubscriptionEmptyError, SubscriptionResult};
use jsonrpsee::{RpcModule, SubscriptionSink};
use sui_json_rpc::api::{EventReadApiClient, EventReadApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{EventPage, SuiEventFilter};
use sui_open_rpc::Module;
use sui_types::event::EventID;
use sui_types::query::EventQuery;

pub(crate) struct EventReadApi<S> {
    state: S,
    fullnode: HttpClient,
    method_to_be_forwarded: Vec<String>,
}

impl<S: IndexerStore> EventReadApi<S> {
    pub fn new(state: S, fullnode_client: HttpClient) -> Self {
        Self {
            state,
            fullnode: fullnode_client,
            // TODO: read from centralized config
            method_to_be_forwarded: vec![],
        }
    }

    pub fn get_events_internal(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> Result<EventPage, IndexerError> {
        self.state
            .get_events(query, cursor, limit, descending_order.unwrap_or_default())
    }
}

#[async_trait]
impl<S> EventReadApiServer for EventReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn get_events(
        &self,
        query: EventQuery,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        if self
            .method_to_be_forwarded
            .contains(&"get_events".to_string())
        {
            return self
                .fullnode
                .get_events(query, cursor, limit, descending_order)
                .await;
        }
        Ok(self.get_events_internal(query, cursor, limit, descending_order)?)
    }

    fn subscribe_event(
        &self,
        mut _sink: SubscriptionSink,
        _filter: SuiEventFilter,
    ) -> SubscriptionResult {
        // subscription not supported by subscription yet
        Err(SubscriptionEmptyError)
    }
}

impl<S> SuiRpcModule for EventReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::EventReadApiOpenRpc::module_doc()
    }
}

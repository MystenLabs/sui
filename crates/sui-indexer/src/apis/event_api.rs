// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

pub(crate) struct EventReadApi {
    fullnode: HttpClient,
}

impl EventReadApi {
    pub fn new(fullnode_client: HttpClient) -> Self {
        Self {
            fullnode: fullnode_client,
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
        self.fullnode
            .get_events(query, cursor, limit, descending_order)
            .await
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

impl SuiRpcModule for EventReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::EventReadApiOpenRpc::module_doc()
    }
}

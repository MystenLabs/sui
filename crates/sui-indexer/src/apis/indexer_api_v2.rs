// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove after the functions are implemented
#![allow(unused_variables)]

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};

use sui_json_rpc::api::{cap_page_limit, IndexerApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    DynamicFieldPage, EventFilter, EventPage, ObjectsPage, Page, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
    TransactionFilter,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::event::EventID;

use crate::store::IndexerStoreV2;

pub(crate) struct IndexerApiV2<S> {
    state: S,
}

impl<S: IndexerStoreV2> IndexerApiV2<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }

    async fn get_owned_objects_internal(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        let limit = limit.unwrap_or(50);
        if limit == 0 {
            return Ok(ObjectsPage::empty());
        }

        let options = query.unwrap_or_default().options.unwrap_or_default();
        // TODO how to cap?
        let mut objects = self
            .state
            .get_owned_object(address, cursor, limit + 1)
            .await?;

        let has_next_page = objects.len() > limit;
        objects.truncate(limit);
        let next_cursor = objects
            .last()
            .map_or(cursor, |o_read| Some(o_read.object_id()));

        let data = objects
            .into_iter()
            .map(|o| (o, options.clone()).try_into())
            .collect::<Result<Vec<SuiObjectResponse>, _>>()?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }
}

#[async_trait]
impl<S> IndexerApiServer for IndexerApiV2<S>
where
    S: IndexerStoreV2 + Sync + Send + 'static,
{
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        self.get_owned_objects_internal(address, query, cursor, limit)
            .await
    }

    async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage> {
        let limit = cap_page_limit(limit);
        self.state
            .query_transaction_blocks(query, cursor, limit, descending_order)
            .await
            .map_err(|e| anyhow!(e).into())
    }

    async fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        unimplemented!()
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        unimplemented!()
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        unimplemented!()
    }

    fn subscribe_event(&self, _sink: SubscriptionSink, _filter: EventFilter) -> SubscriptionResult {
        unimplemented!()
    }

    fn subscribe_transaction(
        &self,
        _sink: SubscriptionSink,
        _filter: TransactionFilter,
    ) -> SubscriptionResult {
        unimplemented!()
    }

    async fn resolve_name_service_address(&self, name: String) -> RpcResult<Option<SuiAddress>> {
        unimplemented!()
    }

    async fn resolve_name_service_names(
        &self,
        address: SuiAddress,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<Page<String, ObjectID>> {
        unimplemented!()
    }
}

impl<S> SuiRpcModule for IndexerApiV2<S>
where
    S: IndexerStoreV2 + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::IndexerApiOpenRpc::module_doc()
    }
}

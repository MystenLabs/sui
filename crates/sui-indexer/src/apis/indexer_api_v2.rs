// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove after the functions are implemented
#![allow(unused_variables)]
#![allow(dead_code)]

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};

use sui_json_rpc::api::IndexerApiServer;
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

use crate::store::PgIndexerStoreV2;

pub(crate) struct IndexerApiV2 {
    pg_store: PgIndexerStoreV2,
}

impl IndexerApiV2 {
    pub fn new(pg_store: PgIndexerStoreV2) -> Self {
        Self { pg_store }
    }
}

#[async_trait]
impl IndexerApiServer for IndexerApiV2 {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        // A ref impl can be found in https://github.com/MystenLabs/sui/pull/13574/
        unimplemented!()
    }

    async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage> {
        // A ref impl can be found in https://github.com/MystenLabs/sui/pull/13574/
        unimplemented!()
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

    fn subscribe_event(&self, sink: SubscriptionSink, filter: EventFilter) -> SubscriptionResult {
        unimplemented!()
    }

    fn subscribe_transaction(
        &self,
        sink: SubscriptionSink,
        filter: TransactionFilter,
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

impl SuiRpcModule for IndexerApiV2 {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::IndexerApiOpenRpc::module_doc()
    }
}

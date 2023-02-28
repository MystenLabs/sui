// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::checkpoints::{get_latest_checkpoint_sequence_number, get_rpc_checkpoint};
use crate::{get_pg_pool_connection, PgConnectionPool};
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_json_rpc::api::ReadApiClient;
use sui_json_rpc::api::ReadApiServer;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, DynamicFieldPage, GetObjectDataResponse, GetPastObjectDataResponse,
    GetRawObjectDataResponse, MoveFunctionArgType, SuiMoveNormalizedFunction,
    SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiObjectInfo, SuiTransactionResponse,
    TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TxSequenceNumber};
use sui_types::digests::{CheckpointContentsDigest, CheckpointDigest, TransactionDigest};
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
};
use sui_types::query::TransactionQuery;

pub(crate) struct ReadApi {
    fullnode: HttpClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    method_to_be_forwarded: Vec<String>,
}

impl ReadApi {
    pub fn new(pg_connection_pool: Arc<PgConnectionPool>, fullnode_client: HttpClient) -> Self {
        Self {
            pg_connection_pool,
            fullnode: fullnode_client,
            // TODO: read from config or env file
            method_to_be_forwarded: vec![],
        }
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        get_latest_checkpoint_sequence_number(&mut pg_pool_conn)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        let checkpoint = get_rpc_checkpoint(&mut pg_pool_conn, id)?;
        Ok(checkpoint)
    }
}

#[async_trait]
impl ReadApiServer for ReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        self.fullnode.get_objects_owned_by_address(address).await
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        self.fullnode
            .get_dynamic_fields(parent_object_id, cursor, limit)
            .await
    }

    async fn get_object(&self, object_id: ObjectID) -> RpcResult<GetObjectDataResponse> {
        self.fullnode.get_object(object_id).await
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<GetObjectDataResponse> {
        self.fullnode
            .get_dynamic_field_object(parent_object_id, name)
            .await
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        self.fullnode.get_total_transaction_number().await
    }

    async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        self.fullnode
            .get_transactions(query, cursor, limit, descending_order)
            .await
    }

    async fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>> {
        self.fullnode.get_transactions_in_range(start, end).await
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionResponse> {
        self.fullnode.get_transaction(digest).await
    }

    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        self.fullnode
            .get_normalized_move_modules_by_package(package)
            .await
    }

    async fn get_normalized_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> RpcResult<SuiMoveNormalizedModule> {
        self.fullnode
            .get_normalized_move_module(package, module_name)
            .await
    }

    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct> {
        self.fullnode
            .get_normalized_move_struct(package, module_name, struct_name)
            .await
    }

    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction> {
        self.fullnode
            .get_normalized_move_function(package, module_name, function_name)
            .await
    }

    async fn get_move_function_arg_types(
        &self,
        package: ObjectID,
        module: String,
        function: String,
    ) -> RpcResult<Vec<MoveFunctionArgType>> {
        self.fullnode
            .get_move_function_arg_types(package, module, function)
            .await
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> RpcResult<GetPastObjectDataResponse> {
        self.fullnode.try_get_past_object(object_id, version).await
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<CheckpointSequenceNumber> {
        if self
            .method_to_be_forwarded
            .contains(&"get_latest_checkpoint_sequence_number".to_string())
        {
            return self.fullnode.get_latest_checkpoint_sequence_number().await;
        }
        Ok(self.get_latest_checkpoint_sequence_number().await? as u64)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        if self
            .method_to_be_forwarded
            .contains(&"get_checkpoint".to_string())
        {
            return self.fullnode.get_checkpoint(id).await;
        }
        Ok(self.get_checkpoint(id).await?)
    }

    // NOTE: checkpoint APIs below will be deprecated,
    // thus skipping them regarding indexer native implementations.
    async fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> RpcResult<CheckpointSummary> {
        self.fullnode.get_checkpoint_summary_by_digest(digest).await
    }

    async fn get_checkpoint_summary(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointSummary> {
        self.fullnode.get_checkpoint_summary(sequence_number).await
    }

    async fn get_checkpoint_contents_by_digest(
        &self,
        digest: CheckpointContentsDigest,
    ) -> RpcResult<CheckpointContents> {
        self.fullnode
            .get_checkpoint_contents_by_digest(digest)
            .await
    }

    async fn get_checkpoint_contents(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointContents> {
        self.fullnode.get_checkpoint_contents(sequence_number).await
    }

    async fn get_raw_object(&self, object_id: ObjectID) -> RpcResult<GetRawObjectDataResponse> {
        self.fullnode.get_raw_object(object_id).await
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ReadApiOpenRpc::module_doc()
    }
}

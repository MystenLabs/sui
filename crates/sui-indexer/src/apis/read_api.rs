// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::checkpoints::{get_latest_checkpoint_sequence_number, get_rpc_checkpoint};
use crate::models::transactions::{
    get_all_transaction_digest_page, get_total_transaction_number, get_transaction_by_digest,
    get_transaction_digest_page_by_mutated_object,
    get_transaction_digest_page_by_recipient_address,
    get_transaction_digest_page_by_sender_address, get_transaction_sequence_by_digest,
};
use crate::{get_pg_pool_connection, PgConnectionPool};
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_json_rpc::api::{cap_page_limit, ReadApiClient, ReadApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, DynamicFieldPage, GetObjectDataResponse, GetPastObjectDataResponse,
    GetRawObjectDataResponse, MoveFunctionArgType, Page, SuiMoveNormalizedFunction,
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

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        let total_tx_number = get_total_transaction_number(&mut pg_pool_conn)?;
        Ok(total_tx_number as u64)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionResponse> {
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        let txn_resp: SuiTransactionResponse =
            get_transaction_by_digest(&mut pg_pool_conn, digest.to_string())?.try_into()?;
        Ok(txn_resp)
    }

    async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        let limit = cap_page_limit(limit);
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        let indexer_seq_number = get_transaction_sequence_by_digest(
            &mut pg_pool_conn,
            cursor.map(|digest| digest.to_string()),
            descending_order.unwrap_or_default(),
        )?;

        let digests_from_db = match query {
            TransactionQuery::All => get_all_transaction_digest_page(
                &mut pg_pool_conn,
                indexer_seq_number,
                limit,
                descending_order.unwrap_or_default(),
            ),
            // TODO(gegaowp): implement Move call query handling.
            TransactionQuery::MoveFunction {
                package: _,
                module: _,
                function: _,
            } => Ok(vec![]),
            // TODO(gegaowp): input objects are tricky to retrive from
            // SuiTransactionResponse, instead we should store the BCS
            // serialized transaction and retrive from there.
            // This is now blocked by the endpoint on FN side.
            TransactionQuery::InputObject(_input_obj_id) => Ok(vec![]),
            TransactionQuery::MutatedObject(mutated_obj_id) => {
                get_transaction_digest_page_by_mutated_object(
                    &mut pg_pool_conn,
                    mutated_obj_id.to_string(),
                    indexer_seq_number,
                    limit + 1,
                    descending_order.unwrap_or_default(),
                )
            }
            TransactionQuery::FromAddress(sender_address) => {
                get_transaction_digest_page_by_sender_address(
                    &mut pg_pool_conn,
                    sender_address.to_string(),
                    indexer_seq_number,
                    limit + 1,
                    descending_order.unwrap_or_default(),
                )
            }
            TransactionQuery::ToAddress(recipient_address) => {
                get_transaction_digest_page_by_recipient_address(
                    &mut pg_pool_conn,
                    recipient_address.to_string(),
                    indexer_seq_number,
                    limit + 1,
                    descending_order.unwrap_or_default(),
                )
            }
        }?;

        // digests here are of size (limit + 1), where the last one is the cursor for the next page
        let mut txn_digests = digests_from_db
            .iter()
            .map(|digest| {
                let txn_digest: Result<TransactionDigest, _> = digest.clone().parse();
                txn_digest.map_err(|e| {
                    IndexerError::JsonSerdeError(format!(
                        "Failed to deserialize transaction digest: {:?} with error {:?}",
                        digest, e
                    ))
                })
            })
            .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?;

        let next_cursor = txn_digests.get(limit).cloned();
        txn_digests.truncate(limit);

        Ok(Page {
            data: txn_digests,
            next_cursor,
        })
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
        if self
            .method_to_be_forwarded
            .contains(&"get_total_transaction_number".to_string())
        {
            return self.fullnode.get_total_transaction_number().await;
        }
        self.get_total_transaction_number().await
    }

    async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        if self
            .method_to_be_forwarded
            .contains(&"get_transactions".to_string())
        {
            return self
                .fullnode
                .get_transactions(query, cursor, limit, descending_order)
                .await;
        }
        self.get_transactions(query, cursor, limit, descending_order)
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
        if self
            .method_to_be_forwarded
            .contains(&"get_transaction".to_string())
        {
            return self.fullnode.get_transaction(digest).await;
        }
        self.get_transaction(digest).await
    }

    async fn multi_get_transactions(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> RpcResult<Vec<SuiTransactionResponse>> {
        if self
            .method_to_be_forwarded
            .contains(&"muti_get_transactions".to_string())
        {
            return self.fullnode.multi_get_transactions(digests).await;
        }
        self.multi_get_transactions(digests).await
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

    async fn get_display_deprecated(
        &self,
        object_id: ObjectID,
    ) -> RpcResult<BTreeMap<String, String>> {
        self.fullnode.get_display_deprecated(object_id).await
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

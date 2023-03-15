// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::store::IndexerStore;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use std::collections::BTreeMap;
use sui_json_rpc::api::{cap_page_limit, ReadApiClient, ReadApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    BigInt, Checkpoint, CheckpointId, DynamicFieldPage, MoveFunctionArgType, ObjectsPage, Page,
    SuiGetPastObjectRequest, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionResponse, SuiTransactionResponseOptions, SuiTransactionResponseQuery,
    TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TxSequenceNumber};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::query::TransactionFilter;

pub(crate) struct ReadApi<S> {
    fullnode: HttpClient,
    state: S,
    method_to_be_forwarded: Vec<String>,
}

impl<S: IndexerStore> ReadApi<S> {
    pub fn new(state: S, fullnode_client: HttpClient) -> Self {
        Self {
            state,
            fullnode: fullnode_client,
            // TODO: read from config or env file
            method_to_be_forwarded: vec![],
        }
    }

    fn get_total_transaction_number_internal(&self) -> Result<u64, IndexerError> {
        self.state.get_total_transaction_number().map(|n| n as u64)
    }

    fn get_transaction_with_options_internal(
        &self,
        digest: &TransactionDigest,
        _options: Option<SuiTransactionResponseOptions>,
    ) -> Result<SuiTransactionResponse, IndexerError> {
        // TODO(chris): support options in indexer
        let txn_resp: SuiTransactionResponse = self
            .state
            .get_transaction_by_digest(&digest.base58_encode())?
            .try_into()?;
        Ok(txn_resp)
    }

    fn multi_get_transactions_with_options_internal(
        &self,
        digests: &[TransactionDigest],
        _options: Option<SuiTransactionResponseOptions>,
    ) -> Result<Vec<SuiTransactionResponse>, IndexerError> {
        let digest_strs = digests
            .iter()
            .map(|digest| digest.base58_encode())
            .collect::<Vec<_>>();
        let tx_vec = self.state.multi_get_transactions_by_digests(&digest_strs)?;
        let tx_resp_vec = tx_vec
            .into_iter()
            .map(|txn| txn.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tx_resp_vec)
    }

    fn query_transactions_internal(
        &self,
        query: SuiTransactionResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> Result<TransactionsPage, IndexerError> {
        let limit = cap_page_limit(limit);
        let is_descending = descending_order.unwrap_or_default();
        let cursor_str = cursor.map(|digest| digest.to_string());

        let opts = query.options.unwrap_or_default();
        if !opts.only_digest() {
            // TODO(chris): implement this as a separate PR
            return Err(IndexerError::NotImplementedError(
                "options has not been implemented on indexer for queryTransactions".to_string(),
            ));
        }

        let digests_from_db = match query.filter {
            None => {
                let indexer_seq_number = self
                    .state
                    .get_transaction_sequence_by_digest(cursor_str, is_descending)?;
                self.state
                    .get_all_transaction_digest_page(indexer_seq_number, limit, is_descending)
            }
            Some(TransactionFilter::MoveFunction {
                package,
                module,
                function,
            }) => {
                let move_call_seq_number = self
                    .state
                    .get_move_call_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_move_call(
                    package.to_string(),
                    module,
                    function,
                    move_call_seq_number,
                    limit,
                    is_descending,
                )
            }
            // TODO(gegaowp): input objects are tricky to retrive from
            // SuiTransactionResponse, instead we should store the BCS
            // serialized transaction and retrive from there.
            // This is now blocked by the endpoint on FN side.
            Some(TransactionFilter::InputObject(_input_obj_id)) => Ok(vec![]),
            Some(TransactionFilter::ChangedObject(mutated_obj_id)) => {
                let indexer_seq_number = self
                    .state
                    .get_transaction_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_mutated_object(
                    mutated_obj_id.to_string(),
                    indexer_seq_number,
                    limit + 1,
                    is_descending,
                )
            }
            Some(TransactionFilter::FromAddress(sender_address)) => {
                let indexer_seq_number = self
                    .state
                    .get_transaction_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_sender_address(
                    sender_address.to_string(),
                    indexer_seq_number,
                    limit + 1,
                    is_descending,
                )
            }
            Some(TransactionFilter::ToAddress(recipient_address)) => {
                let recipient_seq_number = self
                    .state
                    .get_recipient_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_recipient_address(
                    recipient_address.to_string(),
                    recipient_seq_number,
                    limit + 1,
                    is_descending,
                )
            }
        }?;

        // digests here are of size (limit + 1), where the last one is the cursor for the next page
        let mut txn_digests = digests_from_db
            .iter()
            .map(|digest| {
                let txn_digest: Result<TransactionDigest, _> = digest.clone().parse();
                txn_digest.map_err(|e| {
                    IndexerError::SerdeError(format!(
                        "Failed to deserialize transaction digest: {:?} with error {:?}",
                        digest, e
                    ))
                })
            })
            .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?;

        let has_next_page = txn_digests.len() > limit;
        txn_digests.truncate(limit);
        let next_cursor = txn_digests.last().cloned().map_or(cursor, Some);

        Ok(Page {
            data: txn_digests
                .into_iter()
                .map(SuiTransactionResponse::new)
                .collect(),
            next_cursor,
            has_next_page,
        })
    }

    fn get_object_with_options_internal(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<SuiObjectResponse, IndexerError> {
        let read = self.state.get_object(object_id, None)?;
        Ok((read, options.unwrap_or_default()).try_into()?)
    }

    fn get_latest_checkpoint_sequence_number_internal(&self) -> Result<u64, IndexerError> {
        self.state
            .get_latest_checkpoint_sequence_number()
            .map(|n| n as u64)
    }

    fn get_checkpoint_internal(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError> {
        let checkpoint = self.state.get_checkpoint(id)?;
        checkpoint.try_into()
    }
}

#[async_trait]
impl<S> ReadApiServer for ReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        options: Option<SuiObjectDataOptions>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage> {
        self.fullnode
            .get_owned_objects(address, options, cursor, limit, at_checkpoint)
            .await
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        if self
            .method_to_be_forwarded
            .contains(&"get_object_with_options".into())
        {
            return self
                .fullnode
                .get_object_with_options(object_id, options)
                .await;
        }

        Ok(self.get_object_with_options_internal(object_id, options)?)
    }

    async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        return self
            .fullnode
            .multi_get_object_with_options(object_ids, options)
            .await;
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

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        self.fullnode
            .get_dynamic_field_object(parent_object_id, name)
            .await
    }

    async fn get_total_transaction_number(&self) -> RpcResult<BigInt> {
        if self
            .method_to_be_forwarded
            .contains(&"get_total_transaction_number".to_string())
        {
            return self.fullnode.get_total_transaction_number().await;
        }
        Ok(self.get_total_transaction_number_internal()?.into())
    }

    async fn query_transactions(
        &self,
        query: SuiTransactionResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        if self
            .method_to_be_forwarded
            .contains(&"query_transactions".to_string())
        {
            return self
                .fullnode
                .query_transactions(query, cursor, limit, descending_order)
                .await;
        }
        Ok(self.query_transactions_internal(query, cursor, limit, descending_order)?)
    }

    async fn get_transactions_in_range_deprecated(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>> {
        self.fullnode
            .get_transactions_in_range_deprecated(start, end)
            .await
    }

    async fn get_transaction_with_options(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<SuiTransactionResponse> {
        if self
            .method_to_be_forwarded
            .contains(&"get_transaction".to_string())
        {
            return self
                .fullnode
                .get_transaction_with_options(digest, options)
                .await;
        }
        Ok(self.get_transaction_with_options_internal(&digest, options)?)
    }

    async fn multi_get_transactions_with_options(
        &self,
        digests: Vec<TransactionDigest>,
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionResponse>> {
        if self
            .method_to_be_forwarded
            .contains(&"multi_get_transactions_with_options".to_string())
        {
            return self
                .fullnode
                .multi_get_transactions_with_options(digests, options)
                .await;
        }
        Ok(self.multi_get_transactions_with_options_internal(&digests, options)?)
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
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        self.fullnode
            .try_get_past_object(object_id, version, options)
            .await
    }

    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        self.fullnode
            .try_multi_get_past_objects(past_objects, options)
            .await
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<CheckpointSequenceNumber> {
        if self
            .method_to_be_forwarded
            .contains(&"get_latest_checkpoint_sequence_number".to_string())
        {
            return self.fullnode.get_latest_checkpoint_sequence_number().await;
        }
        Ok(self.get_latest_checkpoint_sequence_number_internal()?)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        if self
            .method_to_be_forwarded
            .contains(&"get_checkpoint".to_string())
        {
            return self.fullnode.get_checkpoint(id).await;
        }
        Ok(self.get_checkpoint_internal(id)?)
    }
}

impl<S> SuiRpcModule for ReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ReadApiOpenRpc::module_doc()
    }
}

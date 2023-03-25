// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};

use sui_core::event_handler::EventHandler;
use sui_json_rpc::api::IndexerApiServer;
use sui_json_rpc::api::{validate_limit, IndexerApiClient, QUERY_MAX_RESULT_LIMIT};
use sui_json_rpc::indexer_api::spawn_subscription;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    CheckpointId, DynamicFieldPage, EventFilter, EventPage, ObjectsPage, Page, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionResponse, SuiTransactionResponseQuery, TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::event::EventID;
use sui_types::query::TransactionFilter;

use crate::errors::IndexerError;
use crate::store::IndexerStore;

pub(crate) struct IndexerApi<S> {
    state: S,
    fullnode: HttpClient,
    event_handler: Arc<EventHandler>,
    migrated_methods: Vec<String>,
}

impl<S: IndexerStore> IndexerApi<S> {
    pub fn new(
        state: S,
        fullnode_client: HttpClient,
        event_handler: Arc<EventHandler>,
        migrated_methods: Vec<String>,
    ) -> Self {
        Self {
            state,
            fullnode: fullnode_client,
            event_handler,
            migrated_methods,
        }
    }

    pub fn get_events_internal(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> Result<EventPage, IndexerError> {
        self.state
            .get_events(query, cursor, limit, descending_order.unwrap_or_default())
    }

    fn query_transactions_internal(
        &self,
        query: SuiTransactionResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> Result<TransactionsPage, IndexerError> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT)?;
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
                self.state.get_all_transaction_digest_page(
                    indexer_seq_number,
                    limit + 1,
                    is_descending,
                )
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
                    limit + 1,
                    is_descending,
                )
            }
            Some(TransactionFilter::InputObject(input_obj_id)) => {
                let input_obj_seq = self
                    .state
                    .get_input_object_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_input_object(
                    input_obj_id.to_string(),
                    /* version */ None,
                    input_obj_seq,
                    limit + 1,
                    is_descending,
                )
            }
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
            // NOTE: more efficient to run this query over transactions table
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
                self.state
                    .get_transaction_digest_page_by_sender_recipient_address(
                        /* from */ None,
                        recipient_address.to_string(),
                        recipient_seq_number,
                        limit + 1,
                        is_descending,
                    )
            }
            Some(TransactionFilter::FromAndToAddress { from, to }) => {
                let recipient_seq_number = self
                    .state
                    .get_recipient_sequence_by_digest(cursor_str, is_descending)?;
                self.state
                    .get_transaction_digest_page_by_sender_recipient_address(
                        Some(from.to_string()),
                        to.to_string(),
                        recipient_seq_number,
                        limit + 1,
                        is_descending,
                    )
            }
            Some(TransactionFilter::TransactionKind(tx_kind_name)) => {
                let indexer_seq_number = self
                    .state
                    .get_transaction_sequence_by_digest(cursor_str, is_descending)?;
                self.state.get_transaction_digest_page_by_transaction_kind(
                    tx_kind_name,
                    indexer_seq_number,
                    limit + 1,
                    is_descending,
                )
            }
        }?;

        // digests here are of size (limit + 1), where the last one is the cursor for the next page
        let mut tx_digests = digests_from_db
            .iter()
            .map(|digest| {
                let tx_digest: Result<TransactionDigest, _> = digest.clone().parse();
                tx_digest.map_err(|e| {
                    IndexerError::SerdeError(format!(
                        "Failed to deserialize transaction digest: {:?} with error {:?}",
                        digest, e
                    ))
                })
            })
            .collect::<Result<Vec<TransactionDigest>, IndexerError>>()?;

        let has_next_page = tx_digests.len() > limit;
        tx_digests.truncate(limit);
        let next_cursor = tx_digests.last().cloned().map_or(cursor, Some);

        Ok(Page {
            data: tx_digests
                .into_iter()
                .map(SuiTransactionResponse::new)
                .collect(),
            next_cursor,
            has_next_page,
        })
    }
}

#[async_trait]
impl<S> IndexerApiServer for IndexerApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage> {
        self.fullnode
            .get_owned_objects(address, query, cursor, limit, at_checkpoint)
            .await
    }

    async fn query_transactions(
        &self,
        query: SuiTransactionResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        if !self
            .migrated_methods
            .contains(&"query_transactions".to_string())
        {
            return self
                .fullnode
                .query_transactions(query, cursor, limit, descending_order)
                .await;
        }
        Ok(self.query_transactions_internal(query, cursor, limit, descending_order)?)
    }

    async fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        if self.migrated_methods.contains(&"get_events".to_string()) {
            return self
                .fullnode
                .query_events(query, cursor, limit, descending_order)
                .await;
        }
        Ok(self.get_events_internal(query, cursor, limit, descending_order)?)
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

    fn subscribe_event(&self, sink: SubscriptionSink, filter: EventFilter) -> SubscriptionResult {
        spawn_subscription(sink, self.event_handler.subscribe(filter));
        Ok(())
    }
}

impl<S> SuiRpcModule for IndexerApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::IndexerApiOpenRpc::module_doc()
    }
}

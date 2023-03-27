// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::future::join_all;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;

use sui_json_rpc::api::{ReadApiClient, ReadApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    BigInt, Checkpoint, CheckpointId, CheckpointPage, SuiCheckpointSequenceNumber, SuiEvent,
    SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionResponse, SuiTransactionResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;

use crate::errors::IndexerError;
use crate::store::IndexerStore;
use crate::types::{SuiTransactionFullResponse, SuiTransactionFullResponseWithOptions};

pub(crate) struct ReadApi<S> {
    fullnode: HttpClient,
    state: S,
    migrated_methods: Vec<String>,
}

impl<S: IndexerStore> ReadApi<S> {
    pub fn new(state: S, fullnode_client: HttpClient, migrated_methods: Vec<String>) -> Self {
        Self {
            state,
            fullnode: fullnode_client,
            migrated_methods,
        }
    }

    fn get_total_transaction_number_internal(&self) -> Result<u64, IndexerError> {
        self.state
            .get_total_transaction_number_from_checkpoints()
            .map(|n| n as u64)
    }

    async fn get_transaction_with_options_internal(
        &self,
        digest: &TransactionDigest,
        options: Option<SuiTransactionResponseOptions>,
    ) -> Result<SuiTransactionResponse, IndexerError> {
        let tx = self
            .state
            .get_transaction_by_digest(&digest.base58_encode())?;
        let tx_full_resp: SuiTransactionFullResponse = self
            .state
            .compose_full_transaction_response(tx, options.clone())
            .await?;

        let sui_transaction_response = SuiTransactionFullResponseWithOptions {
            response: tx_full_resp,
            options: options.unwrap_or_default(),
        }
        .into();
        Ok(sui_transaction_response)
    }

    async fn multi_get_transactions_with_options_internal(
        &self,
        digests: &[TransactionDigest],
        options: Option<SuiTransactionResponseOptions>,
    ) -> Result<Vec<SuiTransactionResponse>, IndexerError> {
        let digest_strs = digests
            .iter()
            .map(|digest| digest.base58_encode())
            .collect::<Vec<_>>();
        let tx_vec = self.state.multi_get_transactions_by_digests(&digest_strs)?;
        let ordered_tx_vec = digest_strs
            .iter()
            .filter_map(|digest| {
                tx_vec
                    .iter()
                    .find(|tx| tx.transaction_digest == *digest)
                    .cloned()
            })
            .collect::<Vec<_>>();
        if ordered_tx_vec.len() != tx_vec.len() {
            return Err(IndexerError::PostgresReadError(
                "Transaction count changed after reorder, this should never happen.".to_string(),
            ));
        }
        let tx_full_resp_futures = ordered_tx_vec.into_iter().map(|tx| {
            self.state
                .compose_full_transaction_response(tx, options.clone())
        });
        let tx_full_resp_vec: Vec<SuiTransactionFullResponse> = join_all(tx_full_resp_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        let tx_resp_vec: Vec<SuiTransactionResponse> =
            tx_full_resp_vec.into_iter().map(|tx| tx.into()).collect();
        Ok(tx_resp_vec)
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
}

#[async_trait]
impl<S> ReadApiServer for ReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        if !self
            .migrated_methods
            .contains(&"get_object_with_options".into())
        {
            return self.fullnode.get_object(object_id, options).await;
        }

        Ok(self.get_object_with_options_internal(object_id, options)?)
    }

    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        return self.fullnode.multi_get_objects(object_ids, options).await;
    }

    async fn get_total_transaction_number(&self) -> RpcResult<BigInt> {
        if !self
            .migrated_methods
            .contains(&"get_total_transaction_number".to_string())
        {
            return self.fullnode.get_total_transaction_number().await;
        }
        Ok(self.get_total_transaction_number_internal()?.into())
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<SuiTransactionResponse> {
        if !self
            .migrated_methods
            .contains(&"get_transaction".to_string())
        {
            return self.fullnode.get_transaction(digest, options).await;
        }
        Ok(self
            .get_transaction_with_options_internal(&digest, options)
            .await?)
    }

    async fn multi_get_transactions(
        &self,
        digests: Vec<TransactionDigest>,
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionResponse>> {
        if !self
            .migrated_methods
            .contains(&"multi_get_transactions_with_options".to_string())
        {
            return self.fullnode.multi_get_transactions(digests, options).await;
        }
        Ok(self
            .multi_get_transactions_with_options_internal(&digests, options)
            .await?)
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

    async fn get_latest_checkpoint_sequence_number(
        &self,
    ) -> RpcResult<SuiCheckpointSequenceNumber> {
        if !self
            .migrated_methods
            .contains(&"get_latest_checkpoint_sequence_number".to_string())
        {
            return self.fullnode.get_latest_checkpoint_sequence_number().await;
        }
        Ok(self
            .get_latest_checkpoint_sequence_number_internal()?
            .into())
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        if !self
            .migrated_methods
            .contains(&"get_checkpoint".to_string())
        {
            return self.fullnode.get_checkpoint(id).await;
        }
        Ok(self.state.get_checkpoint(id)?)
    }

    async fn get_checkpoints(
        &self,
        cursor: Option<SuiCheckpointSequenceNumber>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        return self
            .fullnode
            .get_checkpoints(cursor, limit, descending_order)
            .await;
    }

    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        self.fullnode.get_events(transaction_digest).await
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

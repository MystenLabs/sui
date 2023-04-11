// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::executor::block_on;
use futures::future::join_all;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;

use sui_json_rpc::api::{ReadApiClient, ReadApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, CheckpointPage, SuiEvent, SuiGetPastObjectRequest,
    SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;
use sui_types::sui_serde::BigInt;

use crate::errors::IndexerError;
use crate::store::IndexerStore;
use crate::types::SuiTransactionBlockResponseWithOptions;

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

    async fn get_total_transaction_blocks_internal(&self) -> Result<u64, IndexerError> {
        self.state
            .get_total_transaction_number_from_checkpoints()
            .await
            .map(|n| n as u64)
    }

    async fn get_transaction_block_internal(
        &self,
        digest: &TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError> {
        let tx = self
            .state
            .get_transaction_by_digest(&digest.base58_encode())
            .await?;
        let sui_tx_resp = self
            .state
            .compose_sui_transaction_block_response(tx, options.as_ref())
            .await?;
        let sui_transaction_response = SuiTransactionBlockResponseWithOptions {
            response: sui_tx_resp,
            options: options.unwrap_or_default(),
        }
        .into();
        Ok(sui_transaction_response)
    }

    async fn multi_get_transaction_blocks_internal(
        &self,
        digests: &[TransactionDigest],
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<Vec<SuiTransactionBlockResponse>, IndexerError> {
        let digest_strs = digests
            .iter()
            .map(|digest| digest.base58_encode())
            .collect::<Vec<_>>();
        let tx_vec = self
            .state
            .multi_get_transactions_by_digests(&digest_strs)
            .await?;
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
        let sui_tx_resp_futures = ordered_tx_vec.into_iter().map(|tx| {
            self.state
                .compose_sui_transaction_block_response(tx, options.as_ref())
        });
        let sui_tx_resp_vec = join_all(sui_tx_resp_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sui_tx_resp_vec)
    }

    async fn get_object_internal(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<SuiObjectResponse, IndexerError> {
        let read = self.state.get_object(object_id, None).await?;
        Ok((read, options.unwrap_or_default()).try_into()?)
    }

    async fn get_latest_checkpoint_sequence_number_internal(&self) -> Result<u64, IndexerError> {
        self.state
            .get_latest_checkpoint_sequence_number()
            .await
            .map(|n| n as u64)
    }
}

#[async_trait]
impl<S> ReadApiServer for ReadApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        if !self.migrated_methods.contains(&"get_object".into()) {
            let obj_guard = self
                .state
                .indexer_metrics()
                .get_object_latency
                .start_timer();
            let obj_resp = block_on(async { self.fullnode.get_object(object_id, options).await });
            obj_guard.stop_and_record();
            return obj_resp;
        }

        Ok(block_on(self.get_object_internal(object_id, options))?)
    }

    fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        let objs_guard = self
            .state
            .indexer_metrics()
            .multi_get_objects_latency
            .start_timer();
        let objs_resp =
            block_on(async { self.fullnode.multi_get_objects(object_ids, options).await });
        objs_guard.stop_and_record();
        objs_resp
    }

    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>> {
        if !self
            .migrated_methods
            .contains(&"get_total_transaction_blocks".to_string())
        {
            let total_tx_guard = self
                .state
                .indexer_metrics()
                .get_total_transaction_blocks_latency
                .start_timer();
            let total_tx_resp = self.fullnode.get_total_transaction_blocks().await;
            total_tx_guard.stop_and_record();
            return total_tx_resp;
        }
        Ok(self.get_total_transaction_blocks_internal().await?.into())
    }

    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        if !self
            .migrated_methods
            .contains(&"get_transaction_block".to_string())
        {
            let tx_guard = self
                .state
                .indexer_metrics()
                .get_transaction_block_latency
                .start_timer();
            let tx_resp = self.fullnode.get_transaction_block(digest, options).await;
            tx_guard.stop_and_record();
            return tx_resp;
        }
        Ok(self
            .get_transaction_block_internal(&digest, options)
            .await?)
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: Vec<TransactionDigest>,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>> {
        if !self
            .migrated_methods
            .contains(&"multi_get_transaction_blocks".to_string())
        {
            let multi_tx_guard = self
                .state
                .indexer_metrics()
                .multi_get_transaction_blocks_latency
                .start_timer();
            let multi_tx_resp =
                block_on(self.fullnode.multi_get_transaction_blocks(digests, options));
            multi_tx_guard.stop_and_record();
            return multi_tx_resp;
        }
        Ok(block_on(
            self.multi_get_transaction_blocks_internal(&digests, options),
        )?)
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let past_obj_guard = self
            .state
            .indexer_metrics()
            .try_get_past_object_latency
            .start_timer();
        let past_obj_resp = self
            .fullnode
            .try_get_past_object(object_id, version, options)
            .await;
        past_obj_guard.stop_and_record();
        past_obj_resp
    }

    fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        let multi_past_obj_guard = self
            .state
            .indexer_metrics()
            .try_multi_get_past_objects_latency
            .start_timer();
        let multi_past_obj_resp = block_on(
            self.fullnode
                .try_multi_get_past_objects(past_objects, options),
        );
        multi_past_obj_guard.stop_and_record();
        multi_past_obj_resp
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>> {
        if !self
            .migrated_methods
            .contains(&"get_latest_checkpoint_sequence_number".to_string())
        {
            let latest_cp_guard = self
                .state
                .indexer_metrics()
                .get_latest_checkpoint_sequence_number_latency
                .start_timer();
            let latest_cp_resp = self.fullnode.get_latest_checkpoint_sequence_number().await;
            latest_cp_guard.stop_and_record();
            return latest_cp_resp;
        }
        Ok(self
            .get_latest_checkpoint_sequence_number_internal()
            .await?
            .into())
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        if !self
            .migrated_methods
            .contains(&"get_checkpoint".to_string())
        {
            let cp_guard = self
                .state
                .indexer_metrics()
                .get_checkpoint_latency
                .start_timer();
            let cp_resp = self.fullnode.get_checkpoint(id).await;
            cp_guard.stop_and_record();
            return cp_resp;
        }
        Ok(self.state.get_checkpoint(id).await?)
    }

    fn get_checkpoints(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<BigInt<u64>>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        let cps_guard = self
            .state
            .indexer_metrics()
            .get_checkpoints_latency
            .start_timer();
        let cps_resp = block_on(
            self.fullnode
                .get_checkpoints(cursor, limit, descending_order),
        );
        cps_guard.stop_and_record();
        cps_resp
    }

    fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        let events_guard = self
            .state
            .indexer_metrics()
            .get_events_latency
            .start_timer();
        let events_resp = block_on(self.fullnode.get_events(transaction_digest));
        events_guard.stop_and_record();
        events_resp
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

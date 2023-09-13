// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove after the functions are implemented
#![allow(unused_variables)]
#![allow(dead_code)]

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use crate::store::IndexerStoreV2;
use sui_json_rpc::api::ReadApiServer;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, CheckpointPage, ProtocolConfigResponse, SuiEvent,
    SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;
use sui_types::sui_serde::BigInt;

use sui_json_rpc_types::SuiLoadedChildObjectsResponse;

pub(crate) struct ReadApiV2<S> {
    state: S,
}

impl<S: IndexerStoreV2> ReadApiV2<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }
}

#[async_trait]
impl<S> ReadApiServer for ReadApiV2<S>
where
    S: IndexerStoreV2 + Sync + Send + 'static,
{
    async fn get_object(
        &self,
        _object_id: ObjectID,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        unimplemented!()
    }

    async fn multi_get_objects(
        &self,
        _object_ids: Vec<ObjectID>,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        unimplemented!()
    }

    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>> {
        unimplemented!()
    }

    async fn get_transaction_block(
        &self,
        _digest: TransactionDigest,
        _options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        unimplemented!()
    }

    async fn multi_get_transaction_blocks(
        &self,
        _digests: Vec<TransactionDigest>,
        _options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>> {
        unimplemented!()
    }

    async fn try_get_past_object(
        &self,
        _object_id: ObjectID,
        _version: SequenceNumber,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        unimplemented!()
    }

    async fn try_multi_get_past_objects(
        &self,
        _past_objects: Vec<SuiGetPastObjectRequest>,
        _options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        unimplemented!()
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>> {
        unimplemented!()
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        unimplemented!()
    }

    async fn get_checkpoints(
        &self,
        _cursor: Option<BigInt<u64>>,
        _limit: Option<usize>,
        _descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        unimplemented!()
    }

    async fn get_checkpoints_deprecated_limit(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<BigInt<u64>>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        unimplemented!()
    }

    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        unimplemented!()
    }

    async fn get_loaded_child_objects(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiLoadedChildObjectsResponse> {
        unimplemented!()
    }

    async fn get_protocol_config(
        &self,
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse> {
        unimplemented!()
    }

    async fn get_chain_identifier(&self) -> RpcResult<String> {
        unimplemented!()
    }
}

impl<S> SuiRpcModule for ReadApiV2<S>
where
    S: IndexerStoreV2 + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ReadApiOpenRpc::module_doc()
    }
}

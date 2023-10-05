// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc::api::ExtendedApiServer;
use sui_json_rpc::SuiRpcModule;

use sui_json_rpc_types::{
    AddressMetrics, CheckpointedObjectID, EpochInfo, EpochPage, MoveCallMetrics, NetworkMetrics,
    Page, QueryObjectsPage, SuiObjectDataFilter, SuiObjectResponse, SuiObjectResponseQuery,
};

use crate::errors::IndexerError;
use crate::store::IndexerStoreV2;

pub(crate) struct ExtendedApiV2<S> {
    state: S,
}

impl<S: IndexerStoreV2> ExtendedApiV2<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }
}

#[async_trait]
impl<S> ExtendedApiServer for ExtendedApiV2<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn get_epochs(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EpochPage> {
        unimplemented!()
    }

    async fn get_current_epoch(&self) -> RpcResult<EpochInfo> {
        unimplemented!()
    }

    async fn query_objects(
        &self,
        query: SuiObjectResponseQuery,
        cursor: Option<CheckpointedObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<QueryObjectsPage> {
        unimplemented!()
    }

    async fn get_network_metrics(&self) -> RpcResult<NetworkMetrics> {
        todo!()
    }

    async fn get_move_call_metrics(&self) -> RpcResult<MoveCallMetrics> {
        todo!()
    }

    async fn get_latest_address_metrics(&self) -> RpcResult<AddressMetrics> {
        todo!()
    }

    async fn get_checkpoint_address_metrics(&self, checkpoint: u64) -> RpcResult<AddressMetrics> {
        todo!()
    }

    async fn get_all_epoch_address_metrics(
        &self,
        descending_order: Option<bool>,
    ) -> RpcResult<Vec<AddressMetrics>> {
        todo!()
    }

    async fn get_total_transactions(&self) -> RpcResult<BigInt<u64>> {
        todo!()
    }
}

impl<S> SuiRpcModule for ExtendedApiV2<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ExtendedApiOpenRpc::module_doc()
    }
}

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::RpcReadApiServer;
use crate::api::{RpcFullNodeReadApiServer, SuiRpcModule};
use crate::rpc_gateway::responses::ObjectResponse;
use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    rpc_gateway::responses::SuiTypeTag,
};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_core::gateway_types::{GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse};
use sui_core::{
    authority::AuthorityState,
    gateway_types::{GetObjectInfoResponse, TransactionEffectsResponse, TransactionResponse},
};
use sui_json::SuiJsonValue;
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};

// An implementation of the read portion of the Gateway JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

pub struct FullNodeApi {
    pub state: Arc<AuthorityState>,
}

impl FullNodeApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

impl ReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl RpcReadApiServer for ReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<ObjectInfo>> {
        Ok(self
            .client
            .get_owner_objects(Owner::AddressOwner(address))?)
    }

    async fn get_objects_owned_by_object(&self, object_id: ObjectID) -> RpcResult<Vec<ObjectInfo>> {
        Ok(self
            .client
            .get_owner_objects(Owner::ObjectOwner(object_id.into()))?)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        Ok(self
            .state
            .get_object_info(&object_id)
            .await
            .map_err(|e| anyhow!("{}", e))?
            .try_into()
            .map_err(|e| anyhow!("{}", e))?)
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        Ok(self.state.get_transaction(digest).await?)
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcReadApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl RpcFullNodeReadApiServer for FullNodeApi {
    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_by_input_object(object).await?)
    }

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .state
            .get_transactions_by_mutated_object(object)
            .await?)
    }

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_from_addr(addr).await?)
    }

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.state.get_transactions_to_addr(addr).await?)
    }
}

impl SuiRpcModule for FullNodeApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcFullNodeReadApiOpenRpc::module_doc()
    }
}

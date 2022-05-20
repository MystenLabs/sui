// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::RpcFullNodeReadApiServer;
use crate::api::RpcReadApiServer;
use crate::rpc_gateway::responses::ObjectResponse;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_core::gateway_types::{GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};

// An implementation of the read portion of the Gateway JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

pub struct SuiFullNode {
    pub state: Arc<AuthorityState>,
}

impl SuiFullNode {
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
    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        let resp = ObjectResponse {
            objects: self
                .state
                .get_owned_objects(owner)
                .await
                .map_err(|e| anyhow!("{}", e))?
                .iter()
                .map(|w| SuiObjectRef::from(*w))
                .collect(),
        };
        Ok(resp)
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

#[async_trait]
impl RpcFullNodeReadApiServer for SuiFullNode {
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

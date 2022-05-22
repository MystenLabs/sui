// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    rpc_gateway::responses::{ObjectResponse, SuiTypeTag},
};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use std::sync::Arc;
use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_core::{
    authority::AuthorityState,
    gateway_types::{
        GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse, TransactionResponse,
    },
};
use sui_json::SuiJsonValue;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::sui_serde::Base64;

// An implementation of the read portion of the Gateway JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

#[async_trait]
impl RpcGatewayServer for ReadApi {
    async fn transfer_coin(
        &self,
        _signer: SuiAddress,
        _object_id: ObjectID,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
        _recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn publish(
        &self,
        _sender: SuiAddress,
        _compiled_modules: Vec<Base64>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn split_coin(
        &self,
        _signer: SuiAddress,
        _coin_object_id: ObjectID,
        _split_amounts: Vec<u64>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn merge_coin(
        &self,
        _signer: SuiAddress,
        _primary_coin: ObjectID,
        _coin_to_merge: ObjectID,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn execute_transaction(
        &self,
        _tx_bytes: Base64,
        _signature: Base64,
        _pub_key: Base64,
    ) -> RpcResult<TransactionResponse> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn move_call(
        &self,
        _signer: SuiAddress,
        _package_object_id: ObjectID,
        _module: String,
        _function: String,
        _type_arguments: Vec<SuiTypeTag>,
        _rpc_arguments: Vec<SuiJsonValue>,
        _gas: Option<ObjectID>,
        _gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        Err(anyhow!("Sui Node only supports read-only methods").into())
    }

    async fn sync_account_state(&self, _address: SuiAddress) -> RpcResult<()> {
        todo!()
    }

    //
    // Read APIs
    //

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

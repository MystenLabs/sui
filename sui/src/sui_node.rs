// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    rpc_gateway::responses::{ObjectResponse, SuiTypeTag},
};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use sui_core::gateway_types::{TransactionEffectsResponse, TransactionResponse};

use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_core::gateway_types::GetObjectInfoResponse;
use sui_core::sui_json::SuiJsonValue;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::sui_serde::Base64;

pub struct SuiNode {}

impl SuiNode {
    pub fn new(_config_path: &Path) -> anyhow::Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl RpcGatewayServer for SuiNode {
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

    async fn get_owned_objects(&self, _owner: SuiAddress) -> RpcResult<ObjectResponse> {
        todo!()
    }

    async fn get_object_info(&self, _object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        todo!()
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

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        todo!()
    }

    async fn get_transactions_in_range(
        &self,
        _start: GatewayTxSeqNumber,
        _end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        todo!()
    }

    async fn get_recent_transactions(
        &self,
        _count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        todo!()
    }

    async fn get_transaction(
        &self,
        _digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        todo!()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use sui_json_rpc::api::{WriteApiClient, WriteApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{DevInspectResults, SuiTransactionEffects, SuiTransactionResponse};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, SuiAddress};
use sui_types::messages::ExecuteTransactionRequestType;

pub(crate) struct WriteApi {
    fullnode: HttpClient,
}

impl WriteApi {
    pub fn new(fullnode_client: HttpClient) -> Self {
        Self {
            fullnode: fullnode_client,
        }
    }
}

#[async_trait]
impl WriteApiServer for WriteApi {
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiTransactionResponse> {
        self.submit_transaction(tx_bytes, vec![signature], request_type)
            .await
    }

    async fn execute_transaction_serialized_sig(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiTransactionResponse> {
        self.execute_transaction(tx_bytes, signature, request_type)
            .await
    }

    async fn submit_transaction(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiTransactionResponse> {
        self.fullnode
            .submit_transaction(tx_bytes, signatures, request_type)
            .await
    }

    async fn dev_inspect_transaction(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<u64>,
        epoch: Option<EpochId>,
    ) -> RpcResult<DevInspectResults> {
        self.fullnode
            .dev_inspect_transaction(sender_address, tx_bytes, gas_price, epoch)
            .await
    }

    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<SuiTransactionEffects> {
        self.fullnode.dry_run_transaction(tx_bytes).await
    }
}

impl SuiRpcModule for WriteApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::WriteApiOpenRpc::module_doc()
    }
}

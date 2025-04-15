// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;

use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{WriteApiClient, WriteApiServer};
use sui_json_rpc_types::{
    DevInspectArgs, DevInspectResults, DryRunTransactionBlockResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::SuiAddress;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::sui_serde::BigInt;

use crate::types::SuiTransactionBlockResponseWithOptions;

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
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        let sui_transaction_response = self
            .fullnode
            .execute_transaction_block(tx_bytes, signatures, options.clone(), request_type)
            .await
            .map_err(crate::errors::client_error_to_error_object)?;
        Ok(SuiTransactionBlockResponseWithOptions {
            response: sui_transaction_response,
            options: options.unwrap_or_default(),
        }
        .into())
    }

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        epoch: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults> {
        self.fullnode
            .dev_inspect_transaction_block(
                sender_address,
                tx_bytes,
                gas_price,
                epoch,
                additional_args,
            )
            .await
            .map_err(crate::errors::client_error_to_error_object)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        self.fullnode
            .dry_run_transaction_block(tx_bytes)
            .await
            .map_err(crate::errors::client_error_to_error_object)
    }
}

impl SuiRpcModule for WriteApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc_api::WriteApiOpenRpc::module_doc()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::TransactionExecutionApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_metrics::spawn_monitored_task;
use signature::Signature;
use std::sync::Arc;
use sui_core::authority::{AuthorityStore, ResolverWrapper};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_types::SuiExecuteTransactionResponse;
use sui_open_rpc::Module;
use sui_types::intent::Intent;
use sui_types::messages::{ExecuteTransactionRequest, ExecuteTransactionRequestType};
use sui_types::{crypto, messages::Transaction};

pub struct FullNodeTransactionExecutionApi {
    pub transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    pub module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>,
}

impl FullNodeTransactionExecutionApi {
    pub fn new(
        transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
        module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>,
    ) -> Self {
        Self {
            transaction_orchestrator,
            module_cache,
        }
    }
}

#[async_trait]
impl TransactionExecutionApiServer for FullNodeTransactionExecutionApi {
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse> {
        let tx_data =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
        let signature = crypto::Signature::from_bytes(&signature.to_vec().map_err(|e| anyhow!(e))?)
            .map_err(|e| anyhow!(e))?;

        let txn = Transaction::from_data(tx_data, Intent::default(), signature);

        let transaction_orchestrator = self.transaction_orchestrator.clone();
        let response = spawn_monitored_task!(transaction_orchestrator.execute_transaction(
            ExecuteTransactionRequest {
                transaction: txn,
                request_type,
            }
        ))
        .await
        .map_err(|e| anyhow!(e))? // for JoinError
        .map_err(|e| anyhow!(e))?; // For Sui transaction execution error (SuiResult<ExecuteTransactionResponse>)

        SuiExecuteTransactionResponse::from_execute_transaction_response(
            response,
            self.module_cache.as_ref(),
        )
        .map_err(jsonrpsee::core::Error::from)
    }

    async fn execute_transaction_serialized_sig(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse> {
        let tx_data =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
        let signature = crypto::Signature::from_bytes(&signature.to_vec().map_err(|e| anyhow!(e))?)
            .map_err(|e| anyhow!(e))?;

        let txn = Transaction::from_data(tx_data, Intent::default(), signature);

        let transaction_orchestrator = self.transaction_orchestrator.clone();
        let response = spawn_monitored_task!(transaction_orchestrator.execute_transaction(
            ExecuteTransactionRequest {
                transaction: txn,
                request_type,
            }
        ))
        .await
        .map_err(|e| anyhow!(e))? // for JoinError
        .map_err(|e| anyhow!(e))?; // For Sui transaction execution error (SuiResult<ExecuteTransactionResponse>)

        SuiExecuteTransactionResponse::from_execute_transaction_response(
            response,
            self.module_cache.as_ref(),
        )
        .map_err(jsonrpsee::core::Error::from)
    }
}

impl SuiRpcModule for FullNodeTransactionExecutionApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::TransactionExecutionApiOpenRpc::module_doc()
    }
}

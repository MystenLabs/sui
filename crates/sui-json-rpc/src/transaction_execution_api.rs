// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::TransactionExecutionApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use move_bytecode_utils::module_cache::SyncModuleCache;
use signature::Signature;
use std::sync::Arc;
use sui_core::authority::{AuthorityStore, ResolverWrapper};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_types::SuiExecuteTransactionResponse;
use sui_open_rpc::Module;
use sui_types::crypto::SignatureScheme;
use sui_types::intent::IntentMessage;
use sui_types::messages::{ExecuteTransactionRequest, ExecuteTransactionRequestType};
use sui_types::sui_serde::Base64;
use sui_types::{
    crypto,
    messages::{Transaction, TransactionData},
};

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
        sig_scheme: SignatureScheme,
        signature: Base64,
        pub_key: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse> {
        let intent_msg = IntentMessage::<TransactionData>::from_bytes(&tx_bytes.to_vec()?)?;
        let flag = vec![sig_scheme.flag()];
        let signature = crypto::Signature::from_bytes(
            &[&*flag, &*signature.to_vec()?, &pub_key.to_vec()?].concat(),
        )
        .map_err(|e| anyhow!(e))?;
        let txn = Transaction::new(intent_msg.value, intent_msg.intent, signature);
        let txn_digest = *txn.digest();

        let response = self
            .transaction_orchestrator
            .execute_transaction(ExecuteTransactionRequest {
                transaction: txn,
                request_type,
            })
            .await
            .map_err(|e| anyhow!(e))?;
        SuiExecuteTransactionResponse::from_execute_transaction_response(
            response,
            txn_digest,
            self.module_cache.as_ref(),
        )
        .map_err(jsonrpsee_core::Error::from)
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

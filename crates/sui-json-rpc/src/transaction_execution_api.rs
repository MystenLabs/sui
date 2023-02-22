// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::TransactionExecutionServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_metrics::spawn_monitored_task;
use std::sync::Arc;
use sui_core::authority::{AuthorityStore, ResolverWrapper};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_types::{SuiTransactionEffects, SuiTransactionResponse};
use sui_open_rpc::Module;
use sui_types::intent::Intent;
use sui_types::messages::{ExecuteTransactionRequest, ExecuteTransactionRequestType};
use sui_types::messages::{ExecuteTransactionResponse, Transaction};
use sui_types::signature::GenericSignature;

pub struct TransactionExecutionApi {
    pub transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    pub module_cache: Arc<SyncModuleCache<ResolverWrapper<AuthorityStore>>>,
}

impl TransactionExecutionApi {
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
impl TransactionExecutionServer for TransactionExecutionApi {
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiTransactionResponse> {
        self.submit_transaction(tx_bytes, vec![signature], request_type)
            .await
    }

    // TODO: remove this or execute_transaction
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
        let tx_data =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(
                GenericSignature::from_bytes(&sig.to_vec().map_err(|e| anyhow!(e))?)
                    .map_err(|e| anyhow!(e))?,
            );
        }

        let txn = Transaction::from_generic_sig_data(tx_data, Intent::default(), sigs);
        let tx = txn.data().clone().try_into()?;

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

        match response {
            ExecuteTransactionResponse::EffectsCert(cert) => {
                let (_, effects, is_executed_locally) = *cert;
                Ok(SuiTransactionResponse {
                    transaction: tx,
                    effects: SuiTransactionEffects::try_from(
                        effects.effects,
                        self.module_cache.as_ref(),
                    )?,
                    timestamp_ms: None,
                    confirmed_local_execution: Some(is_executed_locally),
                    checkpoint: None,
                })
            }
        }
    }
}

impl SuiRpcModule for TransactionExecutionApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::TransactionExecutionOpenRpc::module_doc()
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::WriteApiServer;
use crate::read_api::get_transaction_data_and_digest;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use mysten_metrics::spawn_monitored_task;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_types::{
    DevInspectResults, DryRunTransactionResponse, SuiTransactionEvents, SuiTransactionResponse,
};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, SuiAddress};
use sui_types::intent::Intent;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, TransactionKind,
};
use sui_types::messages::{ExecuteTransactionResponse, Transaction};
use sui_types::signature::GenericSignature;

pub struct TransactionExecutionApi {
    state: Arc<AuthorityState>,
    transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
}

impl TransactionExecutionApi {
    pub fn new(
        state: Arc<AuthorityState>,
        transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    ) -> Self {
        Self {
            state,
            transaction_orchestrator,
        }
    }
}

#[async_trait]
impl WriteApiServer for TransactionExecutionApi {
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
                let (effects, events, is_executed_locally) = *cert;
                let module_cache = self
                    .state
                    .load_epoch_store_one_call_per_task()
                    .module_cache()
                    .clone();
                Ok(SuiTransactionResponse {
                    transaction: tx,
                    effects: effects.effects.try_into()?,
                    events: SuiTransactionEvents::try_from(events, module_cache.as_ref())?,
                    timestamp_ms: None,
                    confirmed_local_execution: Some(is_executed_locally),
                    checkpoint: None,
                })
            }
        }
    }

    async fn dev_inspect_transaction(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<u64>,
        _epoch: Option<EpochId>,
    ) -> RpcResult<DevInspectResults> {
        let tx_kind: TransactionKind =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
        Ok(self
            .state
            .dev_inspect_transaction(sender_address, tx_kind, gas_price)
            .await?)
    }

    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<DryRunTransactionResponse> {
        let (txn_data, txn_digest) = get_transaction_data_and_digest(tx_bytes)?;
        Ok(self
            .state
            .dry_exec_transaction(txn_data, txn_digest)
            .await?)
    }
}

impl SuiRpcModule for TransactionExecutionApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::WriteApiOpenRpc::module_doc()
    }
}

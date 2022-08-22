// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::{EstimatorApiServer, QuorumDriverApiServer};
use crate::SuiRpcModule;
use crate::quorum_driver_api::FullNodeQuorumDriverApi;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use move_bytecode_utils::module_cache::SyncModuleCache;
use signature::Signature;
use std::sync::Arc;
use sui_core::authority::{AuthorityStore, ResolverWrapper};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_json_rpc_types::{SuiExecuteTransactionResponse, SuiGasCostSummary};
use sui_open_rpc::Module;
use sui_quorum_driver::QuorumDriver;
use sui_types::crypto::SignatureScheme;
use sui_types::messages::{ExecuteTransactionRequest, ExecuteTransactionRequestType};
use sui_types::sui_serde::Base64;
use sui_types::{
    crypto,
    crypto::SignableBytes,
    messages::{Transaction, TransactionData},
};

pub struct EstimatorApi {}

#[async_trait]
impl EstimatorApiServer for EstimatorApi {
    async fn estimate_transaction_base_gas(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<SuiGasCostSummary> {
        let data = TransactionData::from_signable_bytes(&tx_bytes.to_vec()?)?;

        Ok(SuiGasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        })

        // SuiExecuteTransactionResponse::from_execute_transaction_response(
        //     response,
        //     txn_digest,
        //     self.module_cache.as_ref(),
        // )
        // .map_err(jsonrpsee_core::Error::from)
    }
}

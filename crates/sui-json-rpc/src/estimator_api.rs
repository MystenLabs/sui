// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::EstimatorApiServer;
use crate::SuiRpcModule;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use sui_cost::estimator::estimate_computational_costs_for_transaction;
use sui_json_rpc_types::SuiGasCostSummary;
use sui_open_rpc::Module;
use sui_types::sui_serde::Base64;
use sui_types::{crypto::SignableBytes, messages::TransactionData};

pub struct EstimatorApi {}

#[async_trait]
impl EstimatorApiServer for EstimatorApi {
    async fn estimate_transaction_computation_cost(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<SuiGasCostSummary> {
        let data = TransactionData::from_signable_bytes(&tx_bytes.to_vec()?)?;
        let est = estimate_computational_costs_for_transaction(data.kind)?;
        Ok(SuiGasCostSummary::from(est))
    }
}

impl SuiRpcModule for EstimatorApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EstimatorApiOpenRpc::module_doc()
    }
}

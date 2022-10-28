// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::EstimatorApiServer;
use crate::SuiRpcModule;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_cost::estimator::estimate_transaction_computation_cost;
use sui_json_rpc_types::SuiGasCostSummary;
use sui_open_rpc::Module;
use sui_types::crypto::SignableBytes;
use sui_types::messages::TransactionData;

use fastcrypto::encoding::Base64;

pub struct EstimatorApi {
    pub state: Arc<AuthorityState>,
}

impl EstimatorApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl EstimatorApiServer for EstimatorApi {
    async fn estimate_transaction_computation_cost(
        &self,
        tx_bytes: Base64,
        computation_gas_unit_price: Option<u64>,
        storage_gas_unit_price: Option<u64>,
        mutated_object_sizes_after: Option<usize>,
        storage_rebate: Option<u64>,
    ) -> RpcResult<SuiGasCostSummary> {
        let data = TransactionData::from_signable_bytes(
            &tx_bytes.to_vec().map_err(|e| anyhow::anyhow!(e))?,
        )?;

        Ok(SuiGasCostSummary::from(
            estimate_transaction_computation_cost(
                data,
                self.state.clone(),
                computation_gas_unit_price,
                storage_gas_unit_price,
                mutated_object_sizes_after,
                storage_rebate,
            )
            .await?,
        ))
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

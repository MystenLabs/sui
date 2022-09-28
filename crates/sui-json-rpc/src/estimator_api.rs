// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::EstimatorApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use signature::Signature;
use std::sync::Arc;
use sui_core::authority::{AuthorityState, TemporaryStore};
use sui_core::transaction_input_checker;
use sui_cost::estimator::estimate_transaction_inner;
use sui_json_rpc_types::SuiGasCostSummary;
use sui_open_rpc::Module;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::Signature as SuiSignature;
use sui_types::gas::SuiGas;
use sui_types::messages::Transaction;
use sui_types::sui_serde::Base64;
use sui_types::{crypto::SignableBytes, messages::TransactionData};

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
        computation_gas_unit_price: u64,
        storage_gas_unit_price: u64,
        mutated_object_sizes_after: Option<usize>,
        storage_rebate: u64,
    ) -> RpcResult<SuiGasCostSummary> {
        let data = TransactionData::from_signable_bytes(&tx_bytes.to_vec()?)?;

        // Make a dummy transaction
        let dummy_sig = SuiSignature::from_bytes(&[0]).map_err(|e| anyhow!("{e}"))?;
        let tx = Transaction::new(data, dummy_sig);

        let (_gas_status, input_objects) =
            transaction_input_checker::check_transaction_input(&self.state.db(), &tx)
                .await
                .map_err(|e| anyhow!("{e}"))?;
        let in_mem_temporary_store =
            TemporaryStore::new(self.state.db(), input_objects, TransactionDigest::random());

        Ok(SuiGasCostSummary::from(
            estimate_transaction_inner(
                tx.signed_data.data.kind,
                computation_gas_unit_price,
                storage_gas_unit_price,
                mutated_object_sizes_after,
                SuiGas::new(storage_rebate),
                &in_mem_temporary_store,
            )
            .map_err(|e| anyhow!("{e}"))?,
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

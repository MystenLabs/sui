// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use fastcrypto::encoding::Base64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use rand::rngs::OsRng;
use tokio::sync::RwLock;

use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::{api::rpc_module::RpcModule, error::invalid_params};
use sui_json_rpc_types::{
    DryRunTransactionBlockResponse, ProtocolConfigResponse, SuiObjectData, SuiObjectDataOptions,
    SuiObjectResponse, SuiTransactionBlock, SuiTransactionBlockEffects,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::{
    base_types::ObjectID,
    digests::ChainIdentifier,
    quorum_driver_types::ExecuteTransactionRequestType,
    sui_serde::BigInt,
    sui_system_state::{SuiSystemStateTrait, sui_system_state_summary::SuiSystemStateSummary},
    supported_protocol_versions::{self, Chain, ProtocolConfig},
    transaction::{Transaction, TransactionData},
};

use crate::rpc::read::ReadApiOpenRpc;
use crate::store::ForkingStore;

#[open_rpc(namespace = "suix", tag = "Governance API")]
#[rpc(server, namespace = "suix")]
trait GovernanceApi {
    /// Return the reference gas price for the network as of the latest epoch.
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>>;

    /// Return a summary of the latest version of the Sui System State object (0x5), on-chain.
    #[method(name = "getLatestSuiSystemState")]
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary>;
}

pub(crate) struct Governance {
    pub simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    pub protocol_version: u64,
    pub chain: Chain,
}

impl Governance {
    pub fn new(
        simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
        protocol_version: u64,
        chain: Chain,
    ) -> Self {
        Self {
            simulacrum,
            protocol_version,
            chain,
        }
    }
}

#[async_trait::async_trait]
impl GovernanceApiServer for Governance {
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        let sim = self.simulacrum.read().await;

        println!("Getting reference gas price from simulacrum");
        Ok(BigInt::from(sim.reference_gas_price()))
    }
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        let sim = self.simulacrum.read().await;
        let system_state = sim.store_1().get_system_state();

        Ok(system_state.into_sui_system_state_summary())
    }
}

impl RpcModule for Governance {
    fn schema(&self) -> Module {
        ReadApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

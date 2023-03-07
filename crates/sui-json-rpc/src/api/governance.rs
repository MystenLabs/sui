// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{SuiCommittee, SuiSystemStateRpc};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::SuiAddress;

use sui_types::committee::EpochId;
use sui_types::governance::DelegatedStake;

use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorMetadata;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

#[open_rpc(namespace = "sui", tag = "Governance Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait GovernanceReadApi {
    /// Return all [DelegatedStake].
    #[method(name = "getDelegatedStakes")]
    async fn get_delegated_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>>;

    /// Return all validators available for stake delegation.
    #[method(name = "getValidators")]
    async fn get_validators(&self) -> RpcResult<Vec<ValidatorMetadata>>;

    /// Return the committee information for the asked `epoch`.
    #[method(name = "getCommitteeInfo")]
    async fn get_committee_info(
        &self,
        /// The epoch of interest. If None, default to the latest epoch
        epoch: Option<EpochId>,
    ) -> RpcResult<SuiCommittee>;

    /// (Deprecated) Return latest SUI system state object on-chain.
    /// This is now deprecated in favor of get_latest_sui_system_state.
    #[method(name = "getSuiSystemState", deprecated)]
    async fn get_sui_system_state(&self) -> RpcResult<SuiSystemStateRpc>;

    /// Return the latest SUI system state object on-chain.
    #[method(name = "getLatestSuiSystemState")]
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary>;

    /// Return the reference gas price for the network
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<u64>;
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use std::sync::Arc;

use crate::api::GovernanceReadApiServer;
use crate::error::Error;
use crate::SuiRpcModule;
use async_trait::async_trait;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_open_rpc::Module;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::governance::{Delegation, PendingDelegation, StakedSui};
use sui_types::messages::{CommitteeInfoRequest, CommitteeInfoResponse};
use sui_types::sui_system_state::{SuiSystemState, ValidatorMetadata};

pub struct GovernanceReadApi {
    state: Arc<AuthorityState>,
}

impl GovernanceReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl GovernanceReadApiServer for GovernanceReadApi {
    async fn get_staked_sui(&self, owner: SuiAddress) -> RpcResult<Vec<StakedSui>> {
        Ok(self
            .state
            .get_move_objects(owner, &StakedSui::type_())
            .await
            .map_err(Error::from)?)
    }
    async fn get_delegations(&self, owner: SuiAddress) -> RpcResult<Vec<Delegation>> {
        Ok(self
            .state
            .get_move_objects(owner, &Delegation::type_())
            .await
            .map_err(Error::from)?)
    }

    async fn get_pending_delegations(
        &self,
        owner: SuiAddress,
    ) -> RpcResult<Vec<PendingDelegation>> {
        let system_state: SuiSystemState = self.get_sui_system_state().await?;
        let validators = system_state
            .validators
            .pending_validators
            .iter()
            .chain(system_state.validators.active_validators.iter());
        Ok(validators
            .flat_map(|v| {
                v.delegation_staking_pool
                    .pending_delegations
                    .iter()
                    .filter_map(|d| {
                        if d.delegator == owner {
                            Some(PendingDelegation {
                                validator_address: v.metadata.sui_address,
                                pool_starting_epoch: v.delegation_staking_pool.starting_epoch,
                                principal_sui_amount: d.sui_amount,
                            })
                        } else {
                            None
                        }
                    })
            })
            .collect::<Vec<_>>())
    }

    async fn next_epoch_validators(&self) -> RpcResult<Vec<ValidatorMetadata>> {
        Ok(self
            .state
            .get_sui_system_state_object()
            .await
            .map_err(Error::from)?
            .validators
            .next_epoch_validators)
    }

    async fn get_committee_info(&self, epoch: Option<EpochId>) -> RpcResult<CommitteeInfoResponse> {
        Ok(self
            .state
            .handle_committee_info_request(&CommitteeInfoRequest { epoch })
            .map_err(Error::from)?)
    }

    async fn get_sui_system_state(&self) -> RpcResult<SuiSystemState> {
        Ok(self
            .state
            .get_sui_system_state_object()
            .await
            .map_err(Error::from)?)
    }
}

impl SuiRpcModule for GovernanceReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::GovernanceReadApiOpenRpc::module_doc()
    }
}

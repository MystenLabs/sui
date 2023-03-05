// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_json_rpc_types::SuiCommittee;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

use crate::api::GovernanceReadApiServer;
use crate::error::Error;
use crate::SuiRpcModule;
use async_trait::async_trait;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{DelegatedStake, Delegation, DelegationStatus};
use sui_open_rpc::Module;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::governance::StakedSui;
use sui_types::governance::{DelegatedStake, Delegation, DelegationStatus, StakedSui};
use sui_types::messages::{CommitteeInfoRequest, CommitteeInfoResponse};
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::{
    PoolTokenExchangeRate, SuiSystemState, SuiSystemStateTrait, ValidatorMetadata,
};

pub struct GovernanceReadApi {
    state: Arc<AuthorityState>,
}

impl GovernanceReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }

    async fn get_staked_sui(&self, owner: SuiAddress) -> Result<Vec<StakedSui>, Error> {
        Ok(self
            .state
            .get_move_objects(owner, &StakedSui::type_())
            .await?)
    }

    async fn get_delegated_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error> {
        let stakes = self.get_staked_sui(owner).await?;
        if stakes.is_empty() {
            return Ok(vec![]);
        }

        let pools = stakes
            .into_iter()
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut pools, s| {
                pools.entry(s.pool_id()).or_default().push(s);
                pools
            });

        let system_state = self.get_system_state()?;

        let mut delegated_stakes = vec![];
        for (pool_id, stakes) in pools {
            let validator_address: SuiAddress = self
                .state
                .read_table_value(system_state.staking_pool_mappings(), &pool_id)
                .await
                .ok_or_else(|| {
                    Error::UnexpectedError(format!(
                        "Cannot find validator mapping for staking pool {pool_id}"
                    ))
                })?;
            let pool = system_state.get_staking_pool(&pool_id).ok_or_else(|| {
                Error::UnexpectedError(format!("Cannot find staking pool [{pool_id}]"))
            })?;

            let current_rate: PoolTokenExchangeRate = self
                .state
                .read_table_value(&pool.exchange_rates, &system_state.epoch())
                .await
                .ok_or_else(|| {
                    Error::UnexpectedError(format!(
                        "Cannot find exchange rate for pool [{pool_id}] at epoch {}",
                        system_state.epoch()
                    ))
                })?;

            let mut delegations = vec![];
            for stake in stakes {
                // delegation will be active in next epoch
                let status = if system_state.epoch() >= stake.request_epoch() {
                    let stake_rate: PoolTokenExchangeRate = self
                        .state
                        .read_table_value(&pool.exchange_rates, &stake.request_epoch())
                        .await
                        .ok_or_else(|| {
                            Error::UnexpectedError(format!(
                                "Cannot find exchange rate for pool [{pool_id}] at epoch {}",
                                system_state.epoch()
                            ))
                        })?;
                    let estimated_reward = (((stake_rate.rate() / current_rate.rate()) - 1.0)
                        * stake.principal() as f64)
                        .round() as u64;
                    DelegationStatus::Active { estimated_reward }
                } else {
                    DelegationStatus::Pending
                };
                delegations.push(Delegation {
                    staked_sui_id: stake.id(),
                    delegation_request_epoch: stake.request_epoch(),
                    principal: stake.principal(),
                    token_lock: stake.sui_token_lock(),
                    status,
                })
            }

            delegated_stakes.push(DelegatedStake {
                validator_address,
                staking_pool: pool_id,
                delegations,
            })
        }
        Ok(delegated_stakes)
    }

    fn get_system_state(&self) -> Result<SuiSystemState, Error> {
        Ok(self.state.database.get_sui_system_state_object()?)
    }
}

#[async_trait]
impl GovernanceReadApiServer for GovernanceReadApi {
    async fn get_delegated_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>> {
        Ok(self.get_delegated_stakes(owner).await?)
    }

    async fn get_committee_info(&self, epoch: Option<EpochId>) -> RpcResult<SuiCommittee> {
        Ok(self
            .state
            .committee_store()
            .get_or_latest_committee(epoch)
            .map(|committee| committee.into())
            .map_err(Error::from)?)
    }

    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        Ok(self
            .state
            .database
            .get_sui_system_state_object()
            .map_err(Error::from)?
            .into_sui_system_state_summary())
    }

    async fn get_reference_gas_price(&self) -> RpcResult<u64> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        Ok(epoch_store.reference_gas_price())
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::max;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use sui_core::authority::AuthorityState;
use sui_json_rpc_types::SuiCommittee;
use sui_json_rpc_types::{DelegatedStake, Stake, StakeStatus};
use sui_open_rpc::Module;
use sui_types::base_types::{MoveObjectType, ObjectID, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::dynamic_field::get_dynamic_field_from_store;
use sui_types::error::{SuiError, UserInputError};
use sui_types::governance::StakedSui;
use sui_types::id::ID;
use sui_types::object::ObjectRead;
use sui_types::sui_serde::BigInt;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::PoolTokenExchangeRate;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::{
    get_validator_from_table, sui_system_state_summary::get_validator_by_pool_id, SuiSystemState,
};

use crate::api::{GovernanceReadApiServer, JsonRpcMetrics};
use crate::error::Error;
use crate::SuiRpcModule;

pub struct GovernanceReadApi {
    state: Arc<AuthorityState>,
    pub metrics: Arc<JsonRpcMetrics>,
}

impl GovernanceReadApi {
    pub fn new(state: Arc<AuthorityState>, metrics: Arc<JsonRpcMetrics>) -> Self {
        Self { state, metrics }
    }

    async fn get_staked_sui(&self, owner: SuiAddress) -> Result<Vec<StakedSui>, Error> {
        let result = self
            .state
            .get_move_objects(owner, MoveObjectType::staked_sui())
            .await?;
        self.metrics
            .get_stake_sui_result_size
            .report(result.len() as u64);
        self.metrics
            .get_stake_sui_result_size_total
            .inc_by(result.len() as u64);
        Ok(result)
    }

    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> Result<Vec<DelegatedStake>, Error> {
        let stakes_read = staked_sui_ids
            .iter()
            .map(|id| self.state.get_object_read(id))
            .collect::<Result<Vec<_>, _>>()?;
        if stakes_read.is_empty() {
            return Ok(vec![]);
        }

        let mut stakes: Vec<(StakedSui, bool)> = vec![];

        for stake in stakes_read.into_iter() {
            match stake {
                ObjectRead::Exists(_, o, _) => stakes.push((StakedSui::try_from(&o)?, true)),
                ObjectRead::Deleted(oref) => {
                    match self
                        .state
                        .database
                        .find_object_lt_or_eq_version(oref.0, oref.1.one_before().unwrap())
                    {
                        Some(o) => stakes.push((StakedSui::try_from(&o)?, false)),
                        None => {
                            return Err(Error::UserInputError(UserInputError::ObjectNotFound {
                                object_id: oref.0,
                                version: None,
                            }))
                        }
                    }
                }
                ObjectRead::NotExists(id) => {
                    return Err(Error::UserInputError(UserInputError::ObjectNotFound {
                        object_id: id,
                        version: None,
                    }))
                }
            }
        }

        self.get_delegated_stakes(stakes).await
    }

    async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error> {
        let stakes = self.get_staked_sui(owner).await?;
        if stakes.is_empty() {
            return Ok(vec![]);
        }

        self.get_delegated_stakes(stakes.iter().map(|s| (s.clone(), true)).collect())
            .await
    }

    async fn get_delegated_stakes(
        &self,
        stakes: Vec<(StakedSui, bool)>,
    ) -> Result<Vec<DelegatedStake>, Error> {
        let pools = stakes.into_iter().fold(
            BTreeMap::<_, Vec<_>>::new(),
            |mut pools, (stake, exists)| {
                pools
                    .entry(stake.pool_id())
                    .or_default()
                    .push((stake, exists));
                pools
            },
        );

        let system_state: SuiSystemStateSummary =
            self.get_system_state()?.into_sui_system_state_summary();
        let mut delegated_stakes = vec![];
        for (pool_id, stakes) in pools {
            // Rate table and rate can be null when the pool is not active
            let rate_table = self
                .get_exchange_rate_table(&system_state, &pool_id)
                .await
                .ok();
            let current_rate = if let Some(rate_table) = rate_table {
                self.get_exchange_rate(rate_table, system_state.epoch)
                    .await
                    .ok()
            } else {
                None
            };

            let mut delegations = vec![];
            for (stake, exists) in stakes {
                let status = if !exists {
                    StakeStatus::Unstaked
                } else if system_state.epoch >= stake.activation_epoch() {
                    let estimated_reward = if let (Some(rate_table), Some(current_rate)) =
                        (&rate_table, &current_rate)
                    {
                        let stake_rate = self
                            .get_exchange_rate(*rate_table, stake.activation_epoch())
                            .await
                            .unwrap_or_default();
                        let estimated_reward = ((stake_rate.rate() / current_rate.rate()) - 1.0)
                            * stake.principal() as f64;
                        max(0, estimated_reward.round() as u64)
                    } else {
                        0
                    };
                    StakeStatus::Active { estimated_reward }
                } else {
                    StakeStatus::Pending
                };
                delegations.push(Stake {
                    staked_sui_id: stake.id(),
                    // TODO: this might change when we implement warm up period.
                    stake_request_epoch: stake.activation_epoch() - 1,
                    stake_active_epoch: stake.activation_epoch(),
                    principal: stake.principal(),
                    status,
                })
            }
            let validator =
                get_validator_by_pool_id(self.state.db().as_ref(), &system_state, pool_id)?;
            delegated_stakes.push(DelegatedStake {
                validator_address: validator.sui_address,
                staking_pool: pool_id,
                stakes: delegations,
            })
        }
        Ok(delegated_stakes)
    }

    fn get_system_state(&self) -> Result<SuiSystemState, Error> {
        Ok(self.state.database.get_sui_system_state_object()?)
    }

    async fn get_exchange_rate_table(
        &self,
        system_state: &SuiSystemStateSummary,
        pool_id: &ObjectID,
    ) -> Result<ObjectID, Error> {
        let active_rate = system_state.active_validators.iter().find_map(|v| {
            if &v.staking_pool_id == pool_id {
                Some(v.exchange_rates_id)
            } else {
                None
            }
        });

        if let Some(active_rate) = active_rate {
            Ok(active_rate)
        } else {
            // try find from inactive pool
            let validator = get_validator_from_table(
                self.state.db().as_ref(),
                system_state.inactive_pools_id,
                &ID::new(*pool_id),
            )?;

            Ok(validator.exchange_rates_id)
        }
    }

    async fn get_exchange_rate(
        &self,
        table: ObjectID,
        epoch: EpochId,
    ) -> Result<PoolTokenExchangeRate, Error> {
        let exchange_rate: PoolTokenExchangeRate = get_dynamic_field_from_store(
            self.state.db().as_ref(),
            table,
            &epoch,
        )
        .map_err(|err| {
            SuiError::SuiSystemStateReadError(format!("Failed to get exchange rate: {:?}", err))
        })?;
        Ok(exchange_rate)
    }
}

#[async_trait]
impl GovernanceReadApiServer for GovernanceReadApi {
    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> RpcResult<Vec<DelegatedStake>> {
        Ok(self.get_stakes_by_ids(staked_sui_ids).await?)
    }

    async fn get_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>> {
        Ok(self.get_stakes(owner).await?)
    }

    async fn get_committee_info(&self, epoch: Option<BigInt<u64>>) -> RpcResult<SuiCommittee> {
        Ok(self
            .state
            .committee_store()
            .get_or_latest_committee(epoch.map(|e| *e))
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

    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        Ok(epoch_store.reference_gas_price().into())
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

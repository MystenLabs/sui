// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::cmp::max;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use tracing::{info, instrument};

use cached::proc_macro::cached;
use cached::SizedCache;
use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{DelegatedStake, Stake, StakeStatus};
use sui_json_rpc_types::{SuiCommittee, ValidatorApy, ValidatorApys};
use sui_open_rpc::Module;
use sui_types::base_types::{MoveObjectType, ObjectID, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::dynamic_field::get_dynamic_field_from_store;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::governance::StakedSui;
use sui_types::id::ID;
use sui_types::object::ObjectRead;
use sui_types::sui_serde::BigInt;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::PoolTokenExchangeRate;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::{get_validator_from_table, SuiSystemState};

use crate::api::{GovernanceReadApiServer, JsonRpcMetrics};
use crate::error::Error;
use crate::{with_tracing, ObjectProvider, SuiRpcModule};

#[derive(Clone)]
pub struct GovernanceReadApi {
    state: Arc<AuthorityState>,
    pub metrics: Arc<JsonRpcMetrics>,
}

impl GovernanceReadApi {
    pub fn new(state: Arc<AuthorityState>, metrics: Arc<JsonRpcMetrics>) -> Self {
        Self { state, metrics }
    }

    async fn get_staked_sui(&self, owner: SuiAddress) -> Result<Vec<StakedSui>, Error> {
        let state = self.state.clone();
        let result = spawn_monitored_task!(async move {
            state
                .get_move_objects(owner, MoveObjectType::staked_sui())
                .await
        })
        .await??;

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
        let state = self.state.clone();
        let stakes_read = spawn_monitored_task!(async move {
            staked_sui_ids
                .iter()
                .map(|id| state.get_object_read(id))
                .collect::<Result<Vec<_>, _>>()
        })
        .await??;

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
                        .find_object_lt_or_eq_version(&oref.0, &oref.1.one_before().unwrap())
                        .await?
                    {
                        Some(o) => stakes.push((StakedSui::try_from(&o)?, false)),
                        None => {
                            return Err(Error::UserInputError(UserInputError::ObjectNotFound {
                                object_id: oref.0,
                                version: None,
                            }));
                        }
                    }
                }
                ObjectRead::NotExists(id) => {
                    return Err(Error::UserInputError(UserInputError::ObjectNotFound {
                        object_id: id,
                        version: None,
                    }));
                }
            }
        }

        self.get_delegated_stakes(stakes).await
    }

    async fn get_stakes(&self, owner: SuiAddress) -> Result<Vec<DelegatedStake>, Error> {
        let timer = self.metrics.get_stake_sui_latency.start_timer();
        let stakes = self.get_staked_sui(owner).await?;
        if stakes.is_empty() {
            return Ok(vec![]);
        }
        drop(timer);

        let _timer = self.metrics.get_delegated_sui_latency.start_timer();

        let self_clone = self.clone();
        spawn_monitored_task!(
            self_clone.get_delegated_stakes(stakes.into_iter().map(|s| (s, true)).collect())
        )
        .await?
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

        let system_state = self.get_system_state()?;
        let system_state_summary: SuiSystemStateSummary =
            system_state.clone().into_sui_system_state_summary();

        let rates = exchange_rates(&self.state, system_state_summary.epoch)
            .await?
            .into_iter()
            .map(|rates| (rates.pool_id, rates))
            .collect::<BTreeMap<_, _>>();

        let mut delegated_stakes = vec![];
        for (pool_id, stakes) in pools {
            // Rate table and rate can be null when the pool is not active
            let rate_table = rates
                .get(&pool_id)
                .ok_or_else(|| anyhow!("Cannot find rates for staking pool {pool_id}"))?;
            let current_rate = rate_table.rates.first().map(|(_, rate)| rate);

            let mut delegations = vec![];
            for (stake, exists) in stakes {
                let status = if !exists {
                    StakeStatus::Unstaked
                } else if system_state_summary.epoch >= stake.activation_epoch() {
                    let estimated_reward = if let Some(current_rate) = current_rate {
                        let stake_rate = rate_table
                            .rates
                            .iter()
                            .find_map(|(epoch, rate)| {
                                if *epoch == stake.activation_epoch() {
                                    Some(rate.clone())
                                } else {
                                    None
                                }
                            })
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
            delegated_stakes.push(DelegatedStake {
                validator_address: rate_table.address,
                staking_pool: pool_id,
                stakes: delegations,
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
    #[instrument(skip(self))]
    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> RpcResult<Vec<DelegatedStake>> {
        with_tracing!("get_stakes_by_ids", async move {
            Ok(self.get_stakes_by_ids(staked_sui_ids).await?)
        })
    }

    #[instrument(skip(self))]
    async fn get_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>> {
        with_tracing!(
            "get_stakes",
            async move { Ok(self.get_stakes(owner).await?) }
        )
    }

    #[instrument(skip(self))]
    async fn get_committee_info(&self, epoch: Option<BigInt<u64>>) -> RpcResult<SuiCommittee> {
        with_tracing!("get_committee_info", async move {
            Ok(self
                .state
                .committee_store()
                .get_or_latest_committee(epoch.map(|e| *e))
                .map(|committee| committee.into())
                .map_err(Error::from)?)
        })
    }

    #[instrument(skip(self))]
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        with_tracing!("get_latest_sui_system_state", async move {
            Ok(self
                .state
                .database
                .get_sui_system_state_object()
                .map_err(Error::from)?
                .into_sui_system_state_summary())
        })
    }

    #[instrument(skip(self))]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        with_tracing!("get_reference_gas_price", async move {
            let epoch_store = self.state.load_epoch_store_one_call_per_task();
            Ok(epoch_store.reference_gas_price().into())
        })
    }

    #[instrument(skip(self))]
    async fn get_validators_apy(&self) -> RpcResult<ValidatorApys> {
        info!("get_validator_apy");
        let system_state_summary: SuiSystemStateSummary =
            self.get_latest_sui_system_state().await?;

        let exchange_rate_table = exchange_rates(&self.state, system_state_summary.epoch)
            .await
            .map_err(Error::from)?;

        let mut apys = vec![];

        for rates in exchange_rate_table.into_iter().filter(|r| r.active) {
            let apy = if let Some((_, latest_rate)) = rates.rates.first() {
                let (n, rates_n) = if rates.rates.len() < 29 {
                    (rates.rates.len() as f64, rates.rates.last())
                } else {
                    (29.0, rates.rates.get(29))
                };
                if let Some((_, rate_n_days)) = rates_n {
                    (rate_n_days.rate() / latest_rate.rate()).powf(365.0 / (n + 1.0)) - 1.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            apys.push(ValidatorApy {
                address: rates.address,
                apy,
            });
        }
        Ok(ValidatorApys {
            apys,
            epoch: system_state_summary.epoch,
        })
    }
}

/// Cached exchange rates for validators for the given epoch, the cache size is 1, it will be cleared when the epoch changes.
#[cached(
    type = "SizedCache<EpochId, Vec<ValidatorExchangeRates>>",
    create = "{ SizedCache::with_size(1) }",
    convert = "{ _current_epoch }",
    result = true
)]
async fn exchange_rates(
    state: &Arc<AuthorityState>,
    _current_epoch: EpochId,
) -> SuiResult<Vec<ValidatorExchangeRates>> {
    let system_state = state.database.get_sui_system_state_object()?;
    let system_state_summary: SuiSystemStateSummary = system_state.into_sui_system_state_summary();

    // Get validator rate tables
    let mut tables = vec![];

    for validator in system_state_summary.active_validators {
        tables.push((
            validator.sui_address,
            validator.staking_pool_id,
            validator.exchange_rates_id,
            validator.exchange_rates_size,
            true,
        ));
    }

    // Get inactive validator rate tables
    for df in state.get_dynamic_fields(
        system_state_summary.inactive_pools_id,
        None,
        system_state_summary.inactive_pools_size as usize,
    )? {
        let pool_id: ID =
            bcs::from_bytes(&df.bcs_name).map_err(|e| SuiError::ObjectDeserializationError {
                error: e.to_string(),
            })?;
        let validator = get_validator_from_table(
            state.database.as_ref(),
            system_state_summary.inactive_pools_id,
            &pool_id,
        )?;
        tables.push((
            validator.sui_address,
            validator.staking_pool_id,
            validator.exchange_rates_id,
            validator.exchange_rates_size,
            false,
        ));
    }

    let mut exchange_rates = vec![];
    // Get exchange rates for each validator
    for (address, pool_id, exchange_rates_id, exchange_rates_size, active) in tables {
        let mut rates = state
            .get_dynamic_fields(exchange_rates_id, None, exchange_rates_size as usize)?
            .into_iter()
            .map(|df| {
                let epoch: EpochId = bcs::from_bytes(&df.bcs_name).map_err(|e| {
                    SuiError::ObjectDeserializationError {
                        error: e.to_string(),
                    }
                })?;

                let exchange_rate: PoolTokenExchangeRate =
                    get_dynamic_field_from_store(state.db().as_ref(), exchange_rates_id, &epoch)?;

                Ok::<_, SuiError>((epoch, exchange_rate))
            })
            .collect::<Result<Vec<_>, _>>()?;

        rates.sort_by(|(a, _), (b, _)| a.cmp(b).reverse());

        exchange_rates.push(ValidatorExchangeRates {
            address,
            pool_id,
            active,
            rates,
        });
    }
    Ok(exchange_rates)
}

#[derive(Clone, Debug)]
pub struct ValidatorExchangeRates {
    address: SuiAddress,
    pool_id: ObjectID,
    active: bool,
    rates: Vec<(EpochId, PoolTokenExchangeRate)>,
}

impl SuiRpcModule for GovernanceReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::GovernanceReadApiOpenRpc::module_doc()
    }
}

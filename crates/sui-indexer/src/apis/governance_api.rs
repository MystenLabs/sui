// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{errors::IndexerError, indexer_reader::IndexerReader};
use async_trait::async_trait;
use jsonrpsee::{core::RpcResult, RpcModule};

use cached::{proc_macro::cached, SizedCache};
use sui_json_rpc::{governance_api::ValidatorExchangeRates, SuiRpcModule};
use sui_json_rpc_api::GovernanceReadApiServer;
use sui_json_rpc_types::{
    DelegatedStake, EpochInfo, StakeStatus, SuiCommittee, SuiObjectDataFilter, ValidatorApys,
};
use sui_open_rpc::Module;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SuiAddress},
    committee::EpochId,
    governance::StakedSui,
    sui_serde::BigInt,
    sui_system_state::{sui_system_state_summary::SuiSystemStateSummary, PoolTokenExchangeRate},
};

#[derive(Clone)]
pub struct GovernanceReadApi {
    inner: IndexerReader,
}

impl GovernanceReadApi {
    pub fn new(inner: IndexerReader) -> Self {
        Self { inner }
    }

    pub async fn get_epoch_info(&self, epoch: Option<EpochId>) -> Result<EpochInfo, IndexerError> {
        match self.inner.get_epoch_info(epoch).await {
            Ok(Some(epoch_info)) => Ok(epoch_info),
            Ok(None) => Err(IndexerError::InvalidArgumentError(format!(
                "Missing epoch {epoch:?}"
            ))),
            Err(e) => Err(e),
        }
    }

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, IndexerError> {
        self.inner.get_latest_sui_system_state().await
    }

    async fn get_stakes_by_ids(
        &self,
        ids: Vec<ObjectID>,
    ) -> Result<Vec<DelegatedStake>, IndexerError> {
        let mut stakes = vec![];
        for stored_object in self.inner.multi_get_objects(ids).await? {
            let object = sui_types::object::Object::try_from(stored_object)?;
            let stake_object = StakedSui::try_from(&object)?;
            stakes.push(stake_object);
        }

        self.get_delegated_stakes(stakes).await
    }

    async fn get_staked_by_owner(
        &self,
        owner: SuiAddress,
    ) -> Result<Vec<DelegatedStake>, IndexerError> {
        let mut stakes = vec![];
        for stored_object in self
            .inner
            .get_owned_objects(
                owner,
                Some(SuiObjectDataFilter::StructType(
                    MoveObjectType::staked_sui().into(),
                )),
                None,
                // Allow querying for up to 1000 staked objects
                1000,
            )
            .await?
        {
            let object = sui_types::object::Object::try_from(stored_object)?;
            let stake_object = StakedSui::try_from(&object)?;
            stakes.push(stake_object);
        }

        self.get_delegated_stakes(stakes).await
    }

    pub async fn get_delegated_stakes(
        &self,
        stakes: Vec<StakedSui>,
    ) -> Result<Vec<DelegatedStake>, IndexerError> {
        let pools = stakes
            .into_iter()
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut pools, stake| {
                pools.entry(stake.pool_id()).or_default().push(stake);
                pools
            });

        let system_state_summary = self.get_latest_sui_system_state().await?;
        let epoch = system_state_summary.epoch;

        let rates = exchange_rates(self, &system_state_summary)
            .await?
            .into_iter()
            .map(|rates| (rates.pool_id, rates))
            .collect::<BTreeMap<_, _>>();

        let mut delegated_stakes = vec![];
        for (pool_id, stakes) in pools {
            // Rate table and rate can be null when the pool is not active
            let rate_table = rates.get(&pool_id).ok_or_else(|| {
                IndexerError::InvalidArgumentError(
                    "Cannot find rates for staking pool {pool_id}".to_string(),
                )
            })?;
            let current_rate = rate_table.rates.first().map(|(_, rate)| rate);

            let mut delegations = vec![];
            for stake in stakes {
                let status = if epoch >= stake.activation_epoch() {
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
                        std::cmp::max(0, estimated_reward.round() as u64)
                    } else {
                        0
                    };
                    StakeStatus::Active { estimated_reward }
                } else {
                    StakeStatus::Pending
                };
                delegations.push(sui_json_rpc_types::Stake {
                    staked_sui_id: stake.id(),
                    // TODO: this might change when we implement warm up period.
                    stake_request_epoch: stake.activation_epoch().saturating_sub(1),
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
}

/// Cached exchange rates for validators for the given epoch, the cache size is 1, it will be cleared when the epoch changes.
/// rates are in descending order by epoch.
#[cached(
    type = "SizedCache<EpochId, Vec<ValidatorExchangeRates>>",
    create = "{ SizedCache::with_size(1) }",
    convert = " { system_state_summary.epoch } ",
    result = true
)]
pub async fn exchange_rates(
    state: &GovernanceReadApi,
    system_state_summary: &SuiSystemStateSummary,
) -> Result<Vec<ValidatorExchangeRates>, IndexerError> {
    // Get validator rate tables
    let mut tables = vec![];

    for validator in &system_state_summary.active_validators {
        tables.push((
            validator.sui_address,
            validator.staking_pool_id,
            validator.exchange_rates_id,
            validator.exchange_rates_size,
            true,
        ));
    }

    // Get inactive validator rate tables
    for df in state
        .inner
        .get_dynamic_fields(
            system_state_summary.inactive_pools_id,
            None,
            system_state_summary.inactive_pools_size as usize,
        )
        .await?
    {
        let pool_id: sui_types::id::ID = bcs::from_bytes(&df.bcs_name).map_err(|e| {
            sui_types::error::SuiError::ObjectDeserializationError {
                error: e.to_string(),
            }
        })?;
        let inactive_pools_id = system_state_summary.inactive_pools_id;
        let validator = state
            .inner
            .get_validator_from_table(inactive_pools_id, pool_id)
            .await?;
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
        let mut rates = vec![];
        for df in state
            .inner
            .get_dynamic_fields_raw(exchange_rates_id, None, exchange_rates_size as usize)
            .await?
        {
            let dynamic_field = df
                .to_dynamic_field::<EpochId, PoolTokenExchangeRate>()
                .ok_or_else(|| sui_types::error::SuiError::ObjectDeserializationError {
                    error: "dynamic field malformed".to_owned(),
                })?;

            rates.push((dynamic_field.name, dynamic_field.value));
        }

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

#[async_trait]
impl GovernanceReadApiServer for GovernanceReadApi {
    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> RpcResult<Vec<DelegatedStake>> {
        self.get_stakes_by_ids(staked_sui_ids)
            .await
            .map_err(Into::into)
    }

    async fn get_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>> {
        self.get_staked_by_owner(owner).await.map_err(Into::into)
    }

    async fn get_committee_info(&self, epoch: Option<BigInt<u64>>) -> RpcResult<SuiCommittee> {
        let epoch = self.get_epoch_info(epoch.as_deref().copied()).await?;
        Ok(epoch.committee().map_err(IndexerError::from)?.into())
    }

    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        self.get_latest_sui_system_state().await.map_err(Into::into)
    }

    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        let epoch = self.get_epoch_info(None).await?;
        Ok(BigInt::from(epoch.reference_gas_price.ok_or_else(
            || {
                IndexerError::PersistentStorageDataCorruptionError(
                    "missing latest reference gas price".to_owned(),
                )
            },
        )?))
    }

    async fn get_validators_apy(&self) -> RpcResult<ValidatorApys> {
        Ok(self.get_validators_apy().await?)
    }
}

impl SuiRpcModule for GovernanceReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc_api::GovernanceReadApiOpenRpc::module_doc()
    }
}

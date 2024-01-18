// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::db_backend::GenericQueryBuilder;
use crate::{
    config::{Limits, DEFAULT_SERVER_DB_POOL_SIZE},
    error::Error,
    types::{
        address::Address, coin_metadata::CoinMetadata, move_object::MoveObject, object::Object,
        sui_address::SuiAddress, validator::Validator,
    },
};
use diesel::{OptionalExtension, RunQueryDsl};
use move_core_types::language_storage::StructTag;
use std::collections::BTreeMap;
use sui_indexer::{
    apis::GovernanceReadApiV2,
    indexer_reader::IndexerReader,
    models_v2::{display::StoredDisplay, objects::StoredObject},
    PgConnectionPoolConfig,
};
use sui_json_rpc::coin_api::parse_to_struct_tag;
use sui_json_rpc_types::Stake as RpcStakedSui;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    coin::{CoinMetadata as NativeCoinMetadata, TreasuryCap},
    gas_coin::{GAS, TOTAL_SUPPLY_SUI},
    governance::StakedSui as NativeStakedSui,
    object::Object as NativeObject,
    sui_system_state::sui_system_state_summary::{
        SuiSystemStateSummary as NativeSuiSystemStateSummary, SuiValidatorSummary,
    },
};

#[cfg(feature = "pg_backend")]
use super::pg_backend::{PgQueryExecutor, QueryBuilder};

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum DbValidationError {
    #[error("Invalid checkpoint combination. 'before' or 'after' checkpoint cannot be used with 'at' checkpoint")]
    InvalidCheckpointCombination,
    #[error("Before checkpoint must be greater than after checkpoint")]
    InvalidCheckpointOrder,
    #[error("Filtering objects by package::module::type is not currently supported")]
    UnsupportedPMT,
    #[error("Filtering objects by object keys is not currently supported")]
    UnsupportedObjectKeys,
    #[error("Requires package and module")]
    RequiresPackageAndModule,
    #[error("Requires package")]
    RequiresPackage,
    #[error("'first' can only be used with 'after")]
    FirstAfter,
    #[error("'last' can only be used with 'before'")]
    LastBefore,
    #[error("Pagination is currently disabled on balances")]
    PaginationDisabledOnBalances,
    #[error("Invalid owner type. Must be Address or Object")]
    InvalidOwnerType,
    #[error("Query cost exceeded - cost: {0}, limit: {1}")]
    QueryCostExceeded(u64, u64),
    #[error("Page size exceeded - requested: {0}, limit: {1}")]
    PageSizeExceeded(u64, u64),
    #[error("Invalid type provided as filter: {0}")]
    InvalidType(String),
}

pub(crate) struct PgManager {
    pub inner: IndexerReader,
    pub limits: Limits,
}

impl PgManager {
    pub(crate) fn new(inner: IndexerReader, limits: Limits) -> Self {
        Self { inner, limits }
    }

    /// Create a new underlying reader, which is used by this type as well as other data providers.
    pub(crate) fn reader(db_url: impl Into<String>) -> Result<IndexerReader, Error> {
        Self::reader_with_config(db_url, DEFAULT_SERVER_DB_POOL_SIZE)
    }

    pub(crate) fn reader_with_config(
        db_url: impl Into<String>,
        pool_size: u32,
    ) -> Result<IndexerReader, Error> {
        let mut config = PgConnectionPoolConfig::default();
        config.set_pool_size(pool_size);
        IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(format!("Failed to create reader: {e}")))
    }
}

/// Implement methods to query db and return StoredData
impl PgManager {
    async fn get_obj_by_type(&self, object_type: String) -> Result<Option<StoredObject>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_obj_by_type(object_type.clone())),
            |query| move |conn| query.get_result::<StoredObject>(conn).optional(),
        )
        .await
    }

    async fn get_display_by_obj_type(
        &self,
        object_type: String,
    ) -> Result<Option<StoredDisplay>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_display_by_obj_type(object_type.clone())),
            |query| move |conn| query.get_result::<StoredDisplay>(conn).optional(),
        )
        .await
    }
}

/// Implement methods to be used by graphql resolvers
impl PgManager {
    /// Retrieve the validator APYs
    pub(crate) async fn fetch_validator_apys(
        &self,
        address: &NativeSuiAddress,
    ) -> Result<Option<f64>, Error> {
        let governance_api = GovernanceReadApiV2::new(self.inner.clone());

        governance_api
            .get_validator_apy(address)
            .await
            .map_err(|e| Error::Internal(format!("{e}")))
    }

    pub(crate) async fn fetch_display_object_by_type(
        &self,
        object_type: &StructTag,
    ) -> Result<Option<StoredDisplay>, Error> {
        let object_type = object_type.to_canonical_string(/* with_prefix */ true);
        self.get_display_by_obj_type(object_type).await
    }

    pub(crate) async fn available_range(&self) -> Result<(u64, u64), Error> {
        Ok(self
            .inner
            .spawn_blocking(|this| this.get_consistent_read_range())
            .await
            .map(|(start, end)| (start as u64, end as u64))?)
    }

    /// If no epoch was requested or if the epoch requested is in progress,
    /// returns the latest sui system state.
    pub(crate) async fn fetch_sui_system_state(
        &self,
        epoch_id: Option<u64>,
    ) -> Result<NativeSuiSystemStateSummary, Error> {
        let latest_sui_system_state = self
            .inner
            .spawn_blocking(move |this| this.get_latest_sui_system_state())
            .await?;

        if epoch_id.is_some_and(|id| id == latest_sui_system_state.epoch) {
            Ok(latest_sui_system_state)
        } else {
            Ok(self
                .inner
                .spawn_blocking(move |this| this.get_epoch_sui_system_state(epoch_id))
                .await?)
        }
    }

    /// Make a request to the RPC for its representations of the staked sui we parsed out of the
    /// object.  Used to implement fields that are implemented in JSON-RPC but not GraphQL (yet).
    pub(crate) async fn fetch_rpc_staked_sui(
        &self,
        stake: NativeStakedSui,
    ) -> Result<RpcStakedSui, Error> {
        let governance_api = GovernanceReadApiV2::new(self.inner.clone());

        let mut delegated_stakes = governance_api
            .get_delegated_stakes(vec![stake])
            .await
            .map_err(|e| Error::Internal(format!("Error fetching delegated stake. {e}")))?;

        let Some(mut delegated_stake) = delegated_stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No pools returned.".to_string(),
            ));
        };

        let Some(stake) = delegated_stake.stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No stake in pool.".to_string(),
            ));
        };

        Ok(stake)
    }

    pub(crate) async fn fetch_coin_metadata(
        &self,
        coin_type: String,
    ) -> Result<Option<CoinMetadata>, Error> {
        let coin_struct =
            parse_to_struct_tag(&coin_type).map_err(|e| Error::InvalidCoinType(e.to_string()))?;

        let coin_metadata_type =
            NativeCoinMetadata::type_(coin_struct).to_canonical_string(/* with_prefix */ true);

        let Some(coin_metadata) = self.get_obj_by_type(coin_metadata_type).await? else {
            return Ok(None);
        };

        let object = Object::try_from(coin_metadata)?;
        let move_object = MoveObject::try_from(&object).map_err(|_| {
            Error::Internal(format!(
                "Expected {} to be coin metadata, but it is not an object.",
                object.address,
            ))
        })?;

        let coin_metadata_object = CoinMetadata::try_from(&move_object).map_err(|_| {
            Error::Internal(format!(
                "Expected {} to be coin metadata, but it is not.",
                object.address,
            ))
        })?;

        Ok(Some(coin_metadata_object))
    }

    pub(crate) async fn fetch_total_supply(&self, coin_type: String) -> Result<Option<u64>, Error> {
        let coin_struct =
            parse_to_struct_tag(&coin_type).map_err(|e| Error::InvalidCoinType(e.to_string()))?;

        let supply = if GAS::is_gas(&coin_struct) {
            TOTAL_SUPPLY_SUI
        } else {
            let treasury_cap_type =
                TreasuryCap::type_(coin_struct).to_canonical_string(/* with_prefix */ true);

            let Some(treasury_cap) = self.get_obj_by_type(treasury_cap_type).await? else {
                return Ok(None);
            };

            let native_object = NativeObject::try_from(treasury_cap)?;
            let object_id = native_object.id();
            let treasury_cap_object = TreasuryCap::try_from(native_object).map_err(|e| {
                Error::Internal(format!(
                    "Error while deserializing treasury cap object {object_id}: {e}"
                ))
            })?;
            treasury_cap_object.total_supply.value
        };

        Ok(Some(supply))
    }
}

pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
    system_state: Option<NativeSuiSystemStateSummary>,
) -> Vec<Validator> {
    let (at_risk, reports) = if let Some(NativeSuiSystemStateSummary {
        at_risk_validators,
        validator_report_records,
        ..
    }) = system_state
    {
        (
            BTreeMap::from_iter(at_risk_validators),
            BTreeMap::from_iter(validator_report_records),
        )
    } else {
        Default::default()
    };

    validators
        .into_iter()
        .map(|validator_summary| {
            let at_risk = at_risk.get(&validator_summary.sui_address).copied();
            let report_records = reports.get(&validator_summary.sui_address).map(|addrs| {
                addrs
                    .iter()
                    .cloned()
                    .map(|a| Address {
                        address: SuiAddress::from(a),
                    })
                    .collect()
            });

            Validator {
                validator_summary,
                at_risk,
                report_records,
            }
        })
        .collect()
}

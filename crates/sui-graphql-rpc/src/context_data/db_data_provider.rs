// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::db_backend::GenericQueryBuilder;
use crate::{
    config::{Limits, DEFAULT_SERVER_DB_POOL_SIZE},
    error::Error,
    types::{
        address::Address,
        balance::Balance,
        big_int::BigInt,
        coin::Coin,
        coin_metadata::CoinMetadata,
        dynamic_field::{DynamicField, DynamicFieldName},
        move_object::MoveObject,
        move_type::MoveType,
        object::{Object, ObjectFilter},
        stake::StakedSui,
        sui_address::SuiAddress,
        suins_registration::SuinsRegistration,
        validator::Validator,
    },
};
use async_graphql::connection::{Connection, Edge};
use diesel::{OptionalExtension, RunQueryDsl};
use move_core_types::language_storage::StructTag;
use std::{collections::BTreeMap, str::FromStr};
use sui_indexer::{
    apis::GovernanceReadApiV2,
    indexer_reader::IndexerReader,
    models_v2::{display::StoredDisplay, objects::StoredObject},
    types_v2::OwnerType,
    PgConnectionPoolConfig,
};
use sui_json_rpc::{
    coin_api::{parse_to_struct_tag, parse_to_type_tag},
    name_service::{Domain, NameRecord, NameServiceConfig},
};
use sui_json_rpc_types::Stake as RpcStakedSui;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SuiAddress as NativeSuiAddress},
    coin::{CoinMetadata as NativeCoinMetadata, TreasuryCap},
    dynamic_field::{DynamicFieldType, Field},
    gas_coin::{GAS, TOTAL_SUPPLY_SUI},
    governance::StakedSui as NativeStakedSui,
    object::Object as NativeObject,
    sui_system_state::sui_system_state_summary::{
        SuiSystemStateSummary as NativeSuiSystemStateSummary, SuiValidatorSummary,
    },
    TypeTag,
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

#[derive(thiserror::Error, Debug)]
pub enum TypeFilterError {
    #[error("Invalid format in '{0}' - if '::' is present, there must be a non-empty string on both sides. Expected format like '{1}'")]
    MissingComponents(String, &'static str),
    #[error("Invalid format in '{0}' - value must have {1} or fewer components. Expected format like '{2}'")]
    TooManyComponents(String, u64, &'static str),
}

// Db needs information on whether the first or last n are being selected
#[derive(Clone, Copy)]
pub(crate) enum PageLimit {
    First(i64),
    Last(i64),
}

pub(crate) struct PgManager {
    pub inner: IndexerReader,
    pub limits: Limits,
}

impl PageLimit {
    pub(crate) fn value(&self) -> i64 {
        match self {
            PageLimit::First(limit) => *limit,
            PageLimit::Last(limit) => *limit,
        }
    }
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
    async fn get_obj(
        &self,
        address: Vec<u8>,
        version: Option<i64>,
    ) -> Result<Option<StoredObject>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_obj(address.clone(), version)),
            |query| move |conn| query.get_result::<StoredObject>(conn).optional(),
        )
        .await
    }

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

    /// Fetches the coins owned by the address and filters them by the given coin type.
    /// If no address is given, it fetches all available coin objects matching the coin type.
    async fn multi_get_coins(
        &self,
        address: Option<Vec<u8>>,
        coin_type: String,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<(Vec<StoredObject>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let before = before
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;
        let after = after
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;
        let coin_type = parse_to_type_tag(Some(coin_type))
            .map_err(|e| Error::InvalidCoinType(e.to_string()))?
            .to_canonical_string(/* with_prefix */ true);
        let result: Option<Vec<StoredObject>> = self
            .run_query_async_with_cost(
                move || {
                    Ok(QueryBuilder::multi_get_coins(
                        before.clone(),
                        after.clone(),
                        limit,
                        address.clone(),
                        coin_type.clone(),
                    ))
                },
                |query| move |conn| query.load(conn).optional(),
            )
            .await?;

        result
            .map(|mut stored_objs| {
                let has_next_page = stored_objs.len() as i64 > limit.value();
                if has_next_page {
                    stored_objs.pop();
                }

                if last.is_some() {
                    stored_objs.reverse();
                }

                Ok((stored_objs, has_next_page))
            })
            .transpose()
    }

    async fn get_balance(
        &self,
        address: Vec<u8>,
        coin_type: String,
    ) -> Result<Option<(Option<i64>, Option<i64>, Option<String>)>, Error> {
        self.run_query_async_with_cost(
            move || {
                Ok(QueryBuilder::get_balance(
                    address.clone(),
                    coin_type.clone(),
                ))
            },
            |query| move |conn| query.get_result(conn).optional(),
        )
        .await
    }

    async fn multi_get_balances(
        &self,
        address: Vec<u8>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Vec<(Option<i64>, Option<i64>, Option<String>)>>, Error> {
        // Todo (wlmyng): paginating on balances does not really make sense
        // We'll always need to calculate all balances first
        if first.is_some() || after.is_some() || last.is_some() || before.is_some() {
            return Err(DbValidationError::PaginationDisabledOnBalances.into());
        }

        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::multi_get_balances(address.clone())),
            |query| move |conn| query.load(conn).optional(),
        )
        .await
    }

    async fn multi_get_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<Option<(Vec<StoredObject>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let before = before
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;
        let after = after
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;

        let query = move || {
            QueryBuilder::multi_get_objs(
                before.clone(),
                after.clone(),
                limit,
                filter.clone(),
                owner_type,
            )
        };

        let result: Option<Vec<StoredObject>> = self
            .run_query_async_with_cost(query, |query| move |conn| query.load(conn).optional())
            .await?;

        result
            .map(|mut stored_objs| {
                let has_next_page = stored_objs.len() as i64 > limit.value();
                if has_next_page {
                    stored_objs.pop();
                }

                if last.is_some() {
                    stored_objs.reverse();
                }

                Ok((stored_objs, has_next_page))
            })
            .transpose()
    }
}

/// Implement methods to be used by graphql resolvers
impl PgManager {
    pub(crate) fn parse_obj_cursor(&self, cursor: &str) -> Result<Vec<u8>, Error> {
        Ok(SuiAddress::from_str(cursor)
            .map_err(|e| Error::InvalidCursor(e.to_string()))?
            .into_vec())
    }

    pub(crate) fn validate_obj_filter(&self, filter: &ObjectFilter) -> Result<(), Error> {
        if filter.object_keys.is_some() {
            return Err(DbValidationError::UnsupportedObjectKeys.into());
        }

        Ok(())
    }

    pub(crate) fn validate_page_limit(
        &self,
        first: Option<u64>,
        last: Option<u64>,
    ) -> Result<PageLimit, Error> {
        if let Some(f) = first {
            if f > self.limits.max_page_size {
                return Err(
                    DbValidationError::PageSizeExceeded(f, self.limits.max_page_size).into(),
                );
            }
            Ok(PageLimit::First(f as i64))
        } else if let Some(l) = last {
            if l > self.limits.max_page_size {
                return Err(
                    DbValidationError::PageSizeExceeded(l, self.limits.max_page_size).into(),
                );
            }
            return Ok(PageLimit::Last(l as i64));
        } else {
            Ok(PageLimit::First(self.limits.default_page_size as i64))
        }
    }

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

    pub(crate) async fn fetch_owned_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
        owner: SuiAddress,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        let filter = filter
            .map(|mut f| {
                f.owner = Some(owner);
                f
            })
            .unwrap_or_else(|| ObjectFilter {
                owner: Some(owner),
                ..Default::default()
            });
        self.fetch_objs(first, after, last, before, Some(filter))
            .await
    }

    pub(crate) async fn fetch_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        validate_cursor_pagination(&first, &after, &last, &before)?;
        if let Some(filter) = &filter {
            self.validate_obj_filter(filter)?;
        }
        let objects = self
            .multi_get_objs(first, after, last, before, filter, None)
            .await?;

        if let Some((stored_objs, has_next_page)) = objects {
            let mut connection = Connection::new(false, has_next_page);
            connection
                .edges
                .extend(stored_objs.into_iter().filter_map(|stored_obj| {
                    Object::try_from(stored_obj)
                        .map_err(|e| eprintln!("Error converting object: {:?}", e))
                        .ok()
                        .map(|obj| Edge::new(obj.address.to_string(), obj))
                }));
            Ok(Some(connection))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn fetch_balance(
        &self,
        address: SuiAddress,
        coin_type: Option<String>,
    ) -> Result<Option<Balance>, Error> {
        let address = address.into_vec();
        let coin_type = parse_to_type_tag(coin_type.clone())
            .map_err(|e| Error::InvalidCoinType(e.to_string()))?;
        let result = self
            .get_balance(
                address,
                coin_type.to_canonical_string(/* with_prefix */ true),
            )
            .await?;

        match result {
            None | Some((None, None, None)) => Ok(None),

            Some((Some(balance), Some(count), Some(_coin_type))) => Ok(Some(Balance {
                coin_object_count: Some(count as u64),
                total_balance: Some(BigInt::from(balance)),
                coin_type: Some(MoveType::new(coin_type)),
            })),

            _ => Err(Error::Internal(
                "Expected fields are missing on balance calculation".to_string(),
            )),
        }
    }

    pub(crate) async fn fetch_balances(
        &self,
        address: SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Balance>>, Error> {
        let address = address.into_vec();
        let Some(balances) = self
            .multi_get_balances(address, first, after, last, before)
            .await?
        else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, false);
        for (balance, count, coin_type) in balances {
            let (Some(balance), Some(count), Some(coin_type)) = (balance, count, coin_type) else {
                return Err(Error::Internal(
                    "Expected fields are missing on balance calculation".to_string(),
                ));
            };

            let coin_tag = TypeTag::from_str(&coin_type)
                .map_err(|e| Error::Internal(format!("Error parsing type '{coin_type}': {e}")))?;

            connection.edges.push(Edge::new(
                coin_type.clone(),
                Balance {
                    coin_object_count: Some(count as u64),
                    total_balance: Some(BigInt::from(balance)),
                    coin_type: Some(MoveType::new(coin_tag)),
                },
            ));
        }

        Ok(Some(connection))
    }

    /// Fetches all coins owned by the given address that match the given coin type.
    /// If no address is given, then it will fetch all coin objects of the given type.
    /// If no coin type is provided, it will use the default gas coin (SUI).
    pub(crate) async fn fetch_coins(
        &self,
        address: Option<SuiAddress>,
        coin_type: Option<String>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Coin>>, Error> {
        let address = address.map(|addr| addr.into_vec());
        let coin_type = coin_type.unwrap_or_else(|| {
            GAS::type_().to_canonical_string(/* with_prefix */ true)
        });

        let coins = self
            .multi_get_coins(address, coin_type, first, after, last, before)
            .await?;

        let Some((stored_objs, has_next_page)) = coins else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);
        for stored_obj in stored_objs {
            let object = Object::try_from(stored_obj)?;

            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a coin, but it's not an object",
                    object.address,
                ))
            })?;

            let coin_object = Coin::try_from(&move_object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a coin, but it is not",
                    object.address,
                ))
            })?;

            let cursor = move_object
                .native
                .id()
                .to_canonical_string(/* with_prefix */ true);

            connection.edges.push(Edge::new(cursor, coin_object));
        }

        Ok(Some(connection))
    }

    pub(crate) async fn resolve_name_service_address(
        &self,
        name_service_config: &NameServiceConfig,
        name: String,
    ) -> Result<Option<Address>, Error> {
        let domain = name.parse::<Domain>()?;

        let record_id = name_service_config.record_field_id(&domain);

        let field_record_object = match self.inner.get_object_in_blocking_task(record_id).await? {
            Some(o) => o,
            None => return Ok(None),
        };

        let record = field_record_object
            .to_rust::<Field<Domain, NameRecord>>()
            .ok_or_else(|| Error::Internal(format!("Malformed Object {record_id}")))?
            .value;

        Ok(record.target_address.map(|address| Address {
            address: SuiAddress::from_array(address.to_inner()),
        }))
    }

    pub(crate) async fn available_range(&self) -> Result<(u64, u64), Error> {
        Ok(self
            .inner
            .spawn_blocking(|this| this.get_consistent_read_range())
            .await
            .map(|(start, end)| (start as u64, end as u64))?)
    }

    pub(crate) async fn default_name_service_name(
        &self,
        name_service_config: &NameServiceConfig,
        address: SuiAddress,
    ) -> Result<Option<String>, Error> {
        let reverse_record_id = name_service_config.reverse_record_field_id(address.as_slice());

        let field_reverse_record_object = match self
            .inner
            .get_object_in_blocking_task(reverse_record_id)
            .await?
        {
            Some(o) => o,
            None => return Ok(None),
        };

        let domain = field_reverse_record_object
            .to_rust::<Field<SuiAddress, Domain>>()
            .ok_or_else(|| Error::Internal(format!("Malformed Object {reverse_record_id}")))?
            .value;

        Ok(Some(domain.to_string()))
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

    pub(crate) async fn fetch_staked_sui(
        &self,
        address: SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, StakedSui>>, Error> {
        let obj_filter = ObjectFilter {
            type_: Some(MoveObjectType::staked_sui().to_canonical_string(/* with_prefix */ true)),
            owner: Some(address),
            object_ids: None,
            object_keys: None,
        };

        let objs = self
            .multi_get_objs(
                first,
                after,
                last,
                before,
                Some(obj_filter),
                Some(OwnerType::Address),
            )
            .await?;

        let Some((stored_objs, has_next_page)) = objs else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);
        for stored_obj in stored_objs {
            let object = Object::try_from(stored_obj)?;

            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a staked sui, but it is not an object.",
                    object.address,
                ))
            })?;

            let stake_object = StakedSui::try_from(&move_object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a staked sui, but it is not.",
                    object.address,
                ))
            })?;

            let cursor = move_object
                .native
                .id()
                .to_canonical_string(/* with_prefix */ true);

            connection.edges.push(Edge::new(cursor, stake_object));
        }

        Ok(Some(connection))
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

    pub(crate) async fn fetch_dynamic_fields(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        address: SuiAddress,
    ) -> Result<Option<Connection<String, DynamicField>>, Error> {
        let filter = ObjectFilter {
            owner: Some(address),
            ..Default::default()
        };

        let objs = self
            .multi_get_objs(
                first,
                after,
                last,
                before,
                Some(filter),
                Some(OwnerType::Object),
            )
            .await?;

        let Some((stored_objs, has_next_page)) = objs else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);

        for stored_obj in stored_objs {
            let df_object_id = stored_obj.df_object_id.as_ref().ok_or_else(|| {
                Error::Internal("Dynamic field does not have df_object_id".to_string())
            })?;
            let cursor = SuiAddress::from_bytes(df_object_id)
                .map_err(|e| Error::Internal(format!("{e}")))?;
            let df_kind = match stored_obj.df_kind {
                None => Err(Error::Internal("Dynamic field type is not set".to_string())),
                Some(df_kind) => match df_kind {
                    0 => Ok(DynamicFieldType::DynamicField),
                    1 => Ok(DynamicFieldType::DynamicObject),
                    _ => Err(Error::Internal("Unexpected df_kind value".to_string())),
                },
            }?;

            connection.edges.push(Edge::new(
                cursor.to_string(),
                DynamicField {
                    stored_object: stored_obj,
                    df_object_id: cursor,
                    df_kind,
                },
            ));
        }
        Ok(Some(connection))
    }

    pub(crate) async fn fetch_dynamic_field(
        &self,
        address: SuiAddress,
        name: DynamicFieldName,
        kind: DynamicFieldType,
    ) -> Result<Option<DynamicField>, Error> {
        let name_bcs_value = &name.bcs.0;
        let parent_object_id =
            ObjectID::from_bytes(address.as_slice()).map_err(|e| Error::Client(e.to_string()))?;
        let mut type_tag =
            TypeTag::from_str(&name.type_).map_err(|e| Error::Client(e.to_string()))?;

        if kind == DynamicFieldType::DynamicObject {
            let dynamic_object_field_struct =
                sui_types::dynamic_field::DynamicFieldInfo::dynamic_object_field_wrapper(type_tag);
            type_tag = TypeTag::Struct(Box::new(dynamic_object_field_struct));
        }

        let id = sui_types::dynamic_field::derive_dynamic_field_id(
            parent_object_id,
            &type_tag,
            name_bcs_value,
        )
        .map_err(|e| Error::Internal(format!("Deriving dynamic field id cannot fail: {e}")))?;

        let stored_obj = self.get_obj(id.to_vec(), None).await?;
        if let Some(stored_object) = stored_obj {
            let df_object_id = stored_object.df_object_id.as_ref().ok_or_else(|| {
                Error::Internal("Dynamic field does not have df_object_id".to_string())
            })?;
            let df_object_id =
                SuiAddress::from_bytes(df_object_id).map_err(|e| Error::Internal(e.to_string()))?;
            return Ok(Some(DynamicField {
                stored_object,
                df_object_id,
                df_kind: kind,
            }));
        }
        Ok(None)
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

    pub(crate) async fn fetch_suins_registrations(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        name_service_config: &NameServiceConfig,
        owner: SuiAddress,
    ) -> Result<Option<Connection<String, SuinsRegistration>>, Error> {
        let suins_registration_type = format!(
            "{}::suins_registration::SuinsRegistration",
            name_service_config.package_address
        );
        let struct_tag = parse_to_struct_tag(&suins_registration_type)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let obj_filter = ObjectFilter {
            type_: Some(suins_registration_type),
            owner: Some(owner),
            object_ids: None,
            object_keys: None,
        };

        let objs = self
            .multi_get_objs(
                first,
                after,
                last,
                before,
                Some(obj_filter),
                Some(OwnerType::Address),
            )
            .await?;

        let Some((stored_objs, has_next_page)) = objs else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);
        for stored_obj in stored_objs {
            let object = Object::try_from(stored_obj)?;

            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a suinsRegistration object, but it's not an object",
                    object.address,
                ))
            })?;

            let suins_registration = SuinsRegistration::try_from(&move_object, &struct_tag)
                .map_err(|_| {
                    Error::Internal(format!(
                        "Expected {} to be a suinsRegistration object, but it is not",
                        object.address,
                    ))
                })?;

            let cursor = move_object
                .native
                .id()
                .to_canonical_string(/* with_prefix */ true);

            connection.edges.push(Edge::new(cursor, suins_registration));
        }

        Ok(Some(connection))
    }
}

/// TODO: enfroce limits on first and last
pub(crate) fn validate_cursor_pagination(
    first: &Option<u64>,
    after: &Option<String>,
    last: &Option<u64>,
    before: &Option<String>,
) -> Result<(), Error> {
    if first.is_some() && before.is_some() {
        return Err(DbValidationError::FirstAfter.into());
    }

    if last.is_some() && after.is_some() {
        return Err(DbValidationError::LastBefore.into());
    }

    if before.is_some() && after.is_some() {
        return Err(Error::CursorNoBeforeAfter);
    }

    if first.is_some() && last.is_some() {
        return Err(Error::CursorNoFirstLast);
    }

    Ok(())
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

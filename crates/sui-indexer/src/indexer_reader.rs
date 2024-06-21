// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Result};
use cached::{Cached, SizedCache};
use diesel::r2d2::R2D2Connection;
use diesel::{
    dsl::sql, r2d2::ConnectionManager, sql_types::Bool, ExpressionMethods, OptionalExtension,
    QueryDsl, RunQueryDsl, TextExpressionMethods,
};
use itertools::{any, Itertools};
use tap::TapFallible;

use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use move_core_types::annotated_value::MoveStructLayout;
use move_core_types::language_storage::StructTag;
use sui_json_rpc_types::DisplayFieldsResponse;
use sui_json_rpc_types::{
    Balance, Coin as SuiCoin, SuiCoinMetadata, SuiTransactionBlockEffects,
    SuiTransactionBlockEffectsAPI,
};
use sui_json_rpc_types::{
    CheckpointId, EpochInfo, EventFilter, SuiEvent, SuiObjectDataFilter,
    SuiTransactionBlockResponse, TransactionFilter,
};
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::{PackageStoreWithLruCache, Resolver};
use sui_types::effects::TransactionEvents;
use sui_types::{balance::Supply, coin::TreasuryCap, dynamic_field::DynamicFieldName};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, VersionNumber},
    committee::EpochId,
    digests::{ObjectDigest, TransactionDigest},
    dynamic_field::DynamicFieldInfo,
    is_system_package,
    object::{Object, ObjectRead},
    sui_system_state::{sui_system_state_summary::SuiSystemStateSummary, SuiSystemStateTrait},
};
use sui_types::{coin::CoinMetadata, event::EventID};

use crate::db::{ConnectionConfig, ConnectionPool, ConnectionPoolConfig};
use crate::models::transactions::{stored_events_to_events, StoredTransactionEvents};
use crate::store::diesel_macro::*;
use crate::{
    errors::IndexerError,
    models::{
        checkpoints::StoredCheckpoint,
        display::StoredDisplay,
        epoch::StoredEpochInfo,
        events::StoredEvent,
        objects::{CoinBalance, ObjectRefColumn, StoredObject},
        transactions::{tx_events_to_sui_tx_events, StoredTransaction},
        tx_indices::TxSequenceNumber,
    },
    schema::{checkpoints, display, epochs, events, objects, objects_snapshot, transactions},
    store::package_resolver::IndexerStorePackageResolver,
    types::{IndexerResult, OwnerType},
};

pub const TX_SEQUENCE_NUMBER_STR: &str = "tx_sequence_number";
pub const TRANSACTION_DIGEST_STR: &str = "transaction_digest";
pub const EVENT_SEQUENCE_NUMBER_STR: &str = "event_sequence_number";

pub struct IndexerReader<T>
where
    T: R2D2Connection + 'static,
{
    pool: ConnectionPool<T>,
    package_resolver: PackageResolver<T>,
    package_obj_type_cache: Arc<Mutex<SizedCache<String, Option<ObjectID>>>>,
}

impl<T> Clone for IndexerReader<T>
where
    T: R2D2Connection,
{
    fn clone(&self) -> IndexerReader<T> {
        IndexerReader {
            pool: self.pool.clone(),
            package_resolver: self.package_resolver.clone(),
            package_obj_type_cache: self.package_obj_type_cache.clone(),
        }
    }
}

pub type PackageResolver<T> =
    Arc<Resolver<PackageStoreWithLruCache<IndexerStorePackageResolver<T>>>>;

// Impl for common initialization and utilities
impl<U: R2D2Connection + 'static> IndexerReader<U> {
    pub fn new<T: Into<String>>(db_url: T) -> Result<Self> {
        let config = ConnectionPoolConfig::default();
        Self::new_with_config(db_url, config)
    }

    pub fn new_with_config<T: Into<String>>(
        db_url: T,
        config: ConnectionPoolConfig,
    ) -> Result<Self> {
        let manager = ConnectionManager::<U>::new(db_url);

        let connection_config = ConnectionConfig {
            statement_timeout: config.statement_timeout,
            read_only: true,
        };

        let pool = diesel::r2d2::Pool::builder()
            .max_size(config.pool_size)
            .connection_timeout(config.connection_timeout)
            .connection_customizer(Box::new(connection_config))
            .build(manager)
            .map_err(|e| anyhow!("Failed to initialize connection pool. Error: {:?}. If Error is None, please check whether the configured pool size (currently {}) exceeds the maximum number of connections allowed by the database.", e, config.pool_size))?;

        let indexer_store_pkg_resolver = IndexerStorePackageResolver::new(pool.clone());
        let package_cache = PackageStoreWithLruCache::new(indexer_store_pkg_resolver);
        let package_resolver = Arc::new(Resolver::new(package_cache));
        let package_obj_type_cache = Arc::new(Mutex::new(SizedCache::with_size(10000)));
        Ok(Self {
            pool,
            package_resolver,
            package_obj_type_cache,
        })
    }

    pub async fn spawn_blocking<F, R, E>(&self, f: F) -> Result<R, E>
    where
        F: FnOnce(Self) -> Result<R, E> + Send + 'static,
        R: Send + 'static,
        E: Send + 'static,
    {
        let this = self.clone();
        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            CALLED_FROM_BLOCKING_POOL
                .with(|in_blocking_pool| *in_blocking_pool.borrow_mut() = true);
            let _guard = current_span.enter();
            f(this)
        })
        .await
        .expect("propagate any panics")
    }

    pub fn get_pool(&self) -> ConnectionPool<U> {
        self.pool.clone()
    }
}

// Impl for reading data from the DB
impl<U: R2D2Connection> IndexerReader<U> {
    fn get_object_from_db(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<StoredObject>, IndexerError> {
        let object_id = object_id.to_vec();

        let stored_object = run_query!(&self.pool, |conn| {
            if let Some(version) = version {
                objects::dsl::objects
                    .filter(objects::dsl::object_id.eq(object_id))
                    .filter(objects::dsl::object_version.eq(version.value() as i64))
                    .first::<StoredObject>(conn)
                    .optional()
            } else {
                objects::dsl::objects
                    .filter(objects::dsl::object_id.eq(object_id))
                    .first::<StoredObject>(conn)
                    .optional()
            }
        })?;
        Ok(stored_object)
    }

    fn get_object(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<Object>, IndexerError> {
        let Some(stored_package) = self.get_object_from_db(object_id, version)? else {
            return Ok(None);
        };

        let object = stored_package.try_into()?;
        Ok(Some(object))
    }

    pub async fn get_object_in_blocking_task(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<Object>, IndexerError> {
        self.spawn_blocking(move |this| this.get_object(&object_id, None))
            .await
    }

    pub async fn get_object_read_in_blocking_task(
        &self,
        object_id: ObjectID,
    ) -> Result<ObjectRead, IndexerError> {
        let stored_object = self
            .spawn_blocking(move |this| this.get_object_raw(object_id))
            .await?;

        if let Some(object) = stored_object {
            object
                .try_into_object_read(self.package_resolver.clone())
                .await
        } else {
            Ok(ObjectRead::NotExists(object_id))
        }
    }

    fn get_object_raw(&self, object_id: ObjectID) -> Result<Option<StoredObject>, IndexerError> {
        let id = object_id.to_vec();
        let stored_object = run_query!(&self.pool, |conn| {
            objects::dsl::objects
                .filter(objects::dsl::object_id.eq(id))
                .first::<StoredObject>(conn)
                .optional()
        })?;
        Ok(stored_object)
    }

    pub async fn get_package(&self, package_id: ObjectID) -> Result<Package, IndexerError> {
        let store = self.package_resolver.package_store();
        let pkg = store
            .fetch(package_id.into())
            .await
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Fail to fetch package from package store with error {:?}",
                    e
                ))
            })?
            .as_ref()
            .clone();
        Ok(pkg)
    }

    pub fn get_epoch_info_from_db(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<Option<StoredEpochInfo>, IndexerError> {
        let stored_epoch = run_query!(&self.pool, |conn| {
            if let Some(epoch) = epoch {
                epochs::dsl::epochs
                    .filter(epochs::epoch.eq(epoch as i64))
                    .first::<StoredEpochInfo>(conn)
                    .optional()
            } else {
                epochs::dsl::epochs
                    .order_by(epochs::epoch.desc())
                    .first::<StoredEpochInfo>(conn)
                    .optional()
            }
        })?;

        Ok(stored_epoch)
    }

    pub fn get_latest_epoch_info_from_db(&self) -> Result<StoredEpochInfo, IndexerError> {
        let stored_epoch = run_query!(&self.pool, |conn| {
            epochs::dsl::epochs
                .order_by(epochs::epoch.desc())
                .first::<StoredEpochInfo>(conn)
        })?;

        Ok(stored_epoch)
    }

    pub fn get_epoch_info(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<Option<EpochInfo>, IndexerError> {
        let stored_epoch = self.get_epoch_info_from_db(epoch)?;

        let stored_epoch = match stored_epoch {
            Some(stored_epoch) => stored_epoch,
            None => return Ok(None),
        };

        let epoch_info = EpochInfo::try_from(stored_epoch)?;
        Ok(Some(epoch_info))
    }

    fn get_epochs_from_db(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<StoredEpochInfo>, IndexerError> {
        run_query!(&self.pool, |conn| {
            let mut boxed_query = epochs::table.into_boxed();
            if let Some(cursor) = cursor {
                if descending_order {
                    boxed_query = boxed_query.filter(epochs::epoch.lt(cursor as i64));
                } else {
                    boxed_query = boxed_query.filter(epochs::epoch.gt(cursor as i64));
                }
            }
            if descending_order {
                boxed_query = boxed_query.order_by(epochs::epoch.desc());
            } else {
                boxed_query = boxed_query.order_by(epochs::epoch.asc());
            }

            boxed_query.limit(limit as i64).load(conn)
        })
    }

    pub fn get_epochs(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        self.get_epochs_from_db(cursor, limit, descending_order)?
            .into_iter()
            .map(EpochInfo::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, IndexerError> {
        let system_state: SuiSystemStateSummary =
            sui_types::sui_system_state::get_sui_system_state(self)?
                .into_sui_system_state_summary();
        Ok(system_state)
    }

    /// Retrieve the system state data for the given epoch. If no epoch is given,
    /// it will retrieve the latest epoch's data and return the system state.
    /// System state of the an epoch is written at the end of the epoch, so system state
    /// of the current epoch is empty until the epoch ends. You can call
    /// `get_latest_sui_system_state` for current epoch instead.
    pub fn get_epoch_sui_system_state(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<SuiSystemStateSummary, IndexerError> {
        let stored_epoch = self.get_epoch_info_from_db(epoch)?;
        let stored_epoch = match stored_epoch {
            Some(stored_epoch) => stored_epoch,
            None => return Err(IndexerError::InvalidArgumentError("Invalid epoch".into())),
        };

        let system_state: SuiSystemStateSummary = bcs::from_bytes(&stored_epoch.system_state)
            .map_err(|_| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to deserialize `system_state` for epoch {:?}",
                    epoch,
                ))
            })?;
        Ok(system_state)
    }

    pub fn get_checkpoint_from_db(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<StoredCheckpoint>, IndexerError> {
        let stored_checkpoint = run_query!(&self.pool, |conn| {
            match checkpoint_id {
                CheckpointId::SequenceNumber(seq) => checkpoints::dsl::checkpoints
                    .filter(checkpoints::sequence_number.eq(seq as i64))
                    .first::<StoredCheckpoint>(conn)
                    .optional(),
                CheckpointId::Digest(digest) => checkpoints::dsl::checkpoints
                    .filter(checkpoints::checkpoint_digest.eq(digest.into_inner().to_vec()))
                    .first::<StoredCheckpoint>(conn)
                    .optional(),
            }
        })?;

        Ok(stored_checkpoint)
    }

    pub fn get_latest_checkpoint_from_db(&self) -> Result<StoredCheckpoint, IndexerError> {
        let stored_checkpoint = run_query!(&self.pool, |conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::sequence_number.desc())
                .first::<StoredCheckpoint>(conn)
        })?;

        Ok(stored_checkpoint)
    }

    pub fn get_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<sui_json_rpc_types::Checkpoint>, IndexerError> {
        let stored_checkpoint = match self.get_checkpoint_from_db(checkpoint_id)? {
            Some(stored_checkpoint) => stored_checkpoint,
            None => return Ok(None),
        };

        let checkpoint = sui_json_rpc_types::Checkpoint::try_from(stored_checkpoint)?;
        Ok(Some(checkpoint))
    }

    pub fn get_latest_checkpoint(&self) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        let stored_checkpoint = self.get_latest_checkpoint_from_db()?;

        sui_json_rpc_types::Checkpoint::try_from(stored_checkpoint)
    }

    fn get_checkpoints_from_db(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<StoredCheckpoint>, IndexerError> {
        run_query!(&self.pool, |conn| {
            let mut boxed_query = checkpoints::table.into_boxed();
            if let Some(cursor) = cursor {
                if descending_order {
                    boxed_query =
                        boxed_query.filter(checkpoints::sequence_number.lt(cursor as i64));
                } else {
                    boxed_query =
                        boxed_query.filter(checkpoints::sequence_number.gt(cursor as i64));
                }
            }
            if descending_order {
                boxed_query = boxed_query.order_by(checkpoints::sequence_number.desc());
            } else {
                boxed_query = boxed_query.order_by(checkpoints::sequence_number.asc());
            }

            boxed_query
                .limit(limit as i64)
                .load::<StoredCheckpoint>(conn)
        })
    }

    pub fn get_checkpoints(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<sui_json_rpc_types::Checkpoint>, IndexerError> {
        self.get_checkpoints_from_db(cursor, limit, descending_order)?
            .into_iter()
            .map(sui_json_rpc_types::Checkpoint::try_from)
            .collect()
    }

    fn get_transaction_effects_with_digest(
        &self,
        digest: TransactionDigest,
    ) -> Result<SuiTransactionBlockEffects, IndexerError> {
        let stored_txn: StoredTransaction = run_query!(&self.pool, |conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.into_inner().to_vec()))
                .first::<StoredTransaction>(conn)
        })?;

        stored_txn.try_into_sui_transaction_effects()
    }

    fn get_transaction_effects_with_sequence_number(
        &self,
        sequence_number: i64,
    ) -> Result<SuiTransactionBlockEffects, IndexerError> {
        let stored_txn: StoredTransaction = run_query!(&self.pool, |conn| {
            transactions::table
                .filter(transactions::tx_sequence_number.eq(sequence_number))
                .first::<StoredTransaction>(conn)
        })?;

        stored_txn.try_into_sui_transaction_effects()
    }

    fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        let digests = digests
            .iter()
            .map(|digest| digest.inner().to_vec())
            .collect::<Vec<_>>();
        run_query!(&self.pool, |conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq_any(digests))
                .load::<StoredTransaction>(conn)
        })
    }

    async fn multi_get_transactions_in_blocking_task(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        self.spawn_blocking(move |this| this.multi_get_transactions(&digests))
            .await
    }

    async fn stored_transaction_to_transaction_block(
        &self,
        stored_txes: Vec<StoredTransaction>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        let mut tx_block_responses_futures = vec![];
        for stored_tx in stored_txes {
            let package_resolver_clone = self.package_resolver();
            let options_clone = options.clone();
            tx_block_responses_futures.push(tokio::task::spawn(
                stored_tx
                    .try_into_sui_transaction_block_response(options_clone, package_resolver_clone),
            ));
        }

        let tx_blocks = futures::future::join_all(tx_block_responses_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Failed to join all tx block futures: {}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Failed to collect tx block futures: {}", e))?;
        Ok(tx_blocks)
    }

    fn multi_get_transactions_with_sequence_numbers(
        &self,
        tx_sequence_numbers: Vec<i64>,
        // Some(true) for desc, Some(false) for asc, None for undefined order
        is_descending: Option<bool>,
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        let mut query = transactions::table
            .filter(transactions::tx_sequence_number.eq_any(tx_sequence_numbers))
            .into_boxed();
        match is_descending {
            Some(true) => {
                query = query.order(transactions::dsl::tx_sequence_number.desc());
            }
            Some(false) => {
                query = query.order(transactions::dsl::tx_sequence_number.asc());
            }
            None => (),
        }
        run_query!(&self.pool, |conn| query.load::<StoredTransaction>(conn))
    }

    pub async fn get_owned_objects_in_blocking_task(
        &self,
        address: SuiAddress,
        filter: Option<SuiObjectDataFilter>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        self.spawn_blocking(move |this| this.get_owned_objects_impl(address, filter, cursor, limit))
            .await
    }

    fn get_owned_objects_impl(
        &self,
        address: SuiAddress,
        filter: Option<SuiObjectDataFilter>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        run_query!(&self.pool, |conn| {
            let mut query = objects::dsl::objects
                .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
                .filter(objects::dsl::owner_id.eq(address.to_vec()))
                .order(objects::dsl::object_id.asc())
                .limit(limit as i64)
                .into_boxed();
            if let Some(filter) = filter {
                match filter {
                    SuiObjectDataFilter::StructType(struct_tag) => {
                        let object_type =
                            struct_tag.to_canonical_string(/* with_prefix */ true);
                        query =
                            query.filter(objects::object_type.like(format!("{}%", object_type)));
                    }
                    SuiObjectDataFilter::MatchAny(filters) => {
                        let mut condition = "(".to_string();
                        for (i, filter) in filters.iter().enumerate() {
                            if let SuiObjectDataFilter::StructType(struct_tag) = filter {
                                let object_type =
                                    struct_tag.to_canonical_string(/* with_prefix */ true);
                                if i == 0 {
                                    condition +=
                                        format!("objects.object_type LIKE '{}%'", object_type)
                                            .as_str();
                                } else {
                                    condition +=
                                        format!(" OR objects.object_type LIKE '{}%'", object_type)
                                            .as_str();
                                }
                            } else {
                                return Err(IndexerError::InvalidArgumentError(
                                    "Invalid filter type. Only struct, MatchAny and MatchNone of struct filters are supported.".into(),
                                ));
                            }
                        }
                        condition += ")";
                        query = query.filter(sql::<Bool>(&condition));
                    }
                    SuiObjectDataFilter::MatchNone(filters) => {
                        for filter in filters {
                            if let SuiObjectDataFilter::StructType(struct_tag) = filter {
                                let object_type =
                                    struct_tag.to_canonical_string(/* with_prefix */ true);
                                query = query.filter(
                                    objects::object_type.not_like(format!("{}%", object_type)),
                                );
                            } else {
                                return Err(IndexerError::InvalidArgumentError(
                                    "Invalid filter type. Only struct, MatchAny and MatchNone of struct filters are supported.".into(),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(IndexerError::InvalidArgumentError(
                            "Invalid filter type. Only struct, MatchAny and MatchNone of struct filters are supported.".into(),
                        ));
                    }
                }
            }

            if let Some(object_cursor) = cursor {
                query = query.filter(objects::dsl::object_id.gt(object_cursor.to_vec()));
            }

            query
                .load::<StoredObject>(conn)
                .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
        })
    }

    fn filter_object_id_with_type(
        &self,
        object_ids: Vec<ObjectID>,
        object_type: String,
    ) -> Result<Vec<ObjectID>, IndexerError> {
        let object_ids = object_ids.into_iter().map(|id| id.to_vec()).collect_vec();
        let filtered_ids = run_query!(&self.pool, |conn| {
            objects::dsl::objects
                .filter(objects::object_id.eq_any(object_ids))
                .filter(objects::object_type.eq(object_type))
                .select(objects::object_id)
                .load::<Vec<u8>>(conn)
        })?;

        filtered_ids
            .into_iter()
            .map(|id| {
                ObjectID::from_bytes(id.clone()).map_err(|_e| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Can't convert {:?} to ObjectID",
                        id,
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn multi_get_objects_in_blocking_task(
        &self,
        object_ids: Vec<ObjectID>,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        self.spawn_blocking(move |this| this.multi_get_objects_impl(object_ids))
            .await
    }

    fn multi_get_objects_impl(
        &self,
        object_ids: Vec<ObjectID>,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        let object_ids = object_ids.into_iter().map(|id| id.to_vec()).collect_vec();
        run_query!(&self.pool, |conn| {
            objects::dsl::objects
                .filter(objects::object_id.eq_any(object_ids))
                .load::<StoredObject>(conn)
        })
    }

    async fn query_transaction_blocks_by_checkpoint_impl(
        &self,
        checkpoint_seq: u64,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor_tx_seq: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        let mut query = transactions::dsl::transactions
            .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint_seq as i64))
            .into_boxed();

        // Translate transaction digest cursor to tx sequence number
        if let Some(cursor_tx_seq) = cursor_tx_seq {
            if is_descending {
                query = query.filter(transactions::dsl::tx_sequence_number.lt(cursor_tx_seq));
            } else {
                query = query.filter(transactions::dsl::tx_sequence_number.gt(cursor_tx_seq));
            }
        }
        if is_descending {
            query = query.order(transactions::dsl::tx_sequence_number.desc());
        } else {
            query = query.order(transactions::dsl::tx_sequence_number.asc());
        }
        let pool = self.get_pool();
        let stored_txes = run_query_async!(&pool, move |conn| query
            .limit(limit as i64)
            .load::<StoredTransaction>(conn))?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
            .await
    }

    pub async fn query_transaction_blocks_in_blocking_task(
        &self,
        filter: Option<TransactionFilter>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor: Option<TransactionDigest>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        self.query_transaction_blocks_impl(filter, options, cursor, limit, is_descending)
            .await
    }

    async fn query_transaction_blocks_impl(
        &self,
        filter: Option<TransactionFilter>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor: Option<TransactionDigest>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        let cursor_tx_seq = if let Some(cursor) = cursor {
            let pool = self.get_pool();
            let tx_seq = run_query_async!(&pool, move |conn| {
                transactions::dsl::transactions
                    .select(transactions::tx_sequence_number)
                    .filter(transactions::dsl::transaction_digest.eq(cursor.into_inner().to_vec()))
                    .first::<i64>(conn)
            })?;
            Some(tx_seq)
        } else {
            None
        };
        let cursor_clause = if let Some(cursor_tx_seq) = cursor_tx_seq {
            if is_descending {
                format!("AND {TX_SEQUENCE_NUMBER_STR} < {}", cursor_tx_seq)
            } else {
                format!("AND {TX_SEQUENCE_NUMBER_STR} > {}", cursor_tx_seq)
            }
        } else {
            "".to_string()
        };
        let order_str = if is_descending { "DESC" } else { "ASC" };
        let (table_name, main_where_clause) = match filter {
            // Processed above
            Some(TransactionFilter::Checkpoint(seq)) => {
                return self
                    .query_transaction_blocks_by_checkpoint_impl(
                        seq,
                        options,
                        cursor_tx_seq,
                        limit,
                        is_descending,
                    )
                    .await
            }
            // FIXME: sanitize module & function
            Some(TransactionFilter::MoveFunction {
                package,
                module,
                function,
            }) => {
                let package = Hex::encode(package.to_vec());
                match (module, function) {
                    (Some(module), Some(function)) => (
                        "tx_calls_fun".into(),
                        format!(
                            "package = '\\x{}'::bytea AND module = '{}' AND func = '{}'",
                            package, module, function
                        ),
                    ),
                    (Some(module), None) => (
                        "tx_calls_mod".into(),
                        format!(
                            "package = '\\x{}'::bytea AND module = '{}'",
                            package, module
                        ),
                    ),
                    (None, Some(_)) => {
                        return Err(IndexerError::InvalidArgumentError(
                            "Function cannot be present without Module.".into(),
                        ));
                    }
                    (None, None) => (
                        "tx_calls_pkg".into(),
                        format!("package = '\\x{}'::bytea", package),
                    ),
                }
            }
            Some(TransactionFilter::InputObject(object_id)) => {
                let object_id = Hex::encode(object_id.to_vec());
                (
                    "tx_input_objects".into(),
                    format!("object_id = '\\x{}'::bytea", object_id),
                )
            }
            Some(TransactionFilter::ChangedObject(object_id)) => {
                let object_id = Hex::encode(object_id.to_vec());
                (
                    "tx_changed_objects".into(),
                    format!("object_id = '\\x{}'::bytea", object_id),
                )
            }
            Some(TransactionFilter::FromAddress(from_address)) => {
                let from_address = Hex::encode(from_address.to_vec());
                (
                    "tx_senders".into(),
                    format!("sender = '\\x{}'::bytea", from_address),
                )
            }
            Some(TransactionFilter::ToAddress(to_address)) => {
                let to_address = Hex::encode(to_address.to_vec());
                (
                    "tx_recipients".into(),
                    format!("recipient = '\\x{}'::bytea", to_address),
                )
            }
            Some(TransactionFilter::FromAndToAddress { from, to }) => {
                let from_address = Hex::encode(from.to_vec());
                let to_address = Hex::encode(to.to_vec());
                // Need to remove ambiguities for tx_sequence_number column
                let cursor_clause = if let Some(cursor_tx_seq) = cursor_tx_seq {
                    if is_descending {
                        format!(
                            "AND tx_senders.{TX_SEQUENCE_NUMBER_STR} < {}",
                            cursor_tx_seq
                        )
                    } else {
                        format!(
                            "AND tx_senders.{TX_SEQUENCE_NUMBER_STR} > {}",
                            cursor_tx_seq
                        )
                    }
                } else {
                    "".to_string()
                };
                let inner_query = format!(
                    "(SELECT tx_senders.{TX_SEQUENCE_NUMBER_STR} \
                    FROM tx_senders \
                    JOIN tx_recipients \
                    ON tx_senders.{TX_SEQUENCE_NUMBER_STR} = tx_recipients.{TX_SEQUENCE_NUMBER_STR} \
                    WHERE tx_senders.sender = '\\x{}'::BYTEA \
                    AND tx_recipients.recipient = '\\x{}'::BYTEA \
                    {} \
                    ORDER BY {TX_SEQUENCE_NUMBER_STR} {} \
                    LIMIT {}) AS inner_query
                    ",
                    from_address,
                    to_address,
                    cursor_clause,
                    order_str,
                    limit,
                );
                (inner_query, "1 = 1".into())
            }
            Some(TransactionFilter::FromOrToAddress { addr }) => {
                let address = Hex::encode(addr.to_vec());
                let inner_query = format!(
                    "( \
                        ( \
                            SELECT {TX_SEQUENCE_NUMBER_STR} FROM tx_senders \
                            WHERE sender = '\\x{}'::BYTEA {} \
                            ORDER BY {TX_SEQUENCE_NUMBER_STR} {} \
                            LIMIT {} \
                        ) \
                        UNION \
                        ( \
                            SELECT {TX_SEQUENCE_NUMBER_STR} FROM tx_recipients \
                            WHERE recipient = '\\x{}'::BYTEA {} \
                            ORDER BY {TX_SEQUENCE_NUMBER_STR} {} \
                            LIMIT {} \
                        ) \
                    ) AS combined",
                    address,
                    cursor_clause,
                    order_str,
                    limit,
                    address,
                    cursor_clause,
                    order_str,
                    limit,
                );
                (inner_query, "1 = 1".into())
            }
            Some(
                TransactionFilter::TransactionKind(_) | TransactionFilter::TransactionKindIn(_),
            ) => {
                return Err(IndexerError::NotSupportedError(
                    "TransactionKind filter is not supported.".into(),
                ));
            }
            None => {
                // apply no filter
                ("transactions".into(), "1 = 1".into())
            }
        };

        let query = format!(
            "SELECT {TX_SEQUENCE_NUMBER_STR} FROM {} WHERE {} {} ORDER BY {TX_SEQUENCE_NUMBER_STR} {} LIMIT {}",
            table_name,
            main_where_clause,
            cursor_clause,
            order_str,
            limit,
        );

        tracing::debug!("query transaction blocks: {}", query);
        let pool = self.get_pool();
        let tx_sequence_numbers = run_query_async!(&pool, move |conn| {
            diesel::sql_query(query.clone()).load::<TxSequenceNumber>(conn)
        })?
        .into_iter()
        .map(|tsn| tsn.tx_sequence_number)
        .collect::<Vec<i64>>();
        self.multi_get_transaction_block_response_by_sequence_numbers_in_blocking_task(
            tx_sequence_numbers,
            options,
            Some(is_descending),
        )
        .await
    }

    async fn multi_get_transaction_block_response_in_blocking_task_impl(
        &self,
        digests: &[TransactionDigest],
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        let stored_txes = self
            .multi_get_transactions_in_blocking_task(digests.to_vec())
            .await?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
            .await
    }

    async fn multi_get_transaction_block_response_by_sequence_numbers_in_blocking_task(
        &self,
        tx_sequence_numbers: Vec<i64>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        // Some(true) for desc, Some(false) for asc, None for undefined order
        is_descending: Option<bool>,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        let stored_txes: Vec<StoredTransaction> = self
            .spawn_blocking(move |this| {
                this.multi_get_transactions_with_sequence_numbers(
                    tx_sequence_numbers,
                    is_descending,
                )
            })
            .await?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
            .await
    }

    pub async fn multi_get_transaction_block_response_in_blocking_task(
        &self,
        digests: Vec<TransactionDigest>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        self.multi_get_transaction_block_response_in_blocking_task_impl(&digests, options)
            .await
    }

    pub async fn get_transaction_events_in_blocking_task(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<sui_json_rpc_types::SuiEvent>, IndexerError> {
        let pool = self.get_pool();
        let (timestamp_ms, serialized_events) = run_query_async!(&pool, move |conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.into_inner().to_vec()))
                .select((transactions::timestamp_ms, transactions::events))
                .first::<(i64, StoredTransactionEvents)>(conn)
        })?;

        let events = stored_events_to_events(serialized_events)?;
        let tx_events = TransactionEvents { data: events };

        let sui_tx_events = tx_events_to_sui_tx_events(
            tx_events,
            self.package_resolver(),
            digest,
            timestamp_ms as u64,
        )
        .await?;
        Ok(sui_tx_events.map_or(vec![], |ste| ste.data))
    }

    fn query_events_by_tx_digest_query(
        &self,
        tx_digest: TransactionDigest,
        cursor: Option<EventID>,
        limit: usize,
        descending_order: bool,
    ) -> IndexerResult<String> {
        let cursor = if let Some(cursor) = cursor {
            if cursor.tx_digest != tx_digest {
                return Err(IndexerError::InvalidArgumentError(
                    "Cursor tx_digest does not match the tx_digest in the query.".into(),
                ));
            }
            if descending_order {
                format!("e.{EVENT_SEQUENCE_NUMBER_STR} < {}", cursor.event_seq)
            } else {
                format!("e.{EVENT_SEQUENCE_NUMBER_STR} > {}", cursor.event_seq)
            }
        } else if descending_order {
            format!("e.{EVENT_SEQUENCE_NUMBER_STR} <= {}", i64::MAX)
        } else {
            format!("e.{EVENT_SEQUENCE_NUMBER_STR} >= {}", 0)
        };

        let order_clause = if descending_order { "DESC" } else { "ASC" };
        Ok(format!(
            "SELECT * \
            FROM EVENTS e \
            JOIN TRANSACTIONS t \
            ON t.tx_sequence_number = e.tx_sequence_number \
            AND t.transaction_digest = '\\x{}'::bytea \
            WHERE {cursor} \
            ORDER BY e.{EVENT_SEQUENCE_NUMBER_STR} {order_clause} \
            LIMIT {limit}
            ",
            Hex::encode(tx_digest.into_inner()),
        ))
    }

    pub async fn query_events_in_blocking_task(
        &self,
        filter: EventFilter,
        cursor: Option<EventID>,
        limit: usize,
        descending_order: bool,
    ) -> IndexerResult<Vec<SuiEvent>> {
        let pool = self.get_pool();
        let (tx_seq, event_seq) = if let Some(cursor) = cursor {
            let EventID {
                tx_digest,
                event_seq,
            } = cursor;
            let tx_seq = run_query_async!(&pool, move |conn| {
                transactions::dsl::transactions
                    .select(transactions::tx_sequence_number)
                    .filter(
                        transactions::dsl::transaction_digest.eq(tx_digest.into_inner().to_vec()),
                    )
                    .first::<i64>(conn)
            })?;
            (tx_seq, event_seq)
        } else if descending_order {
            let max_tx_seq: i64 = run_query_async!(&pool, move |conn| {
                events::dsl::events
                    .select(events::tx_sequence_number)
                    .order(events::dsl::tx_sequence_number.desc())
                    .first::<i64>(conn)
            })?;
            (max_tx_seq + 1, 0)
        } else {
            (-1, 0)
        };

        let query = if let EventFilter::Sender(sender) = &filter {
            // Need to remove ambiguities for tx_sequence_number column
            let cursor_clause = if descending_order {
                format!("(e.{TX_SEQUENCE_NUMBER_STR} < {} OR (e.{TX_SEQUENCE_NUMBER_STR} = {} AND e.{EVENT_SEQUENCE_NUMBER_STR} < {}))", tx_seq, tx_seq, event_seq)
            } else {
                format!("(e.{TX_SEQUENCE_NUMBER_STR} > {} OR (e.{TX_SEQUENCE_NUMBER_STR} = {} AND e.{EVENT_SEQUENCE_NUMBER_STR} > {}))", tx_seq, tx_seq, event_seq)
            };
            let order_clause = if descending_order {
                format!("e.{TX_SEQUENCE_NUMBER_STR} DESC, e.{EVENT_SEQUENCE_NUMBER_STR} DESC")
            } else {
                format!("e.{TX_SEQUENCE_NUMBER_STR} ASC, e.{EVENT_SEQUENCE_NUMBER_STR} ASC")
            };
            format!(
                "( \
                    SELECT *
                    FROM tx_senders s
                    JOIN events e
                    ON e.tx_sequence_number = s.tx_sequence_number
                    AND s.sender = '\\x{}'::bytea
                    WHERE {} \
                    ORDER BY {} \
                    LIMIT {}
                )",
                Hex::encode(sender.to_vec()),
                cursor_clause,
                order_clause,
                limit,
            )
        } else if let EventFilter::Transaction(tx_digest) = filter {
            self.query_events_by_tx_digest_query(tx_digest, cursor, limit, descending_order)?
        } else {
            let main_where_clause = match filter {
                EventFilter::Package(package_id) => {
                    format!("package = '\\x{}'::bytea", package_id.to_hex())
                }
                EventFilter::MoveModule { package, module } => {
                    format!(
                        "package = '\\x{}'::bytea AND module = '{}'",
                        package.to_hex(),
                        module,
                    )
                }
                EventFilter::MoveEventType(struct_tag) => {
                    format!("event_type = '{}'", struct_tag)
                }
                EventFilter::MoveEventModule { package, module } => {
                    let package_module_prefix = format!("{}::{}", package.to_hex_literal(), module);
                    format!("event_type LIKE '{package_module_prefix}::%'")
                }
                EventFilter::Sender(_) => {
                    // Processed above
                    unreachable!()
                }
                EventFilter::Transaction(_) => {
                    // Processed above
                    unreachable!()
                }
                EventFilter::MoveEventField { .. }
                | EventFilter::All(_)
                | EventFilter::Any(_)
                | EventFilter::And(_, _)
                | EventFilter::Or(_, _)
                | EventFilter::TimeRange { .. } => {
                    return Err(IndexerError::NotSupportedError(
                        "This type of EventFilter is not supported.".into(),
                    ));
                }
            };

            let cursor_clause = if descending_order {
                format!("AND ({TX_SEQUENCE_NUMBER_STR} < {} OR ({TX_SEQUENCE_NUMBER_STR} = {} AND {EVENT_SEQUENCE_NUMBER_STR} < {}))", tx_seq, tx_seq, event_seq)
            } else {
                format!("AND ({TX_SEQUENCE_NUMBER_STR} > {} OR ({TX_SEQUENCE_NUMBER_STR} = {} AND {EVENT_SEQUENCE_NUMBER_STR} > {}))", tx_seq, tx_seq, event_seq)
            };
            let order_clause = if descending_order {
                format!("{TX_SEQUENCE_NUMBER_STR} DESC, {EVENT_SEQUENCE_NUMBER_STR} DESC")
            } else {
                format!("{TX_SEQUENCE_NUMBER_STR} ASC, {EVENT_SEQUENCE_NUMBER_STR} ASC")
            };

            format!(
                "
                    SELECT * FROM events \
                    WHERE {} {} \
                    ORDER BY {} \
                    LIMIT {}
                ",
                main_where_clause, cursor_clause, order_clause, limit,
            )
        };
        tracing::debug!("query events: {}", query);
        let pool = self.get_pool();
        let stored_events = run_query_async!(&pool, move |conn| diesel::sql_query(query)
            .load::<StoredEvent>(conn))?;

        let mut sui_event_futures = vec![];
        for stored_event in stored_events {
            sui_event_futures.push(tokio::task::spawn(
                stored_event.try_into_sui_event(self.package_resolver.clone()),
            ));
        }

        let sui_events = futures::future::join_all(sui_event_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Failed to join sui event futures: {}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Failed to collect sui event futures: {}", e))?;
        Ok(sui_events)
    }

    pub async fn get_dynamic_fields_in_blocking_task(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<DynamicFieldInfo>, IndexerError> {
        let objects = self
            .spawn_blocking(move |this| {
                this.get_dynamic_fields_raw(parent_object_id, cursor, limit)
            })
            .await?;

        if any(objects.iter(), |o| o.df_object_id.is_none()) {
            return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                "Dynamic field has empty df_object_id column for parent object {}",
                parent_object_id
            )));
        }

        // for Dynamic field objects, df_object_id != object_id, we need another look up
        // to get the version and digests.
        // TODO: simply store df_object_version and df_object_digest as well?
        let dfo_ids = objects
            .iter()
            .filter_map(|o| {
                // Unwrap safe: checked nullity above
                if o.df_object_id.as_ref().unwrap() == &o.object_id {
                    None
                } else {
                    Some(o.df_object_id.clone().unwrap())
                }
            })
            .collect::<Vec<_>>();

        let object_refs = self
            .spawn_blocking(move |this| this.get_object_refs(dfo_ids))
            .await?;
        let mut df_futures = vec![];
        for object in objects {
            let package_resolver_clone = self.package_resolver.clone();
            df_futures.push(tokio::task::spawn(
                object.try_into_expectant_dynamic_field_info(package_resolver_clone),
            ));
        }
        let mut dynamic_fields = futures::future::join_all(df_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Error joining DF futures: {:?}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Error calling DF try_into function: {:?}", e))?;

        for df in dynamic_fields.iter_mut() {
            if let Some(obj_ref) = object_refs.get(&df.object_id) {
                df.version = obj_ref.1;
                df.digest = obj_ref.2;
            }
        }

        Ok(dynamic_fields)
    }

    pub async fn get_dynamic_fields_raw_in_blocking_task(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_dynamic_fields_raw(parent_object_id, cursor, limit)
        })
        .await
    }

    fn get_dynamic_fields_raw(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        let objects: Vec<StoredObject> = run_query!(&self.pool, |conn| {
            let mut query = objects::dsl::objects
                .filter(objects::dsl::owner_type.eq(OwnerType::Object as i16))
                .filter(objects::dsl::owner_id.eq(parent_object_id.to_vec()))
                .order(objects::dsl::object_id.asc())
                .limit(limit as i64)
                .into_boxed();
            if let Some(object_cursor) = cursor {
                query = query.filter(objects::dsl::object_id.gt(object_cursor.to_vec()));
            }
            query.load::<StoredObject>(conn)
        })?;

        Ok(objects)
    }

    pub async fn bcs_name_from_dynamic_field_name(
        &self,
        name: &DynamicFieldName,
    ) -> Result<Vec<u8>, IndexerError> {
        let move_type_layout = self
            .package_resolver()
            .type_layout(name.type_.clone())
            .await
            .map_err(|e| {
                IndexerError::ResolveMoveStructError(format!(
                    "Failed to get type layout for type {}: {}",
                    name.type_, e
                ))
            })?;
        let sui_json_value = sui_json::SuiJsonValue::new(name.value.clone())?;
        let name_bcs_value = sui_json_value.to_bcs_bytes(&move_type_layout)?;
        Ok(name_bcs_value)
    }

    fn get_object_refs(
        &self,
        object_ids: Vec<Vec<u8>>,
    ) -> IndexerResult<HashMap<ObjectID, ObjectRef>> {
        run_query!(&self.pool, |conn| {
            let query = objects::dsl::objects
                .select((
                    objects::dsl::object_id,
                    objects::dsl::object_version,
                    objects::dsl::object_digest,
                ))
                .filter(objects::dsl::object_id.eq_any(object_ids))
                .into_boxed();
            query.load::<ObjectRefColumn>(conn)
        })?
        .into_iter()
        .map(|object_ref: ObjectRefColumn| {
            let object_id = ObjectID::from_bytes(object_ref.object_id.clone()).map_err(|_e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Can't convert {:?} to ObjectID",
                    object_ref.object_id
                ))
            })?;
            let seq = SequenceNumber::from_u64(object_ref.object_version as u64);
            let object_digest = ObjectDigest::try_from(object_ref.object_digest.as_slice())
                .map_err(|e| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "object {:?} has incompatible object digest. Error: {e}",
                        object_ref.object_digest
                    ))
                })?;
            Ok((object_id, (object_id, seq, object_digest)))
        })
        .collect::<IndexerResult<HashMap<_, _>>>()
    }

    pub async fn get_display_object_by_type(
        &self,
        object_type: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<sui_types::display::DisplayVersionUpdatedEvent>, IndexerError> {
        let object_type = object_type.to_canonical_string(/* with_prefix */ true);
        self.spawn_blocking(move |this| this.get_display_update_event(object_type))
            .await
    }

    fn get_display_update_event(
        &self,
        object_type: String,
    ) -> Result<Option<sui_types::display::DisplayVersionUpdatedEvent>, IndexerError> {
        let stored_display = run_query!(&self.pool, |conn| {
            display::table
                .filter(display::object_type.eq(object_type))
                .first::<StoredDisplay>(conn)
                .optional()
        })?;

        let stored_display = match stored_display {
            Some(display) => display,
            None => return Ok(None),
        };

        let display_update = stored_display.to_display_update_event()?;

        Ok(Some(display_update))
    }

    pub async fn get_owned_coins_in_blocking_task(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: ObjectID,
        limit: usize,
    ) -> Result<Vec<SuiCoin>, IndexerError> {
        self.spawn_blocking(move |this| this.get_owned_coins(owner, coin_type, cursor, limit))
            .await
    }

    fn get_owned_coins(
        &self,
        owner: SuiAddress,
        // If coin_type is None, look for all coins.
        coin_type: Option<String>,
        cursor: ObjectID,
        limit: usize,
    ) -> Result<Vec<SuiCoin>, IndexerError> {
        let mut query = objects::dsl::objects
            .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
            .filter(objects::dsl::owner_id.eq(owner.to_vec()))
            .filter(objects::dsl::object_id.gt(cursor.to_vec()))
            .into_boxed();
        if let Some(coin_type) = coin_type {
            query = query.filter(objects::dsl::coin_type.eq(Some(coin_type)));
        } else {
            query = query.filter(objects::dsl::coin_type.is_not_null());
        }
        query = query
            .order((objects::dsl::coin_type.asc(), objects::dsl::object_id.asc()))
            .limit(limit as i64);

        let stored_objects = run_query!(&self.pool, |conn| query.load::<StoredObject>(conn))?;

        stored_objects
            .into_iter()
            .map(|o| o.try_into())
            .collect::<IndexerResult<Vec<_>>>()
    }

    pub async fn get_coin_balances_in_blocking_task(
        &self,
        owner: SuiAddress,
        // If coin_type is None, look for all coins.
        coin_type: Option<String>,
    ) -> Result<Vec<Balance>, IndexerError> {
        self.spawn_blocking(move |this| this.get_coin_balances(owner, coin_type))
            .await
    }

    fn get_coin_balances(
        &self,
        owner: SuiAddress,
        // If coin_type is None, look for all coins.
        coin_type: Option<String>,
    ) -> Result<Vec<Balance>, IndexerError> {
        let coin_type_filter = if let Some(coin_type) = coin_type {
            format!("= '{}'", coin_type)
        } else {
            "IS NOT NULL".to_string()
        };
        // Note: important to cast to BIGINT to avoid deserialize confusion
        let query = format!(
            "
            SELECT coin_type, \
            CAST(COUNT(*) AS BIGINT) AS coin_num, \
            CAST(SUM(coin_balance) AS BIGINT) AS coin_balance \
            FROM objects \
            WHERE owner_type = {} \
            AND owner_id = '\\x{}'::BYTEA \
            AND coin_type {} \
            GROUP BY coin_type \
            ORDER BY coin_type ASC
        ",
            OwnerType::Address as i16,
            Hex::encode(owner.to_vec()),
            coin_type_filter,
        );

        tracing::debug!("get coin balances query: {query}");
        let coin_balances = run_query!(&self.pool, |conn| diesel::sql_query(query)
            .load::<CoinBalance>(conn))?;
        coin_balances
            .into_iter()
            .map(|cb| cb.try_into())
            .collect::<IndexerResult<Vec<_>>>()
    }

    pub(crate) async fn get_display_fields(
        &self,
        original_object: &sui_types::object::Object,
        original_layout: &Option<MoveStructLayout>,
    ) -> Result<DisplayFieldsResponse, IndexerError> {
        let (object_type, layout) = if let Some((object_type, layout)) =
            sui_json_rpc::read_api::get_object_type_and_struct(original_object, original_layout)
                .map_err(|e| IndexerError::GenericError(e.to_string()))?
        {
            (object_type, layout)
        } else {
            return Ok(DisplayFieldsResponse {
                data: None,
                error: None,
            });
        };

        if let Some(display_object) = self.get_display_object_by_type(&object_type).await? {
            return sui_json_rpc::read_api::get_rendered_fields(display_object.fields, &layout)
                .map_err(|e| IndexerError::GenericError(e.to_string()));
        }
        Ok(DisplayFieldsResponse {
            data: None,
            error: None,
        })
    }

    pub async fn get_coin_metadata_in_blocking_task(
        &self,
        coin_struct: StructTag,
    ) -> Result<Option<SuiCoinMetadata>, IndexerError> {
        self.spawn_blocking(move |this| this.get_coin_metadata(coin_struct))
            .await
    }

    fn get_coin_metadata(
        &self,
        coin_struct: StructTag,
    ) -> Result<Option<SuiCoinMetadata>, IndexerError> {
        let package_id = coin_struct.address.into();
        let coin_metadata_type =
            CoinMetadata::type_(coin_struct).to_canonical_string(/* with_prefix */ true);
        let coin_metadata_obj_id = *self
            .package_obj_type_cache
            .lock()
            .unwrap()
            .cache_get_or_set_with(format!("{}{}", package_id, coin_metadata_type), || {
                get_single_obj_id_from_package_publish(self, package_id, coin_metadata_type.clone())
                    .unwrap()
            });
        if let Some(id) = coin_metadata_obj_id {
            let metadata_object = self.get_object(&id, None)?;
            Ok(metadata_object.and_then(|v| SuiCoinMetadata::try_from(v).ok()))
        } else {
            Ok(None)
        }
    }

    pub async fn get_total_supply_in_blocking_task(
        &self,
        coin_struct: StructTag,
    ) -> Result<Supply, IndexerError> {
        self.spawn_blocking(move |this| this.get_total_supply(coin_struct))
            .await
    }

    fn get_total_supply(&self, coin_struct: StructTag) -> Result<Supply, IndexerError> {
        let package_id = coin_struct.address.into();
        let treasury_cap_type =
            TreasuryCap::type_(coin_struct).to_canonical_string(/* with_prefix */ true);
        let treasury_cap_obj_id = self
            .package_obj_type_cache
            .lock()
            .unwrap()
            .cache_get_or_set_with(format!("{}{}", package_id, treasury_cap_type), || {
                get_single_obj_id_from_package_publish(self, package_id, treasury_cap_type.clone())
                    .unwrap()
            })
            .ok_or(IndexerError::GenericError(format!(
                "Cannot find treasury cap for type {}",
                treasury_cap_type
            )))?;
        let treasury_cap_obj_object =
            self.get_object(&treasury_cap_obj_id, None)?
                .ok_or(IndexerError::GenericError(format!(
                    "Cannot find treasury cap object with id {}",
                    treasury_cap_obj_id
                )))?;
        Ok(TreasuryCap::try_from(treasury_cap_obj_object)?.total_supply)
    }

    pub fn get_consistent_read_range(&self) -> Result<(i64, i64), IndexerError> {
        let latest_checkpoint_sequence = run_query!(&self.pool, |conn| {
            checkpoints::table
                .select(checkpoints::sequence_number)
                .order(checkpoints::sequence_number.desc())
                .first::<i64>(conn)
                .optional()
        })?
        .unwrap_or_default();
        let latest_object_snapshot_checkpoint_sequence = run_query!(&self.pool, |conn| {
            objects_snapshot::table
                .select(objects_snapshot::checkpoint_sequence_number)
                .order(objects_snapshot::checkpoint_sequence_number.desc())
                .first::<i64>(conn)
                .optional()
        })?
        .unwrap_or_default();
        Ok((
            latest_object_snapshot_checkpoint_sequence,
            latest_checkpoint_sequence,
        ))
    }

    pub fn package_resolver(&self) -> PackageResolver<U> {
        self.package_resolver.clone()
    }
}

impl<U: R2D2Connection> sui_types::storage::ObjectStore for IndexerReader<U> {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<sui_types::object::Object>, sui_types::storage::error::Error> {
        self.get_object(object_id, None)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<sui_types::object::Object>, sui_types::storage::error::Error> {
        self.get_object(object_id, Some(version))
            .map_err(sui_types::storage::error::Error::custom)
    }
}

fn get_single_obj_id_from_package_publish<U: R2D2Connection>(
    reader: &IndexerReader<U>,
    package_id: ObjectID,
    obj_type: String,
) -> Result<Option<ObjectID>, IndexerError> {
    let publish_txn_effects_opt = if is_system_package(package_id) {
        Some(reader.get_transaction_effects_with_sequence_number(0))
    } else {
        reader.get_object(&package_id, None)?.map(|o| {
            let publish_txn_digest = o.previous_transaction;
            reader.get_transaction_effects_with_digest(publish_txn_digest)
        })
    };
    if let Some(publish_txn_effects) = publish_txn_effects_opt {
        let created_objs = publish_txn_effects?
            .created()
            .iter()
            .map(|o| o.object_id())
            .collect::<Vec<_>>();
        let obj_ids_with_type =
            reader.filter_object_id_with_type(created_objs, obj_type.clone())?;
        if obj_ids_with_type.len() == 1 {
            Ok(Some(obj_ids_with_type[0]))
        } else if obj_ids_with_type.is_empty() {
            // The package exists but no such object is created in that transaction. Or maybe it is wrapped and we don't know yet.
            Ok(None)
        } else {
            // We expect there to be only one object of this type created by the package but more than one is found.
            tracing::error!(
                "There are more than one objects found for type {}",
                obj_type
            );
            Ok(None)
        }
    } else {
        // The coin package does not exist.
        Ok(None)
    }
}

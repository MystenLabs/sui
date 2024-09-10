// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;

use anyhow::Result;
use diesel::{
    dsl::sql, sql_types::Bool, ExpressionMethods, OptionalExtension, QueryDsl,
    TextExpressionMethods,
};
use itertools::Itertools;
use tap::{Pipe, TapFallible};
use tracing::{debug, error, warn};

use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use move_core_types::annotated_value::{MoveStructLayout, MoveTypeLayout};
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_json_rpc_types::DisplayFieldsResponse;
use sui_json_rpc_types::{Balance, Coin as SuiCoin, SuiCoinMetadata, SuiMoveValue};
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
    base_types::{ObjectID, SuiAddress, VersionNumber},
    committee::EpochId,
    digests::TransactionDigest,
    dynamic_field::{DynamicFieldInfo, DynamicFieldType},
    object::{Object, ObjectRead},
    sui_system_state::{sui_system_state_summary::SuiSystemStateSummary, SuiSystemStateTrait},
};
use sui_types::{coin::CoinMetadata, event::EventID};

use crate::database::ConnectionPool;
use crate::db::ConnectionPoolConfig;
use crate::models::transactions::{stored_events_to_events, StoredTransactionEvents};
use crate::{
    errors::IndexerError,
    models::{
        checkpoints::StoredCheckpoint,
        display::StoredDisplay,
        epoch::StoredEpochInfo,
        events::StoredEvent,
        objects::{CoinBalance, StoredObject},
        transactions::{tx_events_to_sui_tx_events, StoredTransaction},
        tx_indices::TxSequenceNumber,
    },
    schema::{checkpoints, display, epochs, events, objects, transactions},
    store::package_resolver::IndexerStorePackageResolver,
    types::{IndexerResult, OwnerType},
};

pub const TX_SEQUENCE_NUMBER_STR: &str = "tx_sequence_number";
pub const TRANSACTION_DIGEST_STR: &str = "transaction_digest";
pub const EVENT_SEQUENCE_NUMBER_STR: &str = "event_sequence_number";

#[derive(Clone)]
pub struct IndexerReader {
    pool: ConnectionPool,
    package_resolver: PackageResolver,
}

pub type PackageResolver = Arc<Resolver<PackageStoreWithLruCache<IndexerStorePackageResolver>>>;

// Impl for common initialization and utilities
impl IndexerReader {
    pub fn new(pool: ConnectionPool) -> Self {
        let indexer_store_pkg_resolver = IndexerStorePackageResolver::new(pool.clone());
        let package_cache = PackageStoreWithLruCache::new(indexer_store_pkg_resolver);
        let package_resolver = Arc::new(Resolver::new(package_cache));
        Self {
            pool,
            package_resolver,
        }
    }

    pub async fn new_with_config<T: Into<String>>(
        db_url: T,
        config: ConnectionPoolConfig,
    ) -> Result<Self> {
        let db_url = db_url.into();

        let pool = ConnectionPool::new(db_url.parse()?, config).await?;

        let indexer_store_pkg_resolver = IndexerStorePackageResolver::new(pool.clone());
        let package_cache = PackageStoreWithLruCache::new(indexer_store_pkg_resolver);
        let package_resolver = Arc::new(Resolver::new(package_cache));
        Ok(Self {
            pool,
            package_resolver,
        })
    }

    pub fn pool(&self) -> &ConnectionPool {
        &self.pool
    }
}

// Impl for reading data from the DB
impl IndexerReader {
    async fn get_object_from_db(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<StoredObject>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = objects::table
            .filter(objects::object_id.eq(object_id.to_vec()))
            .into_boxed();
        if let Some(version) = version {
            query = query.filter(objects::object_version.eq(version.value() as i64))
        }

        query
            .first::<StoredObject>(&mut connection)
            .await
            .optional()
            .map_err(Into::into)
    }

    pub async fn get_object(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<Object>, IndexerError> {
        let Some(stored_package) = self.get_object_from_db(object_id, version).await? else {
            return Ok(None);
        };

        let object = stored_package.try_into()?;
        Ok(Some(object))
    }

    pub async fn get_object_read(&self, object_id: ObjectID) -> Result<ObjectRead, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let stored_object = objects::table
            .filter(objects::object_id.eq(object_id.to_vec()))
            .first::<StoredObject>(&mut connection)
            .await
            .optional()?;

        if let Some(object) = stored_object {
            object
                .try_into_object_read(self.package_resolver.clone())
                .await
        } else {
            Ok(ObjectRead::NotExists(object_id))
        }
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

    async fn get_epoch_info_from_db(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<Option<StoredEpochInfo>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let stored_epoch = epochs::table
            .into_boxed()
            .pipe(|query| {
                if let Some(epoch) = epoch {
                    query.filter(epochs::epoch.eq(epoch as i64))
                } else {
                    query.order_by(epochs::epoch.desc())
                }
            })
            .first::<StoredEpochInfo>(&mut connection)
            .await
            .optional()?;

        Ok(stored_epoch)
    }

    pub async fn get_latest_epoch_info_from_db(&self) -> Result<StoredEpochInfo, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let stored_epoch = epochs::table
            .order_by(epochs::epoch.desc())
            .first::<StoredEpochInfo>(&mut connection)
            .await?;

        Ok(stored_epoch)
    }

    pub async fn get_epoch_info(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<Option<EpochInfo>, IndexerError> {
        let stored_epoch = self.get_epoch_info_from_db(epoch).await?;

        let stored_epoch = match stored_epoch {
            Some(stored_epoch) => stored_epoch,
            None => return Ok(None),
        };

        let epoch_info = EpochInfo::try_from(stored_epoch)?;
        Ok(Some(epoch_info))
    }

    async fn get_epochs_from_db(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<StoredEpochInfo>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = epochs::table.into_boxed();

        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(epochs::epoch.lt(cursor as i64));
            } else {
                query = query.filter(epochs::epoch.gt(cursor as i64));
            }
        }

        if descending_order {
            query = query.order_by(epochs::epoch.desc());
        } else {
            query = query.order_by(epochs::epoch.asc());
        }

        query
            .limit(limit as i64)
            .load(&mut connection)
            .await
            .map_err(Into::into)
    }

    pub async fn get_epochs(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        self.get_epochs_from_db(cursor, limit, descending_order)
            .await?
            .into_iter()
            .map(EpochInfo::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary, IndexerError> {
        let object_store = ConnectionAsObjectStore::from_pool(&self.pool)
            .await
            .map_err(|e| IndexerError::PgPoolConnectionError(e.to_string()))?;

        let system_state = tokio::task::spawn_blocking(move || {
            sui_types::sui_system_state::get_sui_system_state(&object_store)
        })
        .await
        .unwrap()?
        .into_sui_system_state_summary();

        Ok(system_state)
    }

    pub async fn get_validator_from_table(
        &self,
        table_id: ObjectID,
        pool_id: sui_types::id::ID,
    ) -> Result<
        sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary,
        IndexerError,
    > {
        let object_store = ConnectionAsObjectStore::from_pool(&self.pool)
            .await
            .map_err(|e| IndexerError::PgPoolConnectionError(e.to_string()))?;

        let validator = tokio::task::spawn_blocking(move || {
            sui_types::sui_system_state::get_validator_from_table(&object_store, table_id, &pool_id)
        })
        .await
        .unwrap()?;
        Ok(validator)
    }

    /// Retrieve the system state data for the given epoch. If no epoch is given,
    /// it will retrieve the latest epoch's data and return the system state.
    /// System state of the an epoch is written at the end of the epoch, so system state
    /// of the current epoch is empty until the epoch ends. You can call
    /// `get_latest_sui_system_state` for current epoch instead.
    pub async fn get_epoch_sui_system_state(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<SuiSystemStateSummary, IndexerError> {
        let stored_epoch = self.get_epoch_info_from_db(epoch).await?;
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

    async fn get_checkpoint_from_db(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<StoredCheckpoint>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;
        let stored_checkpoint = checkpoints::table
            .into_boxed()
            .pipe(|query| match checkpoint_id {
                CheckpointId::SequenceNumber(seq) => {
                    query.filter(checkpoints::sequence_number.eq(seq as i64))
                }
                CheckpointId::Digest(digest) => {
                    query.filter(checkpoints::checkpoint_digest.eq(digest.into_inner().to_vec()))
                }
            })
            .first::<StoredCheckpoint>(&mut connection)
            .await
            .optional()?;

        Ok(stored_checkpoint)
    }

    async fn get_latest_checkpoint_from_db(&self) -> Result<StoredCheckpoint, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        checkpoints::table
            .order_by(checkpoints::sequence_number.desc())
            .first::<StoredCheckpoint>(&mut connection)
            .await
            .map_err(Into::into)
    }

    pub async fn get_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<sui_json_rpc_types::Checkpoint>, IndexerError> {
        let stored_checkpoint = match self.get_checkpoint_from_db(checkpoint_id).await? {
            Some(stored_checkpoint) => stored_checkpoint,
            None => return Ok(None),
        };

        let checkpoint = sui_json_rpc_types::Checkpoint::try_from(stored_checkpoint)?;
        Ok(Some(checkpoint))
    }

    pub async fn get_latest_checkpoint(
        &self,
    ) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        let stored_checkpoint = self.get_latest_checkpoint_from_db().await?;

        sui_json_rpc_types::Checkpoint::try_from(stored_checkpoint)
    }

    async fn get_checkpoints_from_db(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<StoredCheckpoint>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = checkpoints::table.into_boxed();
        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(checkpoints::sequence_number.lt(cursor as i64));
            } else {
                query = query.filter(checkpoints::sequence_number.gt(cursor as i64));
            }
        }
        if descending_order {
            query = query.order_by(checkpoints::sequence_number.desc());
        } else {
            query = query.order_by(checkpoints::sequence_number.asc());
        }

        query
            .limit(limit as i64)
            .load::<StoredCheckpoint>(&mut connection)
            .await
            .map_err(Into::into)
    }

    pub async fn get_checkpoints(
        &self,
        cursor: Option<u64>,
        limit: usize,
        descending_order: bool,
    ) -> Result<Vec<sui_json_rpc_types::Checkpoint>, IndexerError> {
        self.get_checkpoints_from_db(cursor, limit, descending_order)
            .await?
            .into_iter()
            .map(sui_json_rpc_types::Checkpoint::try_from)
            .collect()
    }

    async fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let digests = digests
            .iter()
            .map(|digest| digest.inner().to_vec())
            .collect::<Vec<_>>();

        transactions::table
            .filter(transactions::transaction_digest.eq_any(digests))
            .load::<StoredTransaction>(&mut connection)
            .await
            .map_err(Into::into)
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
            .tap_err(|e| error!("Failed to join all tx block futures: {}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| error!("Failed to collect tx block futures: {}", e))?;
        Ok(tx_blocks)
    }

    async fn multi_get_transactions_with_sequence_numbers(
        &self,
        tx_sequence_numbers: Vec<i64>,
        // Some(true) for desc, Some(false) for asc, None for undefined order
        is_descending: Option<bool>,
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

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

        query
            .load::<StoredTransaction>(&mut connection)
            .await
            .map_err(Into::into)
    }

    pub async fn get_owned_objects(
        &self,
        address: SuiAddress,
        filter: Option<SuiObjectDataFilter>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = objects::table
            .filter(objects::owner_type.eq(OwnerType::Address as i16))
            .filter(objects::owner_id.eq(address.to_vec()))
            .order(objects::object_id.asc())
            .limit(limit as i64)
            .into_boxed();
        if let Some(filter) = filter {
            match filter {
                SuiObjectDataFilter::StructType(struct_tag) => {
                    let object_type = struct_tag.to_canonical_string(/* with_prefix */ true);
                    query = query.filter(objects::object_type.like(format!("{}%", object_type)));
                }
                SuiObjectDataFilter::MatchAny(filters) => {
                    let mut condition = "(".to_string();
                    for (i, filter) in filters.iter().enumerate() {
                        if let SuiObjectDataFilter::StructType(struct_tag) = filter {
                            let object_type =
                                struct_tag.to_canonical_string(/* with_prefix */ true);
                            if i == 0 {
                                condition +=
                                    format!("objects.object_type LIKE '{}%'", object_type).as_str();
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
                            query = query
                                .filter(objects::object_type.not_like(format!("{}%", object_type)));
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
            query = query.filter(objects::object_id.gt(object_cursor.to_vec()));
        }

        query
            .load::<StoredObject>(&mut connection)
            .await
            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
    }

    pub async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;
        let object_ids = object_ids.into_iter().map(|id| id.to_vec()).collect_vec();

        objects::table
            .filter(objects::object_id.eq_any(object_ids))
            .load::<StoredObject>(&mut connection)
            .await
            .map_err(Into::into)
    }

    async fn query_transaction_blocks_by_checkpoint(
        &self,
        checkpoint_seq: u64,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor_tx_seq: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = transactions::table
            .filter(transactions::checkpoint_sequence_number.eq(checkpoint_seq as i64))
            .into_boxed();

        // Translate transaction digest cursor to tx sequence number
        if let Some(cursor_tx_seq) = cursor_tx_seq {
            if is_descending {
                query = query.filter(transactions::tx_sequence_number.lt(cursor_tx_seq));
            } else {
                query = query.filter(transactions::tx_sequence_number.gt(cursor_tx_seq));
            }
        }
        if is_descending {
            query = query.order(transactions::tx_sequence_number.desc());
        } else {
            query = query.order(transactions::tx_sequence_number.asc());
        }
        let stored_txes = query
            .limit(limit as i64)
            .load::<StoredTransaction>(&mut connection)
            .await?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
            .await
    }

    pub async fn query_transaction_blocks(
        &self,
        filter: Option<TransactionFilter>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor: Option<TransactionDigest>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let cursor_tx_seq = if let Some(cursor) = cursor {
            let tx_seq = transactions::table
                .select(transactions::tx_sequence_number)
                .filter(transactions::transaction_digest.eq(cursor.into_inner().to_vec()))
                .first::<i64>(&mut connection)
                .await?;
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
                    .query_transaction_blocks_by_checkpoint(
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

        debug!("query transaction blocks: {}", query);
        let tx_sequence_numbers = diesel::sql_query(query.clone())
            .load::<TxSequenceNumber>(&mut connection)
            .await?
            .into_iter()
            .map(|tsn| tsn.tx_sequence_number)
            .collect::<Vec<i64>>();
        self.multi_get_transaction_block_response_by_sequence_numbers(
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
        let stored_txes = self.multi_get_transactions(digests).await?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
            .await
    }

    async fn multi_get_transaction_block_response_by_sequence_numbers(
        &self,
        tx_sequence_numbers: Vec<i64>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        // Some(true) for desc, Some(false) for asc, None for undefined order
        is_descending: Option<bool>,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        let stored_txes: Vec<StoredTransaction> = self
            .multi_get_transactions_with_sequence_numbers(tx_sequence_numbers, is_descending)
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

    pub async fn get_transaction_events(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<sui_json_rpc_types::SuiEvent>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let (timestamp_ms, serialized_events) = transactions::table
            .filter(transactions::transaction_digest.eq(digest.into_inner().to_vec()))
            .select((transactions::timestamp_ms, transactions::events))
            .first::<(i64, StoredTransactionEvents)>(&mut connection)
            .await?;

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

    pub async fn query_events(
        &self,
        filter: EventFilter,
        cursor: Option<EventID>,
        limit: usize,
        descending_order: bool,
    ) -> IndexerResult<Vec<SuiEvent>> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let (tx_seq, event_seq) = if let Some(cursor) = cursor {
            let EventID {
                tx_digest,
                event_seq,
            } = cursor;
            let tx_seq = transactions::table
                .select(transactions::tx_sequence_number)
                .filter(transactions::transaction_digest.eq(tx_digest.into_inner().to_vec()))
                .first::<i64>(&mut connection)
                .await?;
            (tx_seq, event_seq)
        } else if descending_order {
            let max_tx_seq = events::table
                .select(events::tx_sequence_number)
                .order(events::tx_sequence_number.desc())
                .first::<i64>(&mut connection)
                .await?;
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
        debug!("query events: {}", query);
        let stored_events = diesel::sql_query(query)
            .load::<StoredEvent>(&mut connection)
            .await?;

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
            .tap_err(|e| error!("Failed to join sui event futures: {}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| error!("Failed to collect sui event futures: {}", e))?;
        Ok(sui_events)
    }

    pub async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<DynamicFieldInfo>, IndexerError> {
        let stored_objects = self
            .get_dynamic_fields_raw(parent_object_id, cursor, limit)
            .await?;
        let mut df_futures = vec![];
        let indexer_reader_arc = Arc::new(self.clone());
        for stored_object in stored_objects {
            let indexer_reader_arc_clone = Arc::clone(&indexer_reader_arc);
            df_futures.push(tokio::task::spawn(async move {
                indexer_reader_arc_clone
                    .try_create_dynamic_field_info(stored_object)
                    .await
            }));
        }
        let df_infos = futures::future::join_all(df_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| error!("Error joining DF futures: {:?}", e))?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| error!("Error calling try_create_dynamic_field_info: {:?}", e))?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Ok(df_infos)
    }

    pub async fn get_dynamic_fields_raw(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut query = objects::table
            .filter(objects::owner_type.eq(OwnerType::Object as i16))
            .filter(objects::owner_id.eq(parent_object_id.to_vec()))
            .order(objects::object_id.asc())
            .limit(limit as i64)
            .into_boxed();

        if let Some(object_cursor) = cursor {
            query = query.filter(objects::object_id.gt(object_cursor.to_vec()));
        }

        query
            .load::<StoredObject>(&mut connection)
            .await
            .map_err(Into::into)
    }

    async fn try_create_dynamic_field_info(
        &self,
        stored_object: StoredObject,
    ) -> Result<Option<DynamicFieldInfo>, IndexerError> {
        if stored_object.df_kind.is_none() {
            return Ok(None);
        }

        let object: Object = stored_object.try_into()?;
        let move_object = match object.data.try_as_move().cloned() {
            Some(move_object) => move_object,
            None => {
                return Err(IndexerError::ResolveMoveStructError(
                    "Object is not a MoveObject".to_string(),
                ));
            }
        };
        let struct_tag: StructTag = move_object.type_().clone().into();
        let move_type_layout = self
            .package_resolver
            .type_layout(TypeTag::Struct(Box::new(struct_tag.clone())))
            .await
            .map_err(|e| {
                IndexerError::ResolveMoveStructError(format!(
                    "Failed to get type layout for type {}: {}",
                    struct_tag, e
                ))
            })?;
        let MoveTypeLayout::Struct(move_struct_layout) = move_type_layout else {
            return Err(IndexerError::ResolveMoveStructError(
                "MoveTypeLayout is not Struct".to_string(),
            ));
        };

        let move_struct = move_object.to_move_struct(&move_struct_layout)?;
        let (move_value, type_, object_id) =
            DynamicFieldInfo::parse_move_object(&move_struct).tap_err(|e| warn!("{e}"))?;
        let name_type = move_object.type_().try_extract_field_name(&type_)?;
        let bcs_name = bcs::to_bytes(&move_value.clone().undecorate()).map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to serialize dynamic field name {:?}: {e}",
                move_value
            ))
        })?;
        let name = DynamicFieldName {
            type_: name_type,
            value: SuiMoveValue::from(move_value).to_json_value(),
        };

        Ok(Some(match type_ {
            DynamicFieldType::DynamicObject => {
                let object = self.get_object(&object_id, None).await?.ok_or(
                    IndexerError::UncategorizedError(anyhow::anyhow!(
                        "Failed to find object_id {:?} when trying to create dynamic field info",
                        object_id
                    )),
                )?;

                let version = object.version();
                let digest = object.digest();
                let object_type = object.data.type_().unwrap().clone();
                DynamicFieldInfo {
                    name,
                    bcs_name,
                    type_,
                    object_type: object_type.to_canonical_string(/* with_prefix */ true),
                    object_id,
                    version,
                    digest,
                }
            }
            DynamicFieldType::DynamicField => DynamicFieldInfo {
                name,
                bcs_name,
                type_,
                object_type: move_object.into_type().into_type_params()[1]
                    .to_canonical_string(/* with_prefix */ true),
                object_id: object.id(),
                version: object.version(),
                digest: object.digest(),
            },
        }))
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

    async fn get_display_object_by_type(
        &self,
        object_type: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<sui_types::display::DisplayVersionUpdatedEvent>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let object_type = object_type.to_canonical_string(/* with_prefix */ true);
        let stored_display = display::table
            .filter(display::object_type.eq(object_type))
            .first::<StoredDisplay>(&mut connection)
            .await
            .optional()?;

        let stored_display = match stored_display {
            Some(display) => display,
            None => return Ok(None),
        };

        let display_update = stored_display.to_display_update_event()?;

        Ok(Some(display_update))
    }

    pub async fn get_owned_coins(
        &self,
        owner: SuiAddress,
        // If coin_type is None, look for all coins.
        coin_type: Option<String>,
        cursor: ObjectID,
        limit: usize,
    ) -> Result<Vec<SuiCoin>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;
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

        query
            .order((objects::dsl::coin_type.asc(), objects::dsl::object_id.asc()))
            .limit(limit as i64)
            .load::<StoredObject>(&mut connection)
            .await?
            .into_iter()
            .map(|o| o.try_into())
            .collect::<IndexerResult<Vec<_>>>()
    }

    pub async fn get_coin_balances(
        &self,
        owner: SuiAddress,
        // If coin_type is None, look for all coins.
        coin_type: Option<String>,
    ) -> Result<Vec<Balance>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

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

        debug!("get coin balances query: {query}");
        diesel::sql_query(query)
            .load::<CoinBalance>(&mut connection)
            .await?
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

    pub async fn get_singleton_object(&self, type_: &StructTag) -> Result<Option<Object>> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let object = match objects::table
            .filter(objects::object_type_package.eq(type_.address.to_vec()))
            .filter(objects::object_type_module.eq(type_.module.to_string()))
            .filter(objects::object_type_name.eq(type_.name.to_string()))
            .filter(objects::object_type.eq(type_.to_canonical_string(/* with_prefix */ true)))
            .first::<StoredObject>(&mut connection)
            .await
            .optional()?
        {
            Some(object) => object,
            None => return Ok(None),
        }
        .try_into()?;

        Ok(Some(object))
    }

    pub async fn get_coin_metadata(
        &self,
        coin_struct: StructTag,
    ) -> Result<Option<SuiCoinMetadata>, IndexerError> {
        let coin_metadata_type = CoinMetadata::type_(coin_struct);

        self.get_singleton_object(&coin_metadata_type)
            .await?
            .and_then(|o| SuiCoinMetadata::try_from(o).ok())
            .pipe(Ok)
    }

    pub async fn get_total_supply(&self, coin_struct: StructTag) -> Result<Supply, IndexerError> {
        let treasury_cap_type = TreasuryCap::type_(coin_struct);

        self.get_singleton_object(&treasury_cap_type)
            .await?
            .and_then(|o| TreasuryCap::try_from(o).ok())
            .ok_or(IndexerError::GenericError(format!(
                "Cannot find treasury cap object with type {}",
                treasury_cap_type
            )))?
            .total_supply
            .pipe(Ok)
    }

    pub fn package_resolver(&self) -> PackageResolver {
        self.package_resolver.clone()
    }
}

// NOTE: Do not make this public and easily accessible as we need to be careful that it is only
// used in non-async contexts via the use of tokio::task::spawn_blocking in order to avoid blocking
// the async runtime.
//
// Maybe we should look into introducing an async object store trait...
struct ConnectionAsObjectStore {
    inner: std::sync::Mutex<
        diesel_async::async_connection_wrapper::AsyncConnectionWrapper<
            crate::database::Connection<'static>,
        >,
    >,
}

impl ConnectionAsObjectStore {
    async fn from_pool(
        pool: &ConnectionPool,
    ) -> Result<Self, diesel_async::pooled_connection::PoolError> {
        let connection = std::sync::Mutex::new(pool.dedicated_connection().await?.into());

        Ok(Self { inner: connection })
    }

    fn get_object_from_db(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<StoredObject>, IndexerError> {
        use diesel::RunQueryDsl;

        let mut guard = self.inner.lock().unwrap();
        let connection: &mut diesel_async::async_connection_wrapper::AsyncConnectionWrapper<_> =
            &mut guard;

        let mut query = objects::table
            .filter(objects::object_id.eq(object_id.to_vec()))
            .into_boxed();
        if let Some(version) = version {
            query = query.filter(objects::object_version.eq(version.value() as i64))
        }

        query
            .first::<StoredObject>(connection)
            .optional()
            .map_err(Into::into)
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
}

impl sui_types::storage::ObjectStore for ConnectionAsObjectStore {
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

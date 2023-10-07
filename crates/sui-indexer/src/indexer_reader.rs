// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::models_v2::checkpoints::StoredCheckpoint;
use crate::models_v2::display::StoredDisplay;
use crate::schema_v2::display;
use crate::{
    errors::IndexerError,
    models_v2::{epoch::StoredEpochInfo, objects::ObjectRefColumn, packages::StoredPackage},
    models_v2::{objects::StoredObject, transactions::StoredTransaction, tx_indices::TxDigest},
    schema_v2::{checkpoints, epochs, objects, packages, transactions},
    types_v2::{IndexerResult, OwnerType},
    PgConnectionConfig, PgConnectionPoolConfig, PgPoolConnection,
};
use anyhow::{anyhow, Result};
use diesel::{
    r2d2::ConnectionManager, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl,
};
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use itertools::{any, Itertools};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};
use sui_json_rpc_types::{CheckpointId, EpochInfo, SuiTransactionBlockResponse, TransactionFilter};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, VersionNumber},
    committee::EpochId,
    digests::{ObjectDigest, TransactionDigest},
    dynamic_field::DynamicFieldInfo,
    move_package::MovePackage,
    object::Object,
    sui_system_state::{sui_system_state_summary::SuiSystemStateSummary, SuiSystemStateTrait},
};

pub const TX_SEQUENCE_NUMBER_STR: &str = "tx_sequence_number";
pub const TRANSACTION_DIGEST_STR: &str = "transaction_digest";

#[derive(Clone)]
pub struct IndexerReader {
    pool: crate::PgConnectionPool,
    package_cache: PackageCache,
}

// Impl for common initialization and utilities
impl IndexerReader {
    pub fn new<T: Into<String>>(db_url: T) -> Result<Self> {
        let config = PgConnectionPoolConfig::default();
        Self::new_with_config(db_url, config)
    }

    pub fn new_with_config<T: Into<String>>(
        db_url: T,
        config: PgConnectionPoolConfig,
    ) -> Result<Self> {
        let manager = ConnectionManager::<PgConnection>::new(db_url);

        let connection_config = PgConnectionConfig {
            statement_timeout: config.statement_timeout,
            read_only: true,
        };

        let pool = diesel::r2d2::Pool::builder()
            .max_size(config.pool_size)
            .connection_timeout(config.connection_timeout)
            .connection_customizer(Box::new(connection_config))
            .build(manager)
            .map_err(|e| anyhow!("Failed to initialize connection pool. Error: {:?}. If Error is None, please check whether the configured pool size (currently {}) exceeds the maximum number of connections allowed by the database.", e, config.pool_size))?;

        Ok(Self {
            pool,
            package_cache: Default::default(),
        })
    }

    fn get_connection(&self) -> Result<PgPoolConnection, IndexerError> {
        self.pool.get().map_err(|e| {
            IndexerError::PgPoolConnectionError(format!(
                "Failed to get connection from PG connection pool with error: {:?}",
                e
            ))
        })
    }

    pub fn run_query<T, E, F>(&self, query: F) -> Result<T, IndexerError>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E>,
        E: From<diesel::result::Error> + std::error::Error,
    {
        blocking_call_is_ok_or_panic();

        let mut connection = self.get_connection()?;
        connection
            .build_transaction()
            .read_only()
            .run(query)
            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
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

    pub async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, IndexerError>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.spawn_blocking(move |this| this.run_query(query)).await
    }
}

thread_local! {
    static CALLED_FROM_BLOCKING_POOL: std::cell::RefCell<bool> = std::cell::RefCell::new(false);
}

/// Check that we are in a context conducive to making blocking calls.
/// This is done by either:
/// - Checking that we are not inside a tokio runtime context
/// Or:
/// - If we are inside a tokio runtime context, ensure that the call went through
/// `IndexerReader::spawn_blocking` which properly moves the blocking call to a blocking thread
/// pool.
fn blocking_call_is_ok_or_panic() {
    if tokio::runtime::Handle::try_current().is_ok()
        && !CALLED_FROM_BLOCKING_POOL.with(|in_blocking_pool| *in_blocking_pool.borrow())
    {
        panic!(
            "You are calling a blocking DB operation directly on an async thread. \
                Please use IndexerReader::spawn_blocking instead to move the \
                operation to a blocking thread"
        );
    }
}

// Impl for reading data from the DB
impl IndexerReader {
    fn get_object_from_db(
        &self,
        object_id: &ObjectID,
        version: Option<VersionNumber>,
    ) -> Result<Option<StoredObject>, IndexerError> {
        let object_id = object_id.to_vec();

        let stored_object = self.run_query(|conn| {
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

    fn get_package_from_db(
        &self,
        package_id: &ObjectID,
    ) -> Result<Option<MovePackage>, IndexerError> {
        let package_id = package_id.to_vec();
        let stored_package = self.run_query(|conn| {
            packages::dsl::packages
                .filter(packages::dsl::package_id.eq(package_id))
                .first::<StoredPackage>(conn)
                .optional()
        })?;

        let stored_package = match stored_package {
            Some(pkg) => pkg,
            None => return Ok(None),
        };

        let move_package =
            bcs::from_bytes::<MovePackage>(&stored_package.move_package).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Error deserializing move package. Error: {}",
                    e
                ))
            })?;
        Ok(Some(move_package))
    }

    pub fn get_package(&self, package_id: &ObjectID) -> Result<Option<MovePackage>, IndexerError> {
        if let Some(package) = self.package_cache.get(package_id) {
            return Ok(Some(package));
        }

        match self.get_package_from_db(package_id) {
            Ok(Some(package)) => {
                self.package_cache.insert(*package_id, package.clone());

                Ok(Some(package))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn get_package_in_blocking_task(
        &self,
        package_id: ObjectID,
    ) -> Result<Option<MovePackage>, IndexerError> {
        self.spawn_blocking(move |this| this.get_package(&package_id))
            .await
    }

    pub fn get_epoch_info_from_db(
        &self,
        epoch: Option<EpochId>,
    ) -> Result<Option<StoredEpochInfo>, IndexerError> {
        let stored_epoch = self.run_query(|conn| {
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
        let stored_epoch = self.run_query(|conn| {
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
        self.run_query(|conn| {
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

    pub fn get_checkpoint_from_db(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<StoredCheckpoint>, IndexerError> {
        let stored_checkpoint = self.run_query(|conn| match checkpoint_id {
            CheckpointId::SequenceNumber(seq) => checkpoints::dsl::checkpoints
                .filter(checkpoints::sequence_number.eq(seq as i64))
                .first::<StoredCheckpoint>(conn)
                .optional(),
            CheckpointId::Digest(digest) => checkpoints::dsl::checkpoints
                .filter(checkpoints::checkpoint_digest.eq(digest.into_inner().to_vec()))
                .first::<StoredCheckpoint>(conn)
                .optional(),
        })?;

        Ok(stored_checkpoint)
    }

    pub fn get_latest_checkpoint_from_db(&self) -> Result<StoredCheckpoint, IndexerError> {
        let stored_checkpoint = self.run_query(|conn| {
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
        self.run_query(|conn| {
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

    fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        let digests = digests
            .iter()
            .map(|digest| digest.inner().to_vec())
            .collect::<Vec<_>>();
        self.run_query(|conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq_any(digests))
                .load::<StoredTransaction>(conn)
        })
    }

    fn stored_transaction_to_transaction_block(
        &self,
        stored_txes: Vec<StoredTransaction>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        stored_txes
            .into_iter()
            .map(|stored_tx| stored_tx.try_into_sui_transaction_block_response(&options, self))
            .collect::<IndexerResult<Vec<_>>>()
    }

    fn multi_get_transactions_with_digest_bytes(
        &self,
        digests: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredTransaction>, IndexerError> {
        self.run_query(|conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq_any(digests))
                .load::<StoredTransaction>(conn)
        })
    }

    pub async fn get_owned_objects_in_blocking_task(
        &self,
        address: SuiAddress,
        object_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_owned_objects_impl(address, object_type, cursor, limit)
        })
        .await
    }

    fn get_owned_objects_impl(
        &self,
        address: SuiAddress,
        object_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<StoredObject>, IndexerError> {
        self.run_query(|conn| {
            let mut query = objects::dsl::objects
                .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
                .filter(objects::dsl::owner_id.eq(address.to_vec()))
                .limit(limit as i64)
                .into_boxed();
            if let Some(object_type) = object_type {
                query = query.filter(objects::dsl::object_type.eq(object_type));
            }

            if let Some(object_cursor) = cursor {
                query = query.filter(objects::dsl::object_id.gt(object_cursor.to_vec()));
            }
            query.load::<StoredObject>(conn)
        })
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

        self.run_query(|conn| {
            objects::dsl::objects
                .filter(objects::object_id.eq_any(object_ids))
                .load::<StoredObject>(conn)
        })
    }

    fn query_transaction_blocks_by_checkpoint_impl(
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
                query = query.filter(transactions::dsl::tx_sequence_number.le(cursor_tx_seq));
            } else {
                query = query.filter(transactions::dsl::tx_sequence_number.ge(cursor_tx_seq));
            }
        }
        if is_descending {
            query = query.order(transactions::dsl::tx_sequence_number.desc());
        } else {
            query = query.order(transactions::dsl::tx_sequence_number.asc());
        }

        let stored_txes =
            self.run_query(|conn| query.limit((limit) as i64).load::<StoredTransaction>(conn))?;

        self.stored_transaction_to_transaction_block(stored_txes, options)
    }

    pub async fn query_transaction_blocks_in_blocking_task(
        &self,
        filter: Option<TransactionFilter>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor: Option<TransactionDigest>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        self.spawn_blocking(move |this| {
            this.query_transaction_blocks_impl(filter, options, cursor, limit, is_descending)
        })
        .await
    }

    fn query_transaction_blocks_impl(
        &self,
        filter: Option<TransactionFilter>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
        cursor: Option<TransactionDigest>,
        limit: usize,
        is_descending: bool,
    ) -> IndexerResult<Vec<SuiTransactionBlockResponse>> {
        let cursor_tx_seq = if let Some(cursor) = cursor {
            Some(self.run_query(|conn| {
                transactions::dsl::transactions
                    .select(transactions::tx_sequence_number)
                    .filter(transactions::dsl::transaction_digest.eq(cursor.into_inner().to_vec()))
                    .first::<i64>(conn)
            })?)
        } else {
            None
        };

        let main_where_clause = match filter {
            // Processed above
            Some(TransactionFilter::Checkpoint(seq)) => {
                return self.query_transaction_blocks_by_checkpoint_impl(
                    seq,
                    options,
                    cursor_tx_seq,
                    limit,
                    is_descending,
                )
            }
            // FIXME: sanitize module & function
            Some(TransactionFilter::MoveFunction {
                package,
                module,
                function,
            }) => match (module, function) {
                (Some(module), Some(function)) => {
                    let package_module_function = format!("{}::{}::{}", package, module, function);
                    format!(
                        "package_module_functions @> ARRAY['{}']",
                        package_module_function
                    )
                }
                (Some(module), None) => {
                    let package_module = format!("{}::{}", package, module);
                    format!("package_modules @> ARRAY['{}']", package_module)
                }
                (None, Some(_)) => {
                    return Err(IndexerError::InvalidArgumentError(
                        "Function can be present wihtout Module.".into(),
                    ));
                }
                (None, None) => {
                    let package = Hex::encode(package.to_vec());
                    format!("packages @> ARRAY['\\x{}'::bytea]", package)
                }
            },
            Some(TransactionFilter::InputObject(object_id)) => {
                let object_id = Hex::encode(object_id.to_vec());
                format!("input_objects @> ARRAY['\\x{}'::bytea]", object_id)
            }
            Some(TransactionFilter::ChangedObject(object_id)) => {
                let object_id = Hex::encode(object_id.to_vec());
                format!("changed_objects @> ARRAY['\\x{}'::bytea]", object_id)
            }
            Some(TransactionFilter::FromAddress(from_address)) => {
                let from_address = Hex::encode(from_address.to_vec());
                format!("senders @> ARRAY['\\x{}'::bytea]", from_address)
            }
            Some(TransactionFilter::ToAddress(to_address)) => {
                let to_address = Hex::encode(to_address.to_vec());
                format!("recipients @> ARRAY['\\x{}'::bytea]", to_address)
            }
            Some(TransactionFilter::FromAndToAddress { from, to }) => {
                let from_address = Hex::encode(from.to_vec());
                let to_address = Hex::encode(to.to_vec());
                format!(
                    "(senders @> ARRAY['\\x{}'::bytea] AND recipients @> ARRAY['\\x{}'::bytea])",
                    from_address, to_address
                )
            }
            Some(TransactionFilter::FromOrToAddress { addr }) => {
                let address = Hex::encode(addr.to_vec());
                format!(
                    "(senders @> ARRAY['\\x{}'::bytea] OR recipients @> ARRAY['\\x{}'::bytea])",
                    address, address
                )
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
                "1 = 1".into()
            }
        };
        let cursor_clause = if let Some(cursor_tx_seq) = cursor_tx_seq {
            if is_descending {
                format!("AND {TX_SEQUENCE_NUMBER_STR} <= {}", cursor_tx_seq)
            } else {
                format!("AND {TX_SEQUENCE_NUMBER_STR} >= {}", cursor_tx_seq)
            }
        } else {
            "".to_string()
        };
        let query = format!(
            "SELECT {TRANSACTION_DIGEST_STR} FROM tx_indices WHERE {} {} ORDER BY {TX_SEQUENCE_NUMBER_STR} {} LIMIT {}",
            main_where_clause,
            cursor_clause,
            if is_descending { "DESC" } else { "ASC" },
            limit,
        );

        tracing::debug!("query_transaction_blocks: {}", query);

        let tx_digests = self
            .run_query(|conn| diesel::sql_query(query.clone()).load::<TxDigest>(conn))?
            .into_iter()
            .map(|td| td.transaction_digest)
            .collect::<Vec<_>>();

        self.multi_get_transaction_block_response_by_digest_bytes(tx_digests, options)
    }

    fn multi_get_transaction_block_response_impl(
        &self,
        digests: &[TransactionDigest],
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        let stored_txes = self.multi_get_transactions(digests)?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
    }

    fn multi_get_transaction_block_response_by_digest_bytes(
        &self,
        digests: Vec<Vec<u8>>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        let stored_txes = self.multi_get_transactions_with_digest_bytes(digests)?;
        self.stored_transaction_to_transaction_block(stored_txes, options)
    }

    pub async fn multi_get_transaction_block_response_in_blocking_task(
        &self,
        digests: Vec<TransactionDigest>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.multi_get_transaction_block_response_impl(&digests, options)
        })
        .await
    }

    fn get_transaction_events_impl(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<sui_json_rpc_types::SuiEvent>, IndexerError> {
        let (timestamp_ms, serialized_events) = self.run_query(|conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.into_inner().to_vec()))
                .select((transactions::timestamp_ms, transactions::events))
                .first::<(i64, Vec<Option<Vec<u8>>>)>(conn)
        })?;

        let events = serialized_events
            .into_iter()
            .flatten()
            .map(|event| bcs::from_bytes::<sui_types::event::Event>(&event))
            .collect::<Result<Vec<_>, _>>()?;

        events
            .into_iter()
            .enumerate()
            .map(|(i, event)| {
                sui_json_rpc_types::SuiEvent::try_from(
                    event,
                    digest,
                    i as u64,
                    Some(timestamp_ms as u64),
                    self,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub async fn get_transaction_events_in_blocking_task(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<sui_json_rpc_types::SuiEvent>, IndexerError> {
        self.spawn_blocking(move |this| this.get_transaction_events_impl(digest))
            .await
    }

    pub async fn get_dynamic_fields_in_blocking_task(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<DynamicFieldInfo>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_dynamic_fields_impl(parent_object_id, cursor, limit)
        })
        .await
    }

    fn get_dynamic_fields_impl(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<DynamicFieldInfo>, IndexerError> {
        let objects: Vec<StoredObject> = self.run_query(|conn| {
            let mut query = objects::dsl::objects
                .filter(objects::dsl::owner_type.eq(OwnerType::Object as i16))
                .filter(objects::dsl::owner_id.eq(parent_object_id.to_vec()))
                .order(objects::dsl::object_id.asc())
                .limit(limit as i64)
                .into_boxed();
            if let Some(object_cursor) = cursor {
                query = query.filter(objects::dsl::object_id.ge(object_cursor.to_vec()));
            }
            query.load::<StoredObject>(conn)
        })?;

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

        let object_refs = self.get_object_refs(dfo_ids)?;
        let mut dynamic_fields = objects
            .into_iter()
            .map(StoredObject::try_into_expectant_dynamic_field_info)
            .collect::<Result<Vec<_>, _>>()?;

        for mut df in dynamic_fields.iter_mut() {
            if let Some(obj_ref) = object_refs.get(&df.object_id) {
                df.version = obj_ref.1;
                df.digest = obj_ref.2;
            }
        }

        Ok(dynamic_fields)
    }

    fn get_object_refs(
        &self,
        object_ids: Vec<Vec<u8>>,
    ) -> IndexerResult<HashMap<ObjectID, ObjectRef>> {
        self.run_query(|conn| {
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

    fn get_display_update_event(
        &self,
        object_type: String,
    ) -> Result<Option<sui_types::display::DisplayVersionUpdatedEvent>, IndexerError> {
        let stored_display = self.run_query(|conn| {
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
}

#[derive(Clone, Default)]
struct PackageCache {
    inner: Arc<RwLock<BTreeMap<ObjectID, MovePackage>>>,
}

impl PackageCache {
    fn insert(&self, object_id: ObjectID, package: MovePackage) {
        self.inner.write().unwrap().insert(object_id, package);
    }

    fn get(&self, object_id: &ObjectID) -> Option<MovePackage> {
        self.inner.read().unwrap().get(object_id).cloned()
    }
}

impl move_core_types::resolver::ModuleResolver for IndexerReader {
    type Error = IndexerError;

    fn get_module(
        &self,
        id: &move_core_types::language_storage::ModuleId,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let package_id = ObjectID::from(*id.address());
        let module_name = id.name().to_string();
        Ok(self
            .get_package(&package_id)?
            .and_then(|package| package.serialized_module_map().get(&module_name).cloned()))
    }
}

impl sui_types::storage::ObjectStore for IndexerReader {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<sui_types::object::Object>, sui_types::error::SuiError> {
        self.get_object(object_id, None)
            .map_err(|e| sui_types::error::SuiError::GenericStorageError(e.to_string()))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<sui_types::object::Object>, sui_types::error::SuiError> {
        self.get_object(object_id, Some(version))
            .map_err(|e| sui_types::error::SuiError::GenericStorageError(e.to_string()))
    }
}

impl move_bytecode_utils::module_cache::GetModule for IndexerReader {
    type Error = IndexerError;
    type Item = move_binary_format::CompiledModule;

    fn get_module_by_id(
        &self,
        id: &move_core_types::language_storage::ModuleId,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let package_id = ObjectID::from(*id.address());
        let module_name = id.name().to_string();
        // TODO: we need a cache here for deserialized module and take care of package upgrades
        self.get_package(&package_id)?
            .and_then(|package| package.serialized_module_map().get(&module_name).cloned())
            .map(|bytes| move_binary_format::CompiledModule::deserialize_with_defaults(&bytes))
            .transpose()
            .map_err(|e| {
                IndexerError::ModuleResolutionError(format!(
                    "Error deserializing module {}: {}",
                    id, e
                ))
            })
    }
}

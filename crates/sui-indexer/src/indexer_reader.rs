// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use crate::{
    errors::IndexerError,
    models_v2::objects::StoredObject,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, packages::StoredPackage,
        transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, objects, packages, transactions},
    PgConnectionConfig, PgConnectionPoolConfig, PgPoolConnection,
};
use anyhow::{anyhow, Result};
use diesel::{
    r2d2::ConnectionManager, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl,
};
use sui_json_rpc_types::{CheckpointId, EpochInfo};
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    committee::EpochId,
    digests::TransactionDigest,
    move_package::MovePackage,
    object::Object,
    sui_system_state::{sui_system_state_summary::SuiSystemStateSummary, SuiSystemStateTrait},
};

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
        let mut connection = self.get_connection()?;
        connection
            .build_transaction()
            .read_only()
            .run(query)
            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
    }

    pub async fn spawn_blocking<F, R>(&self, f: F) -> Result<R, IndexerError>
    where
        F: FnOnce(Self) -> Result<R, IndexerError> + Send + 'static,
        R: Send + 'static,
    {
        let this = self.clone();
        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _guard = current_span.enter();
            f(this)
        })
        .await
        .map_err(Into::into)
        .and_then(std::convert::identity)
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

    pub async fn get_package_async(
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

    pub fn get_checkpoints_from_db(
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

    pub fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<StoredTransaction>, IndexerError> {
        self.run_query(|conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.into_inner().to_vec()))
                .first::<StoredTransaction>(conn)
                .optional()
        })
    }

    pub fn multi_get_transactions(
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

    pub fn multi_get_transaction_block_response(
        &self,
        digests: &[TransactionDigest],
        options: &sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        self.multi_get_transactions(digests)?
            .into_iter()
            .map(|transaction| transaction.try_into_sui_transaction_block_response(options, self))
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn multi_get_transaction_block_response_async(
        &self,
        digests: Vec<TransactionDigest>,
        options: sui_json_rpc_types::SuiTransactionBlockResponseOptions,
    ) -> Result<Vec<sui_json_rpc_types::SuiTransactionBlockResponse>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.multi_get_transaction_block_response(&digests, &options)
        })
        .await
    }

    pub fn get_transaction_events(
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

    pub async fn get_transaction_events_async(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<sui_json_rpc_types::SuiEvent>, IndexerError> {
        self.spawn_blocking(move |this| this.get_transaction_events(digest))
            .await
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

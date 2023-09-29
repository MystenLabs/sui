// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use crate::{
    errors::IndexerError,
    models_v2::objects::StoredObject,
    models_v2::{epoch::StoredEpochInfo, packages::StoredPackage},
    schema_v2::{epochs, objects, packages},
    PgConectionPoolConfig, PgConnectionConfig, PgPoolConnection,
};
use anyhow::{anyhow, Result};
use diesel::{
    r2d2::ConnectionManager, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl,
};
use sui_json_rpc_types::EpochInfo;
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    committee::EpochId,
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
        let config = PgConectionPoolConfig::default();
        Self::new_with_config(db_url, config)
    }

    pub fn new_with_config<T: Into<String>>(
        db_url: T,
        config: PgConectionPoolConfig,
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
            .map_err(|e| anyhow!("Failed to initialize connection pool with error: {e:?}"))?;

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
                    .limit(1)
                    .first::<StoredEpochInfo>(conn)
                    .optional()
            } else {
                epochs::dsl::epochs
                    .order_by(epochs::epoch.desc())
                    .limit(1)
                    .first::<StoredEpochInfo>(conn)
                    .optional()
            }
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::handlers::tx_processor::IndexingPackageCache;
use crate::metrics::IndexerMetrics;
use crate::schema_v2::packages;
use crate::types_v2::IndexedPackage;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;
use std::sync::{Arc, Mutex};
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;

use crate::errors::{Context, IndexerError};
use crate::models_v2::packages::StoredPackage;
use crate::store::diesel_macro::read_only_blocking;
use crate::PgConnectionPool;

/// A package resolver that reads packages from the database.
pub struct IndexerStoreModuleResolver {
    cp: PgConnectionPool,
}

impl IndexerStoreModuleResolver {
    pub fn new(cp: PgConnectionPool) -> Self {
        Self { cp }
    }
}

impl ModuleResolver for IndexerStoreModuleResolver {
    type Error = IndexerError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let package_id = ObjectID::from(*id.address()).to_vec();
        let module_name = id.name().to_string();

        // Note: this implementation is potentially vulnerable to package upgrade race conditions
        // for framework packages because they reuse the same package IDs.
        let stored_package: StoredPackage = read_only_blocking!(&self.cp, |conn| {
            packages::dsl::packages
                .filter(packages::dsl::package_id.eq(package_id))
                .first::<StoredPackage>(conn)
        })
        .context("Error reading module.")?;

        let move_package =
            bcs::from_bytes::<MovePackage>(&stored_package.move_package).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Error deserializing move package. Error: {}",
                    e
                ))
            })?;

        Ok(move_package
            .serialized_module_map()
            .get(&module_name)
            .cloned())
    }
}

/// InterimModuleResolver consists of a backup ModuleResolver
/// (e.g. IndexerStoreModuleResolver) and an in-mem package cache.
pub struct InterimModuleResolver<GM> {
    backup: GM,
    package_cache: Arc<Mutex<IndexingPackageCache>>,
    metrics: IndexerMetrics,
}

impl<GM> InterimModuleResolver<GM> {
    pub fn new(
        backup: GM,
        package_cache: Arc<Mutex<IndexingPackageCache>>,
        new_packages: &[IndexedPackage],
        metrics: IndexerMetrics,
    ) -> Self {
        package_cache.lock().unwrap().insert_packages(new_packages);
        Self {
            backup,
            package_cache,
            metrics,
        }
    }
}

impl<GM> GetModule for InterimModuleResolver<GM>
where
    GM: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    type Error = IndexerError;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Arc<CompiledModule>>, Self::Error> {
        if let Some(m) = self.package_cache.lock().unwrap().get_module_by_id(id) {
            self.metrics.indexing_module_resolver_in_mem_hit.inc();
            Ok(Some(m))
        } else {
            self.backup
                .get_module_by_id(id)
                .map_err(|e| IndexerError::ModuleResolutionError(e.to_string()))
        }
    }
}

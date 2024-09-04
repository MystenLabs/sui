// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};

use move_core_types::account_address::AccountAddress;
use sui_package_resolver::{error::Error as PackageResolverError, Package, PackageStore};
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::db::ConnectionPool;
use crate::errors::IndexerError;
use crate::handlers::tx_processor::IndexingPackageBuffer;
use crate::metrics::IndexerMetrics;
use crate::schema::objects;
use crate::store::diesel_macro::*;

/// A package resolver that reads packages from the database.
pub struct IndexerStorePackageResolver {
    cp: ConnectionPool,
}

impl Clone for IndexerStorePackageResolver {
    fn clone(&self) -> IndexerStorePackageResolver {
        Self {
            cp: self.cp.clone(),
        }
    }
}

impl IndexerStorePackageResolver {
    pub fn new(cp: ConnectionPool) -> Self {
        Self { cp }
    }
}

#[async_trait]
impl PackageStore for IndexerStorePackageResolver {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>, PackageResolverError> {
        let pkg = self
            .get_package_from_db_in_blocking_task(id)
            .await
            .map_err(|e| PackageResolverError::Store {
                store: "PostgresDB",
                source: Arc::new(e),
            })?;
        Ok(Arc::new(pkg))
    }
}

impl IndexerStorePackageResolver {
    fn get_package_from_db(&self, id: AccountAddress) -> Result<Package, IndexerError> {
        let Some(bcs) = read_only_blocking!(&self.cp, |conn| {
            let query = objects::dsl::objects
                .select(objects::dsl::serialized_object)
                .filter(objects::dsl::object_id.eq(id.to_vec()));
            query.get_result::<Vec<u8>>(conn).optional()
        })?
        else {
            return Err(IndexerError::PostgresReadError(format!(
                "Package not found in DB: {:?}",
                id
            )));
        };
        let object = bcs::from_bytes::<Object>(&bcs)?;
        Package::read_from_object(&object).map_err(|e| {
            IndexerError::PostgresReadError(format!("Failed parsing object to package: {:?}", e))
        })
    }

    async fn get_package_from_db_in_blocking_task(
        &self,
        id: AccountAddress,
    ) -> Result<Package, IndexerError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.get_package_from_db(id)).await?
    }
}

pub struct InterimPackageResolver {
    package_db_resolver: IndexerStorePackageResolver,
    package_buffer: Arc<Mutex<IndexingPackageBuffer>>,
    metrics: IndexerMetrics,
}

impl InterimPackageResolver {
    pub fn new(
        package_db_resolver: IndexerStorePackageResolver,
        package_buffer: Arc<Mutex<IndexingPackageBuffer>>,
        metrics: IndexerMetrics,
    ) -> Self {
        Self {
            package_db_resolver,
            package_buffer,
            metrics,
        }
    }
}

#[async_trait]
impl PackageStore for InterimPackageResolver {
    async fn fetch(&self, addr: AccountAddress) -> Result<Arc<Package>, PackageResolverError> {
        let package_id = ObjectID::from(addr);
        let maybe_obj = {
            let buffer_guard = self.package_buffer.lock().unwrap();
            buffer_guard.get_package(&package_id)
        };
        if let Some(obj) = maybe_obj {
            self.metrics.indexing_package_resolver_in_mem_hit.inc();
            let pkg = Package::read_from_object(&obj).map_err(|e| PackageResolverError::Store {
                store: "InMemoryPackageBuffer",
                source: Arc::new(e),
            })?;
            Ok(Arc::new(pkg))
        } else {
            self.package_db_resolver.fetch(addr).await
        }
    }
}

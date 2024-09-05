// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use crate::database::ConnectionPool;
use crate::handlers::tx_processor::IndexingPackageBuffer;
use crate::metrics::IndexerMetrics;
use crate::schema::objects;
use anyhow::anyhow;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use move_core_types::account_address::AccountAddress;
use sui_package_resolver::{error::Error as PackageResolverError, Package, PackageStore};
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

/// A package resolver that reads packages from the database.
#[derive(Clone)]
pub struct IndexerStorePackageResolver {
    pool: ConnectionPool,
}

impl IndexerStorePackageResolver {
    pub fn new(pool: ConnectionPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PackageStore for IndexerStorePackageResolver {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>, PackageResolverError> {
        let pkg = self
            .get_package_from_db(id)
            .await
            .map_err(|e| PackageResolverError::Store {
                store: "PostgresDB",
                error: e.to_string(),
            })?;
        Ok(Arc::new(pkg))
    }
}

impl IndexerStorePackageResolver {
    async fn get_package_from_db(&self, id: AccountAddress) -> Result<Package, anyhow::Error> {
        let mut connection = self.pool.get().await?;

        let bcs = objects::dsl::objects
            .select(objects::dsl::serialized_object)
            .filter(objects::dsl::object_id.eq(id.to_vec()))
            .get_result::<Vec<u8>>(&mut connection)
            .await
            .map_err(|e| anyhow!("Package not found in DB: {e}"))?;

        let object = bcs::from_bytes::<Object>(&bcs)?;
        Package::read_from_object(&object)
            .map_err(|e| anyhow!("Failed parsing object to package: {e}"))
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
                error: e.to_string(),
            })?;
            Ok(Arc::new(pkg))
        } else {
            self.package_db_resolver.fetch(addr).await
        }
    }
}

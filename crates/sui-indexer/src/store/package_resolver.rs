// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::database::ConnectionPool;
use crate::schema::objects;
use anyhow::anyhow;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use move_core_types::account_address::AccountAddress;
use sui_package_resolver::{error::Error as PackageResolverError, Package, PackageStore};
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

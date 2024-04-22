// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use move_core_types::account_address::AccountAddress;
use sui_indexer::schema::objects;
use sui_package_resolver::Resolver;
use sui_package_resolver::{
    error::Error as PackageResolverError, Package, PackageStore, PackageStoreWithLruCache, Result,
};
use sui_types::object::Object;

use crate::error::Error;

use super::{Db, DbConnection, QueryExecutor};

const STORE: &str = "PostgresDB";

pub(crate) type PackageCache = PackageStoreWithLruCache<DbPackageStore>;
pub(crate) type PackageResolver = Arc<Resolver<PackageCache>>;

/// Store which fetches package for the given address from the backend db on every call
/// to `fetch`
pub struct DbPackageStore(Db);

impl DbPackageStore {
    pub fn new(db: Db) -> Self {
        Self(db)
    }
}

#[async_trait]
impl PackageStore for DbPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let Self(db) = self;
        let bcs: Option<Vec<u8>> = db
            .execute(move |conn| {
                conn.result(move || {
                    objects::dsl::objects
                        .select(objects::dsl::serialized_object)
                        .filter(objects::dsl::object_id.eq(id.to_vec()))
                })
                .optional()
            })
            .await?;

        if let Some(bcs) = bcs {
            let object = bcs::from_bytes::<Object>(&bcs)?;
            Ok(Arc::new(Package::read(&object)?))
        } else {
            Err(PackageResolverError::PackageNotFound(id))
        }
    }
}

impl From<Error> for PackageResolverError {
    fn from(source: Error) -> Self {
        Self::Store {
            store: STORE,
            source: Box::new(source),
        }
    }
}

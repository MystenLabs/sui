// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::{DataLoader, Loader};
use diesel::{
    prelude::QueryableByName,
    sql_types::{Array, Bytea},
};
use move_core_types::account_address::AccountAddress;
use sui_indexer_alt_schema::schema::kv_packages;
use sui_package_resolver::{error::Error, Package, PackageStore, PackageStoreWithLruCache, Result};
use sui_types::object::Object;

use crate::pg_reader::PgReader;

const STORE: &str = "PostgreSQL";

pub type PackageCache = PackageStoreWithLruCache<DbPackageStore>;
pub struct DbPackageStore(Arc<DataLoader<PgReader>>);

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct PackageKey(AccountAddress);

impl DbPackageStore {
    pub fn new(loader: Arc<DataLoader<PgReader>>) -> Self {
        Self(loader)
    }
}

#[async_trait::async_trait]
impl PackageStore for DbPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let Self(loader) = self;
        let Some(package) = loader.load_one(PackageKey(id)).await? else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(package)
    }
}

#[async_trait::async_trait]
impl Loader<PackageKey> for PgReader {
    type Value = Arc<Package>;
    type Error = Error;

    async fn load(&self, keys: &[PackageKey]) -> Result<HashMap<PackageKey, Arc<Package>>> {
        let mut id_to_package = HashMap::new();
        if keys.is_empty() {
            return Ok(id_to_package);
        }

        let mut conn = self.connect().await.map_err(|e| Error::Store {
            store: STORE,
            error: e.to_string(),
        })?;

        #[derive(QueryableByName)]
        #[diesel(table_name = kv_packages)]
        struct SerializedPackage {
            serialized_object: Vec<u8>,
        }

        let ids: Vec<_> = keys.iter().map(|PackageKey(id)| id.to_vec()).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    v.serialized_object
                FROM (
                    SELECT UNNEST($1) package_id
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        serialized_object
                    FROM
                        kv_packages
                    WHERE
                        kv_packages.package_id = k.package_id
                    ORDER BY
                        package_version DESC
                    LIMIT 1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids);

        let stored_packages: Vec<SerializedPackage> =
            conn.results(query).await.map_err(|e| Error::Store {
                store: STORE,
                error: e.to_string(),
            })?;

        for stored in stored_packages {
            let object: Object = bcs::from_bytes(&stored.serialized_object)?;
            let Some(move_package) = object.data.try_as_package() else {
                return Err(Error::NotAPackage(object.id().into()));
            };

            let package = Package::read_from_package(move_package)?;
            id_to_package.insert(PackageKey(*move_package.id()), Arc::new(package));
        }

        Ok(id_to_package)
    }
}

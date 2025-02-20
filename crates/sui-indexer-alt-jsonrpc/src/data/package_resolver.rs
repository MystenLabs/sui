// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use async_graphql::dataloader::{DataLoader, Loader};
use diesel::{ExpressionMethods, QueryDsl};
use move_core_types::account_address::AccountAddress;
use sui_indexer_alt_schema::{packages::StoredPackage, schema::sum_packages};
use sui_package_resolver::{
    error::Error, Package, PackageStore, PackageStoreWithLruCache, Resolver, Result,
};

use super::reader::Reader;

const STORE: &str = "PostgreSQL";

pub(crate) type PackageCache = PackageStoreWithLruCache<DbPackageStore>;
pub(crate) type PackageResolver = Arc<Resolver<PackageCache>>;
pub(crate) struct DbPackageStore(Arc<DataLoader<Reader>>);

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct PackageKey(AccountAddress);

impl DbPackageStore {
    pub fn new(loader: Arc<DataLoader<Reader>>) -> Self {
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
impl Loader<PackageKey> for Reader {
    type Value = Arc<Package>;
    type Error = Error;

    async fn load(&self, keys: &[PackageKey]) -> Result<HashMap<PackageKey, Arc<Package>>> {
        use sum_packages::dsl as p;

        let mut id_to_package = HashMap::new();
        if keys.is_empty() {
            return Ok(id_to_package);
        }

        let mut conn = self.connect().await.map_err(|e| Error::Store {
            store: STORE,
            error: e.to_string(),
        })?;

        let ids: BTreeSet<_> = keys.iter().map(|PackageKey(id)| id.to_vec()).collect();
        let stored_packages: Vec<StoredPackage> = conn
            .results(p::sum_packages.filter(p::package_id.eq_any(ids)))
            .await
            .map_err(|e| Error::Store {
                store: STORE,
                error: e.to_string(),
            })?;

        for stored_package in stored_packages {
            let move_package = bcs::from_bytes(&stored_package.move_package)?;
            let package = Package::read_from_package(&move_package)?;
            id_to_package.insert(PackageKey(*move_package.id()), Arc::new(package));
        }

        Ok(id_to_package)
    }
}

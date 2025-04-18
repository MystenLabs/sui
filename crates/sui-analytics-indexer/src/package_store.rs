// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
#[cfg(not(test))]
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use sui_package_resolver::{
    error::Error as PackageResolverError, Package, PackageStore, PackageStoreWithLruCache, Result,
};
use sui_rpc_api::Client;
use sui_types::base_types::ObjectID;
#[cfg(not(test))]
use sui_types::object::Data;
use sui_types::object::Object;
use thiserror::Error;
#[cfg(not(test))]
use tokio::sync::RwLock;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::DBMapUtils;
use typed_store::{Map, TypedStoreError};

const STORE: &str = "RocksDB";

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    TypedStore(#[from] TypedStoreError),
}

impl From<Error> for PackageResolverError {
    fn from(source: Error) -> Self {
        match source {
            Error::TypedStore(store_error) => Self::Store {
                store: STORE,
                error: store_error.to_string(),
            },
        }
    }
}

#[derive(DBMapUtils)]
pub struct PackageStoreTables {
    pub(crate) packages: DBMap<ObjectID, Object>,
}

impl PackageStoreTables {
    pub fn new(path: &Path) -> Arc<Self> {
        // Create a custom RocksDB options with controlled memory usage
        use typed_store::rocksdb::{BlockBasedOptions, Options};
        let mut opts = Options::default();

        // Configure block cache options
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_block_size(16 * 1024); // 16KB blocks
        opts.set_block_based_table_factory(&block_opts);

        // Allocate about 3GB to RocksDB total memory budget
        opts.increase_parallelism(4); // Helps with CPU utilization

        // Configure memory usage
        opts.set_max_total_wal_size(256 * 1024 * 1024); // 256MB WAL size
        opts.set_db_write_buffer_size(256 * 1024 * 1024); // 256MB write buffer
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB per memtable
        opts.set_max_write_buffer_number(4); // Limit number of memtables
        opts.set_target_file_size_base(128 * 1024 * 1024); // 128MB target file size

        // Create the DB with controlled memory options
        Arc::new(Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::new("package"),
            Some(opts),
            None,
        ))
    }

    pub(crate) fn update(&self, package: &Object) -> Result<()> {
        let mut batch = self.packages.batch();
        batch
            .insert_batch(&self.packages, std::iter::once((package.id(), package)))
            .map_err(Error::TypedStore)?;
        batch.write().map_err(Error::TypedStore)?;
        Ok(())
    }
}

/// Store which keeps package objects in a local rocksdb store. It is expected that this store is
/// kept updated with latest version of package objects while iterating over checkpoints. If the
/// local db is missing (or gets deleted), packages are fetched from a full node and local store is
/// updated
#[derive(Clone)]
pub struct LocalDBPackageStore {
    package_store_tables: Arc<PackageStoreTables>,
    fallback_client: Client,
    #[cfg(not(test))]
    original_id_cache: Arc<RwLock<HashMap<AccountAddress, ObjectID>>>,
}

impl LocalDBPackageStore {
    pub fn new(path: &Path, rest_url: &str) -> Self {
        Self {
            package_store_tables: PackageStoreTables::new(path),
            fallback_client: Client::new(rest_url).unwrap(),
            #[cfg(not(test))]
            original_id_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn update(&self, object: &Object) -> Result<()> {
        let Some(_package) = object.data.try_as_package() else {
            return Ok(());
        };
        self.package_store_tables.update(object)?;
        Ok(())
    }

    pub async fn get(&self, id: AccountAddress) -> Result<Object> {
        let object = if let Some(object) = self
            .package_store_tables
            .packages
            .get(&ObjectID::from(id))
            .map_err(Error::TypedStore)?
        {
            object
        } else {
            let object = self
                .fallback_client
                .get_object(ObjectID::from(id))
                .await
                .map_err(|_| PackageResolverError::PackageNotFound(id))?;
            self.update(&object)?;
            object
        };
        Ok(object)
    }

    /// Gets the original package id for the given package id.
    #[cfg(not(test))]
    pub async fn get_original_package_id(&self, id: AccountAddress) -> Result<ObjectID> {
        if let Some(&original_id) = self.original_id_cache.read().await.get(&id) {
            return Ok(original_id);
        }

        let object = self.get(id).await?;
        let Data::Package(package) = &object.data else {
            return Err(PackageResolverError::PackageNotFound(id));
        };

        let original_id = package.original_package_id();

        self.original_id_cache.write().await.insert(id, original_id);

        Ok(original_id)
    }

    #[cfg(test)]
    pub async fn get_original_package_id(&self, id: AccountAddress) -> Result<ObjectID> {
        Ok(id.into())
    }
}

#[async_trait]
impl PackageStore for LocalDBPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let object = self.get(id).await?;
        Ok(Arc::new(Package::read_from_object(&object)?))
    }
}

pub(crate) type PackageCache = PackageStoreWithLruCache<LocalDBPackageStore>;

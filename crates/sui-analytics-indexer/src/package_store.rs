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
use tracing::info;
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
        use typed_store::rocksdb::{BlockBasedOptions, Cache, Options};
        let mut opts = Options::default();

        // Create a shared LRU cache for blocks, indices, and filters - explicit size cap
        let lru = Cache::new_lru_cache(512 * 1024 * 1024);

        // Configure block cache options with tight controls
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_cache(&lru); // Use shared explicit cache
        block_opts.set_cache_index_and_filter_blocks(true); // Cache indices in block cache
        block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true); // Keeps them out of RSS
        block_opts.set_block_size(16 * 1024); // 16KB blocks
        opts.set_block_based_table_factory(&block_opts);

        // Allocate about 3GB to RocksDB total memory budget
        opts.increase_parallelism(4); // Helps with CPU utilization

        // Configure memory usage - much more conservative settings
        opts.set_max_total_wal_size(64 * 1024 * 1024); // 64MB WAL size
        opts.set_db_write_buffer_size(128 * 1024 * 1024); // 128MB write buffer
        opts.set_write_buffer_size(32 * 1024 * 1024); // 32MB per memtable
        opts.set_max_write_buffer_number(2); // Limit number of memtables
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB target file size

        // Additional memory settings
        opts.set_max_background_jobs(2); // Limit background jobs
        opts.set_max_open_files(100); // Limit open files
        opts.set_keep_log_file_num(1); // Keep fewer log files

        // Prevent L0 growth explosion which pins memory
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_level_zero_slowdown_writes_trigger(8);
        opts.set_level_zero_stop_writes_trigger(12);

        // Compaction settings to reduce memory usage
        opts.set_max_bytes_for_level_base(64 * 1024 * 1024); // 64MB for base level
        opts.set_max_bytes_for_level_multiplier(8.0); // Less aggressive level sizing
        opts.set_max_subcompactions(1); // Prevent excessive compaction memory
        opts.set_compaction_readahead_size(8 * 1024 * 1024); // 8MB compaction read buffer

        opts.set_allow_mmap_reads(false);
        opts.set_allow_mmap_writes(false);

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

    pub fn log_stats(&self) {
        use typed_store::rocksdb::properties::*;

        match &self.packages.db.storage {
            typed_store::rocks::Storage::Rocks(db) => {
                let db = &db.underlying;

                let props = [
                    (STATS, "RocksDB stats"),
                    (DBSTATS, "DB stats"),
                    (LEVELSTATS, "Level stats"),
                    (CUR_SIZE_ALL_MEM_TABLES, "Cur size of all memtables"),
                    (SIZE_ALL_MEM_TABLES, "Total memtable size"),
                    (BLOCK_CACHE_USAGE, "Block cache usage"),
                    (TOTAL_SST_FILES_SIZE, "Total SST file size"),
                    (ESTIMATE_NUM_KEYS, "Estimated number of keys"),
                    (ESTIMATE_LIVE_DATA_SIZE, "Estimated live data size"),
                    (ESTIMATE_TABLE_READERS_MEM, "Estimated table readers memory"),
                    (NUM_ENTRIES_ACTIVE_MEM_TABLE, "Entries in active memtable"),
                    (NUM_ENTRIES_IMM_MEM_TABLES, "Entries in immutable memtables"),
                    (NUM_DELETES_ACTIVE_MEM_TABLE, "Deletes in active memtable"),
                    (NUM_DELETES_IMM_MEM_TABLES, "Deletes in immutable memtables"),
                    (
                        NUM_IMMUTABLE_MEM_TABLE,
                        "Number of immutable memtables (not flushed)",
                    ),
                    (
                        NUM_IMMUTABLE_MEM_TABLE_FLUSHED,
                        "Number of flushed immutable memtables",
                    ),
                    (MEM_TABLE_FLUSH_PENDING, "Memtable flush pending"),
                    (NUM_RUNNING_FLUSHES, "Running flushes"),
                    (COMPACTION_PENDING, "Compaction pending"),
                    (NUM_RUNNING_COMPACTIONS, "Running compactions"),
                ];

                info!(target: "package_store", "========== RocksDB Stats Dump ==========");
                for (key, label) in props {
                    match db.property_value(key) {
                        Ok(value) => {
                            info!(target: "package_store", "{:?}:\n{:?}", label, value);
                        }
                        Err(e) => {
                            info!(target: "package_store", "{}: <error: {}>", label, e);
                        }
                    }
                }
                info!(target: "package_store", "========== End of Stats ================");
            }
            _ => {
                info!(target: "package_store", "log_stats(): Not a RocksDB-backed store");
            }
        }
    }
}

/// Store which keeps package objects in a local rocksdb store. It is expected that this store is
/// kept updated with latest version of package objects while iterating over checkpoints. If the
/// local db is missing (or gets deleted), packages are fetched from a full node and local store is
/// updated
#[derive(Clone)]
pub struct LocalDBPackageStore {
    pub package_store_tables: Arc<PackageStoreTables>,
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use cache_coordinator::CacheReadyCoordinator;
use lru::LruCache;
use move_core_types::account_address::AccountAddress;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::Path;
use std::result::Result as StdResult;
use std::sync::Arc;
use std::sync::Mutex;
use sui_package_resolver::{
    error::Error as ResolverError, Package, PackageStore, PackageStoreWithLruCache, Resolver,
    Result,
};
use sui_rpc_api::Client;
use sui_types::{
    base_types::ObjectID,
    object::{Data, Object},
    SYSTEM_PACKAGE_ADDRESSES,
};
use thiserror::Error;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::{DBMapUtils, Map, TypedStoreError};

pub mod cache_coordinator;
pub mod package_cache_worker;

use std::sync::OnceLock;

/// A lazy-initialized package cache that tracks whether it was ever accessed.
pub struct LazyPackageCache {
    cache: Option<OnceLock<Arc<PackageCache>>>,
    constructor: Box<dyn Fn() -> Arc<PackageCache> + Send + Sync>,
}

impl LazyPackageCache {
    pub fn new(path: std::path::PathBuf, rest_url: String) -> Self {
        let constructor = Box::new(move || Arc::new(PackageCache::new(&path, &rest_url)));

        Self {
            cache: None,
            constructor,
        }
    }

    /// Initialize the cache if needed and return the package cache.
    pub fn initialize_or_get_cache(&mut self) -> Arc<PackageCache> {
        if self.cache.is_none() {
            self.cache = Some(OnceLock::new());
        }

        self.cache
            .as_ref()
            .unwrap()
            .get_or_init(|| (self.constructor)())
            .clone()
    }

    /// Get the package cache if it was initialized, None otherwise.
    pub fn get_cache_if_initialized(&self) -> Option<Arc<PackageCache>> {
        self.cache.as_ref().and_then(|cell| cell.get().cloned())
    }
}

const STORE: &str = "RocksDB";
const MAX_EPOCH_CACHES: usize = 2; // keep at most two epochs in memory

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    TypedStore(#[from] TypedStoreError),
    #[error("Package not found: {0}")]
    PackageNotFound(AccountAddress),
}

impl From<Error> for ResolverError {
    fn from(e: Error) -> Self {
        ResolverError::Store {
            store: STORE,
            error: e.to_string(),
        }
    }
}

#[derive(DBMapUtils)]
pub struct PackageStoreTables {
    pub(crate) packages: DBMap<ObjectID, Object>,
}

impl PackageStoreTables {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::new("package"),
            None,
            None,
        ))
    }

    fn update(&self, object: &Object) -> StdResult<(), Error> {
        self.update_batch(std::iter::once(object))
    }

    fn update_batch<'a, I>(&self, objects: I) -> StdResult<(), Error>
    where
        I: IntoIterator<Item = &'a Object>,
    {
        let mut batch = self.packages.batch();
        batch.insert_batch(&self.packages, objects.into_iter().map(|o| (o.id(), o)))?;

        batch.write()?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct LocalDBPackageStore {
    tables: Arc<PackageStoreTables>,
    client: Client,
}

impl LocalDBPackageStore {
    pub fn new(path: &Path, rest_url: &str) -> Self {
        Self {
            tables: PackageStoreTables::new(path),
            client: Client::new(rest_url).expect("invalid REST URL"),
        }
    }

    fn update(&self, object: &Object) -> StdResult<(), Error> {
        if object.data.try_as_package().is_some() {
            self.tables.update(object)?;
        }
        Ok(())
    }

    fn update_batch<'a, I>(&self, objects: I) -> StdResult<(), Error>
    where
        I: IntoIterator<Item = &'a Object>,
    {
        let filtered = objects
            .into_iter()
            .filter(|o| o.data.try_as_package().is_some());

        self.tables.update_batch(filtered)?;
        Ok(())
    }

    async fn get(&self, id: AccountAddress) -> StdResult<Object, Error> {
        if let Some(o) = self.tables.packages.get(&ObjectID::from(id))? {
            return Ok(o);
        }
        let o = self
            .client
            .clone()
            .get_object(ObjectID::from(id))
            .await
            .map_err(|_| Error::PackageNotFound(id))?;
        self.update(&o)?;
        Ok(o)
    }

    pub async fn get_original_package_id(&self, id: AccountAddress) -> StdResult<ObjectID, Error> {
        let o = self.get(id).await?;
        let Data::Package(p) = &o.data else {
            return Err(Error::TypedStore(TypedStoreError::SerializationError(
                "not a package".into(),
            )));
        };
        Ok(p.original_package_id())
    }
}

#[async_trait]
impl PackageStore for LocalDBPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let o = self.get(id).await?;
        Ok(Arc::new(Package::read_from_object(&o)?))
    }
}

// A thin new‑type wrapper so we can hand an `Arc` to `Resolver`
#[derive(Clone)]
pub struct GlobalArcStore(pub Arc<PackageStoreWithLruCache<LocalDBPackageStore>>);

#[async_trait]
impl PackageStore for GlobalArcStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        self.0.fetch(id).await
    }
}

impl Deref for GlobalArcStore {
    type Target = PackageStoreWithLruCache<LocalDBPackageStore>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Multi-level cache. System packages can change across epochs while non-system packages are
// immutable and can be cached across epochs. This impl assumes the system is at most working on
// 2 epochs at a time (at the epoch boundary). When the indexer begins processing a new epoch it
// will create a new PackageStoreWithLruCache for that epoch and the oldest epoch in the cache
// will be dropped.
#[derive(Clone)]
pub struct CompositeStore {
    pub epoch: u64,
    pub global: Arc<PackageStoreWithLruCache<LocalDBPackageStore>>,
    pub base: LocalDBPackageStore,
    pub epochs: Arc<Mutex<LruCache<u64, Arc<PackageStoreWithLruCache<LocalDBPackageStore>>>>>,
}

impl CompositeStore {
    /// Lazily obtain (or create) the cache for the current epoch.
    fn epoch_cache(&self) -> Arc<PackageStoreWithLruCache<LocalDBPackageStore>> {
        let mut caches = self.epochs.lock().unwrap();
        if let Some(cache) = caches.get(&self.epoch) {
            return cache.clone();
        }
        // Not present — create a fresh cache backed by the same LocalDB store.
        let cache = Arc::new(PackageStoreWithLruCache::new(self.base.clone()));
        caches.put(self.epoch, cache.clone());
        cache
    }
}

#[async_trait]
impl PackageStore for CompositeStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        if SYSTEM_PACKAGE_ADDRESSES.contains(&id) {
            let cache = self.epoch_cache();
            return cache.fetch(id).await;
        }
        self.global.fetch(id).await
    }
}

// Top‑level cache façade
pub struct PackageCache {
    pub base_store: LocalDBPackageStore,
    pub global_cache: Arc<PackageStoreWithLruCache<LocalDBPackageStore>>,
    pub epochs: Arc<Mutex<LruCache<u64, Arc<PackageStoreWithLruCache<LocalDBPackageStore>>>>>,
    pub resolver: Resolver<GlobalArcStore>,
    pub coordinator: CacheReadyCoordinator,
}

impl PackageCache {
    pub fn new(path: &Path, rest_url: &str) -> Self {
        let base_store = LocalDBPackageStore::new(path, rest_url);
        let global_cache = Arc::new(PackageStoreWithLruCache::new(base_store.clone()));
        Self {
            resolver: Resolver::new(GlobalArcStore(global_cache.clone())),
            base_store,
            global_cache,
            epochs: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_EPOCH_CACHES).unwrap(),
            ))),
            coordinator: CacheReadyCoordinator::new(),
        }
    }

    pub fn resolver_for_epoch(&self, epoch: u64) -> Resolver<CompositeStore> {
        Resolver::new(CompositeStore {
            epoch,
            global: self.global_cache.clone(),
            base: self.base_store.clone(),
            epochs: self.epochs.clone(),
        })
    }

    pub fn update(&self, object: &Object) -> Result<()> {
        self.base_store.update(object)?;
        Ok(())
    }

    fn update_batch<'a, I>(&self, objects: I) -> Result<()>
    where
        I: IntoIterator<Item = &'a Object>,
    {
        self.base_store.update_batch(objects)?;
        Ok(())
    }

    #[cfg(not(test))]
    pub async fn get_original_package_id(&self, id: AccountAddress) -> Result<ObjectID> {
        Ok(self.base_store.get_original_package_id(id).await?)
    }

    #[cfg(test)]
    pub async fn get_original_package_id(&self, id: AccountAddress) -> Result<ObjectID> {
        Ok(id.into())
    }
}

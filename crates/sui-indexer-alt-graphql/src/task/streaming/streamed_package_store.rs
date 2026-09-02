// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::account_address::AccountAddress;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Result;

use crate::task::watermark::KV_PACKAGES_PIPELINE;

use super::streamed_cache_eviction::EvictableCache;
use super::streamed_store::StreamedStore;

/// Package store for streaming subscriptions that holds packages not yet indexed by the DB.
///
/// Packages from streamed checkpoints are indexed here. Once the `kv_packages` pipeline catches up,
/// the eviction task removes them, at which point the inner store (LRU → PackageCache → DB) serves
/// them instead. Each entry records the checkpoint that introduced it, so eviction of an older
/// checkpoint does not remove a system package that was upgraded at a later checkpoint.
pub(crate) struct StreamedPackageStore<S> {
    /// Packages from streamed checkpoints not yet in the DB.
    cache: StreamedStore<AccountAddress, Arc<Package>>,

    /// Fallback store (typically the shared PackageCache → DB).
    inner: S,
}

impl<S> StreamedPackageStore<S> {
    pub(crate) fn new(inner: S) -> Self {
        Self {
            cache: StreamedStore::new(),
            inner,
        }
    }

    /// Index packages from a streamed checkpoint. Called by the checkpoint stream task. Checkpoints
    /// are processed in order, so the latest insert for a package ID is the newest version.
    pub(crate) fn index_packages(&self, checkpoint_seq: u64, packages: &[Arc<Package>]) {
        for package in packages {
            self.cache
                .insert(checkpoint_seq, package.storage_id(), package.clone());
        }
    }
}

#[async_trait::async_trait]
impl<S: PackageStore> PackageStore for StreamedPackageStore<S> {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        if let Some(package) = self.cache.get(&id) {
            return Ok(package);
        }

        self.inner.fetch(id).await
    }
}

impl<S: Send + Sync> EvictableCache for StreamedPackageStore<S> {
    fn watermark_pipeline(&self) -> &'static str {
        KV_PACKAGES_PIPELINE
    }

    fn evict_up_to(&self, indexed_checkpoint: u64) {
        self.cache.evict_up_to(indexed_checkpoint);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use sui_package_resolver::error::Error as PackageResolverError;
    use sui_types::base_types::SequenceNumber;

    use super::super::streamed_cache_eviction::EvictableCache;
    use super::*;

    struct MockStore {
        packages: Mutex<HashMap<AccountAddress, Arc<Package>>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                packages: Mutex::new(HashMap::new()),
            }
        }

        fn with(id: AccountAddress, package: Arc<Package>) -> Self {
            let store = Self::new();
            store.packages.lock().unwrap().insert(id, package);
            store
        }
    }

    #[async_trait::async_trait]
    impl PackageStore for MockStore {
        async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
            self.packages
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .ok_or(PackageResolverError::PackageNotFound(id))
        }
    }

    fn addr(n: u8) -> AccountAddress {
        let mut bytes = [0u8; AccountAddress::LENGTH];
        bytes[AccountAddress::LENGTH - 1] = n;
        AccountAddress::new(bytes)
    }

    fn pkg(id: AccountAddress, version: u64) -> Arc<Package> {
        Arc::new(Package::for_test(id, SequenceNumber::from_u64(version)))
    }

    #[tokio::test]
    async fn fetch_hits_primary_index() {
        let store = StreamedPackageStore::new(MockStore::new());
        let p = pkg(addr(1), 1);
        store.index_packages(5, std::slice::from_ref(&p));

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
    }

    #[tokio::test]
    async fn fetch_falls_through_to_inner() {
        let p = pkg(addr(1), 1);
        let store = StreamedPackageStore::new(MockStore::with(addr(1), p.clone()));

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
    }

    #[tokio::test]
    async fn fetch_primary_takes_precedence_over_inner() {
        let primary = pkg(addr(1), 2);
        let store = StreamedPackageStore::new(MockStore::with(addr(1), pkg(addr(1), 1)));
        store.index_packages(5, std::slice::from_ref(&primary));

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &primary));
    }

    #[tokio::test]
    async fn evict_up_to_removes_indexed_checkpoint() {
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1)]);

        store.evict_up_to(5);

        assert!(store.fetch(addr(1)).await.is_err());
    }

    #[tokio::test]
    async fn evict_up_to_keeps_later_system_package_upgrade() {
        // A system package upgrade re-indexes the same ID at a later checkpoint; evicting up to the
        // older checkpoint must keep the newer entry.
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1)]);
        let upgraded = pkg(addr(1), 2);
        store.index_packages(10, std::slice::from_ref(&upgraded));

        store.evict_up_to(5);

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &upgraded));
    }

    #[tokio::test]
    async fn evict_up_to_handles_multiple_packages() {
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1), pkg(addr(2), 1), pkg(addr(3), 1)]);

        store.evict_up_to(5);

        assert!(store.fetch(addr(1)).await.is_err());
        assert!(store.fetch(addr(2)).await.is_err());
        assert!(store.fetch(addr(3)).await.is_err());
    }
}

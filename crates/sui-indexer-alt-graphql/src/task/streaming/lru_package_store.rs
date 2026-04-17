// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use moka::sync::Cache;
use move_core_types::account_address::AccountAddress;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Result;

/// Bounded concurrent cache that sits between the streaming index and the database.
///
/// On fetch, checks the local cache first. On miss, delegates to the inner store and
/// promotes the result into the cache for subsequent requests. Eviction from this layer
/// is harmless — the inner store (PackageCache → DB) always has the data.
///
/// We use `moka::sync::Cache` over `Mutex<LruCache>` for lock-free concurrent reads.
/// moka is eventually consistent: concurrent misses may race (all delegate to the inner
/// store and all insert) and LRU ordering is approximate. Both are acceptable here —
/// duplicate work is rare and LRU precision isn't critical since eviction only drops
/// packages that the DB can still serve.
pub(crate) struct LruPackageStore<S> {
    cache: Cache<AccountAddress, Arc<Package>>,
    inner: S,
}

impl<S> LruPackageStore<S> {
    pub(crate) fn new(inner: S, capacity: u64) -> Self {
        Self {
            cache: Cache::builder().max_capacity(capacity).build(),
            inner,
        }
    }
}

#[async_trait::async_trait]
impl<S: PackageStore> PackageStore for LruPackageStore<S> {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        if let Some(package) = self.cache.get(&id) {
            return Ok(package);
        }

        let package = self.inner.fetch(id).await?;
        self.cache.insert(id, package.clone());
        Ok(package)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use sui_package_resolver::error::Error as PackageResolverError;
    use sui_types::base_types::SequenceNumber;

    use super::*;

    /// Counting mock that tracks how many times `fetch` was called.
    struct CountingStore {
        packages: Mutex<HashMap<AccountAddress, Arc<Package>>>,
        fetches: AtomicUsize,
    }

    impl CountingStore {
        fn new() -> Self {
            Self {
                packages: Mutex::new(HashMap::new()),
                fetches: AtomicUsize::new(0),
            }
        }

        fn with(id: AccountAddress, package: Arc<Package>) -> Self {
            let store = Self::new();
            store.packages.lock().unwrap().insert(id, package);
            store
        }

        fn fetch_count(&self) -> usize {
            self.fetches.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl PackageStore for CountingStore {
        async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
            self.fetches.fetch_add(1, Ordering::SeqCst);
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
    async fn miss_delegates_to_inner_and_caches() {
        let p = pkg(addr(1), 1);
        let store = LruPackageStore::new(CountingStore::with(addr(1), p.clone()), 16);

        // First fetch: cache miss, delegates to inner.
        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
        assert_eq!(store.inner.fetch_count(), 1);

        // moka inserts are eventually consistent; force pending writes to apply.
        store.cache.run_pending_tasks();

        // Second fetch: cache hit, inner not called again.
        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
        assert_eq!(store.inner.fetch_count(), 1);
    }

    #[tokio::test]
    async fn inner_error_propagates() {
        let store = LruPackageStore::new(CountingStore::new(), 16);

        assert!(store.fetch(addr(1)).await.is_err());
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dashmap::DashMap;
use move_core_types::account_address::AccountAddress;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Result;

/// Package store for streaming subscriptions that holds packages not yet indexed by the DB.
///
/// Packages from streamed checkpoints are indexed here. Once the `kv_packages` pipeline
/// catches up, a separate eviction task removes them — at that point the inner store
/// (LRU → PackageCache → DB) can serve them instead.
///
/// Each package entry stores the checkpoint that introduced it, so that eviction of an
/// older checkpoint does not accidentally remove a system package that was upgraded at a
/// later checkpoint.
pub(crate) struct StreamedPackageStore<S> {
    /// Primary index: packages from streamed checkpoints not yet in the DB.
    packages: DashMap<AccountAddress, IndexedPackage>,

    /// Fallback store (typically the shared PackageCache → DB).
    inner: S,
}

struct IndexedPackage {
    checkpoint: u64,
    package: Arc<Package>,
}

impl<S> StreamedPackageStore<S> {
    pub(crate) fn new(inner: S) -> Self {
        Self {
            packages: DashMap::new(),
            inner,
        }
    }

    /// Index packages from a streamed checkpoint. Called by the checkpoint stream task.
    ///
    /// Checkpoints are processed sequentially, so the latest insert for a given package
    /// ID is always the newest version.
    pub(crate) fn index_packages(&self, checkpoint_seq: u64, packages: &[Arc<Package>]) {
        for package in packages {
            self.packages.insert(
                package.storage_id(),
                IndexedPackage {
                    checkpoint: checkpoint_seq,
                    package: package.clone(),
                },
            );
        }
    }

    /// Remove packages that were introduced at `checkpoint_seq` from the primary index.
    ///
    /// Uses `DashMap::remove_if` for atomic checked removal: a package is only removed
    /// if its stored checkpoint still matches. This handles system package upgrades where
    /// the same ID is re-inserted at a later checkpoint.
    pub(crate) fn evict_checkpoint(&self, checkpoint_seq: u64, package_ids: &[AccountAddress]) {
        for id in package_ids {
            self.packages
                .remove_if(id, |_, entry| entry.checkpoint == checkpoint_seq);
        }
    }
}

#[async_trait::async_trait]
impl<S: PackageStore> PackageStore for StreamedPackageStore<S> {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        if let Some(entry) = self.packages.get(&id) {
            return Ok(entry.package.clone());
        }

        self.inner.fetch(id).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use sui_package_resolver::error::Error as PackageResolverError;
    use sui_types::base_types::SequenceNumber;

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
    async fn evict_removes_matching_checkpoint() {
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1)]);

        store.evict_checkpoint(5, &[addr(1)]);

        assert!(store.fetch(addr(1)).await.is_err());
    }

    #[tokio::test]
    async fn evict_skips_mismatched_checkpoint() {
        // Simulates a system package upgrade: same ID indexed twice at different
        // checkpoints. Evicting the older checkpoint should NOT remove the newer entry.
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1)]);
        let upgraded = pkg(addr(1), 2);
        store.index_packages(10, std::slice::from_ref(&upgraded));

        store.evict_checkpoint(5, &[addr(1)]);

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &upgraded));
    }

    #[tokio::test]
    async fn evict_handles_multiple_packages() {
        let store = StreamedPackageStore::new(MockStore::new());
        store.index_packages(5, &[pkg(addr(1), 1), pkg(addr(2), 1), pkg(addr(3), 1)]);

        store.evict_checkpoint(5, &[addr(1), addr(2), addr(3)]);

        assert!(store.fetch(addr(1)).await.is_err());
        assert!(store.fetch(addr(2)).await.is_err());
        assert!(store.fetch(addr(3)).await.is_err());
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;

use lru::LruCache;
use move_core_types::account_address::AccountAddress;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Result;

/// A layered package store for streaming subscriptions.
///
/// Maintains a dedicated LRU cache of packages seen in streamed checkpoints,
/// isolated from the query path's cache. On fetch, checks the streaming cache
/// first before falling through to the inner store (typically the shared
/// PackageCache backed by the database).
///
/// This prevents query traffic from evicting packages needed by streaming
/// subscribers, while still falling back to the database for packages not
/// recently seen in the stream (e.g., system packages from genesis).
pub(crate) struct StreamingPackageStore<S> {
    packages: Mutex<LruCache<AccountAddress, Arc<Package>>>,
    inner: Arc<S>,
}

impl<S> StreamingPackageStore<S> {
    pub(crate) fn new(inner: Arc<S>, capacity: NonZeroUsize) -> Self {
        Self {
            packages: Mutex::new(LruCache::new(capacity)),
            inner,
        }
    }

    /// Insert a package into the streaming cache. Checkpoints are processed sequentially,
    /// so the latest insert is always the newest version.
    pub(crate) fn insert(&self, id: AccountAddress, package: Arc<Package>) {
        self.packages.lock().unwrap().push(id, package);
    }
}

#[async_trait::async_trait]
impl<S: PackageStore> PackageStore for StreamingPackageStore<S> {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        // Check streaming cache first.
        if let Some(package) = self.packages.lock().unwrap().get(&id).cloned() {
            return Ok(package);
        }

        // Fall through to inner store (PackageCache → DB).
        self.inner.fetch(id).await
    }
}

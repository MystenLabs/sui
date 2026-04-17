// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use move_core_types::account_address::AccountAddress;
use sui_futures::service::Service;
use tracing::debug;

use crate::task::watermark::KV_PACKAGES_PIPELINE;
use crate::task::watermark::Pipeline;
use crate::task::watermark::WatermarksLock;

use super::StreamedPackageStore;

/// Queue that records each streamed checkpoint's package IDs for eventual eviction.
/// The checkpoint stream task records entries via `record_checkpoint_packages_mapping`;
/// the eviction task drains entries via `pop_if_below` once the `kv_packages` watermark
/// catches up.
pub(crate) struct EvictionQueue {
    inner: Mutex<VecDeque<(u64, Vec<AccountAddress>)>>,
}

impl EvictionQueue {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(VecDeque::new()),
        })
    }

    /// Record the set of packages introduced at `checkpoint_seq` so they can be evicted
    /// once the `kv_packages` indexer has processed them.
    pub(crate) fn record_checkpoint_packages_mapping(
        &self,
        checkpoint_seq: u64,
        package_ids: Vec<AccountAddress>,
    ) {
        self.inner
            .lock()
            .unwrap()
            .push_back((checkpoint_seq, package_ids));
    }

    /// Pop the front entry if its checkpoint is at or below `watermark`. Returns `None`
    /// when the queue is empty or the front entry is above the watermark.
    fn pop_if_below(&self, watermark: u64) -> Option<(u64, Vec<AccountAddress>)> {
        let mut q = self.inner.lock().unwrap();
        match q.front() {
            Some((cp_seq, _)) if *cp_seq <= watermark => q.pop_front(),
            _ => None,
        }
    }
}

/// Background task that evicts packages from the `StreamedPackageStore` once the
/// `kv_packages` pipeline has indexed the checkpoint that introduced them.
pub(crate) struct PackageEvictionTask<S> {
    streaming_packages: Arc<StreamedPackageStore<S>>,
    queue: Arc<EvictionQueue>,
    watermarks: WatermarksLock,
    eviction_interval: Duration,
}

impl<S: Send + Sync + 'static> PackageEvictionTask<S> {
    pub(crate) fn new(
        streaming_packages: Arc<StreamedPackageStore<S>>,
        queue: Arc<EvictionQueue>,
        watermarks: WatermarksLock,
        eviction_interval: Duration,
    ) -> Self {
        Self {
            streaming_packages,
            queue,
            watermarks,
            eviction_interval,
        }
    }

    pub(crate) fn run(self) -> Service {
        let Self {
            streaming_packages,
            queue,
            watermarks,
            eviction_interval,
        } = self;

        Service::new().spawn_aborting(async move {
            let mut interval = tokio::time::interval(eviction_interval);

            loop {
                interval.tick().await;

                let Some(kv_packages_hi) = kv_packages_watermark(&watermarks).await else {
                    continue;
                };

                while let Some((cp_seq, package_ids)) = queue.pop_if_below(kv_packages_hi) {
                    streaming_packages.evict_checkpoint(cp_seq, &package_ids);
                    debug!(
                        checkpoint = cp_seq,
                        "Evicted {} packages",
                        package_ids.len()
                    );
                }
            }
        })
    }
}

/// Read the current `kv_packages` high watermark from the shared watermarks.
/// Returns `None` if the pipeline is not being tracked.
async fn kv_packages_watermark(watermarks: &WatermarksLock) -> Option<u64> {
    let watermarks = watermarks.read().await;
    watermarks
        .per_pipeline()
        .get(KV_PACKAGES_PIPELINE)
        .map(|p: &Pipeline| p.hi().checkpoint())
}

#[cfg(test)]
mod tests {
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Result as PackageResult;
    use sui_package_resolver::error::Error as PackageResolverError;
    use sui_types::base_types::SequenceNumber;

    use super::*;
    use crate::task::watermark::Watermarks;

    struct MockStore;

    #[async_trait::async_trait]
    impl sui_package_resolver::PackageStore for MockStore {
        async fn fetch(
            &self,
            id: AccountAddress,
        ) -> PackageResult<Arc<sui_package_resolver::Package>> {
            Err(PackageResolverError::PackageNotFound(id))
        }
    }

    fn addr(n: u8) -> AccountAddress {
        let mut bytes = [0u8; AccountAddress::LENGTH];
        bytes[AccountAddress::LENGTH - 1] = n;
        AccountAddress::new(bytes)
    }

    fn pkg(id: AccountAddress, version: u64) -> Arc<sui_package_resolver::Package> {
        Arc::new(sui_package_resolver::Package::for_test(
            id,
            SequenceNumber::from_u64(version),
        ))
    }

    async fn set_kv_packages_hi(watermarks: &WatermarksLock, hi: u64) {
        *watermarks.write().await = Arc::new(Watermarks::for_test(&[("kv_packages", hi)]));
    }

    #[tokio::test(start_paused = true)]
    async fn evicts_entries_at_or_below_watermark() {
        let store = Arc::new(StreamedPackageStore::new(MockStore));
        let queue = EvictionQueue::new();
        let watermarks: WatermarksLock = Default::default();

        store.index_packages(5, &[pkg(addr(1), 1)]);
        queue.record_checkpoint_packages_mapping(5, vec![addr(1)]);
        set_kv_packages_hi(&watermarks, 10).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            queue.clone(),
            watermarks.clone(),
            Duration::from_secs(5),
        )
        .run();

        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;

        assert!(store.fetch(addr(1)).await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_entries_above_watermark() {
        let store = Arc::new(StreamedPackageStore::new(MockStore));
        let queue = EvictionQueue::new();
        let watermarks: WatermarksLock = Default::default();

        let p = pkg(addr(1), 1);
        store.index_packages(10, std::slice::from_ref(&p));
        queue.record_checkpoint_packages_mapping(10, vec![addr(1)]);
        set_kv_packages_hi(&watermarks, 5).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            queue.clone(),
            watermarks.clone(),
            Duration::from_secs(5),
        )
        .run();

        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
    }

    #[tokio::test(start_paused = true)]
    async fn skips_eviction_when_kv_packages_untracked() {
        let store = Arc::new(StreamedPackageStore::new(MockStore));
        let queue = EvictionQueue::new();
        let watermarks: WatermarksLock = Default::default();

        let p = pkg(addr(1), 1);
        store.index_packages(5, std::slice::from_ref(&p));
        queue.record_checkpoint_packages_mapping(5, vec![addr(1)]);

        let _service = PackageEvictionTask::new(
            store.clone(),
            queue.clone(),
            watermarks.clone(),
            Duration::from_secs(5),
        )
        .run();

        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;

        assert!(Arc::ptr_eq(&store.fetch(addr(1)).await.unwrap(), &p));
    }

    #[tokio::test(start_paused = true)]
    async fn drains_below_watermark_keeps_above() {
        let store = Arc::new(StreamedPackageStore::new(MockStore));
        let queue = EvictionQueue::new();
        let watermarks: WatermarksLock = Default::default();

        for cp in [3u64, 5, 7] {
            store.index_packages(cp, &[pkg(addr(cp as u8), 1)]);
            queue.record_checkpoint_packages_mapping(cp, vec![addr(cp as u8)]);
        }
        let p10 = pkg(addr(10), 1);
        store.index_packages(10, std::slice::from_ref(&p10));
        queue.record_checkpoint_packages_mapping(10, vec![addr(10)]);

        set_kv_packages_hi(&watermarks, 8).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            queue.clone(),
            watermarks.clone(),
            Duration::from_secs(5),
        )
        .run();

        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;

        assert!(store.fetch(addr(3)).await.is_err());
        assert!(store.fetch(addr(5)).await.is_err());
        assert!(store.fetch(addr(7)).await.is_err());
        assert!(Arc::ptr_eq(&store.fetch(addr(10)).await.unwrap(), &p10));
    }
}

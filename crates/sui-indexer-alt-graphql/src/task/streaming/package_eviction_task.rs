// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::StreamExt;
use move_core_types::account_address::AccountAddress;
use sui_futures::service::Service;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::debug;

use crate::task::watermark::KV_PACKAGES_PIPELINE;
use crate::task::watermark::Pipeline;
use crate::task::watermark::WatermarksLock;

use super::StreamedPackageStore;

/// Background task that evicts packages from the `StreamedPackageStore` once the
/// `kv_packages` pipeline has indexed the checkpoint that introduced them.
///
/// The stream task sends `(checkpoint_seq, package_ids)` entries to this task over
/// an unbounded mpsc channel. On each timer tick, the task drains all entries whose
/// checkpoint is at or below the current `kv_packages` watermark.
pub(crate) struct PackageEvictionTask<S> {
    streaming_packages: Arc<StreamedPackageStore<S>>,
    receiver: UnboundedReceiver<(u64, Vec<AccountAddress>)>,
    watermarks: WatermarksLock,
    eviction_interval: Duration,
}

impl<S: Send + Sync + 'static> PackageEvictionTask<S> {
    pub(crate) fn new(
        streaming_packages: Arc<StreamedPackageStore<S>>,
        receiver: UnboundedReceiver<(u64, Vec<AccountAddress>)>,
        watermarks: WatermarksLock,
        eviction_interval: Duration,
    ) -> Self {
        Self {
            streaming_packages,
            receiver,
            watermarks,
            eviction_interval,
        }
    }

    pub(crate) fn run(self) -> Service {
        let Self {
            streaming_packages,
            receiver,
            watermarks,
            eviction_interval,
        } = self;

        Service::new().spawn_aborting(async move {
            let mut stream = Box::pin(UnboundedReceiverStream::new(receiver).peekable());
            let mut interval = tokio::time::interval(eviction_interval);

            loop {
                interval.tick().await;

                let Some(kv_packages_hi) = kv_packages_watermark(&watermarks).await else {
                    continue;
                };

                // Drain entries at or below the watermark. `peek().now_or_never()`
                // gives us a non-blocking peek, so we can stop immediately when the
                // channel is empty or the next entry is above the watermark.
                loop {
                    let peeked_cp = stream
                        .as_mut()
                        .peek()
                        .now_or_never()
                        .map(|opt| opt.map(|(cp, _)| *cp));

                    match peeked_cp {
                        None => break,
                        Some(None) => return Ok(()),
                        Some(Some(cp)) if cp > kv_packages_hi => break,
                        Some(Some(_)) => {
                            let (cp_seq, package_ids) = stream
                                .next()
                                .await
                                .expect("peek returned Some; next must yield same item");
                            streaming_packages.evict_checkpoint(cp_seq, &package_ids);
                            debug!(
                                checkpoint = cp_seq,
                                "Evicted {} packages",
                                package_ids.len()
                            );
                        }
                    }
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
#[allow(clippy::disallowed_methods)]
mod tests {
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Result as PackageResult;
    use sui_package_resolver::error::Error as PackageResolverError;
    use sui_types::base_types::SequenceNumber;
    use tokio::sync::mpsc::unbounded_channel;

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
        let (tx, rx) = unbounded_channel();
        let watermarks: WatermarksLock = Default::default();

        store.index_packages(5, &[pkg(addr(1), 1)]);
        tx.send((5, vec![addr(1)])).unwrap();
        set_kv_packages_hi(&watermarks, 10).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            rx,
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
        let (tx, rx) = unbounded_channel();
        let watermarks: WatermarksLock = Default::default();

        let p = pkg(addr(1), 1);
        store.index_packages(10, std::slice::from_ref(&p));
        tx.send((10, vec![addr(1)])).unwrap();
        set_kv_packages_hi(&watermarks, 5).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            rx,
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
        let (tx, rx) = unbounded_channel();
        let watermarks: WatermarksLock = Default::default();

        let p = pkg(addr(1), 1);
        store.index_packages(5, std::slice::from_ref(&p));
        tx.send((5, vec![addr(1)])).unwrap();

        let _service = PackageEvictionTask::new(
            store.clone(),
            rx,
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
        let (tx, rx) = unbounded_channel();
        let watermarks: WatermarksLock = Default::default();

        for cp in [3u64, 5, 7] {
            store.index_packages(cp, &[pkg(addr(cp as u8), 1)]);
            tx.send((cp, vec![addr(cp as u8)])).unwrap();
        }
        let p10 = pkg(addr(10), 1);
        store.index_packages(10, std::slice::from_ref(&p10));
        tx.send((10, vec![addr(10)])).unwrap();

        set_kv_packages_hi(&watermarks, 8).await;

        let _service = PackageEvictionTask::new(
            store.clone(),
            rx,
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

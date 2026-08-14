// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use sui_futures::service::Service;
use tracing::debug;

use crate::task::watermark::Pipeline;
use crate::task::watermark::WatermarksLock;

/// A streamed cache the eviction task can flush without knowing its concrete key/value types. Each
/// cache reports the watermark pipeline that tracks the index behind it, so a single task can flush
/// them all once each index catches up.
pub(crate) trait EvictableCache: Send + Sync {
    /// The watermark pipeline whose progress means the durable index now holds this checkpoint, so
    /// entries at or below it are safe to drop.
    fn watermark_pipeline(&self) -> &'static str;

    /// Drop entries introduced at a checkpoint at or below `indexed_checkpoint`.
    fn evict_up_to(&self, indexed_checkpoint: u64);
}

/// Background task that flushes every registered [`EvictableCache`]. On each tick it drops, from each
/// cache, the entries whose checkpoint is at or below that cache's watermark pipeline.
pub(crate) struct StreamedCacheEvictionTask {
    caches: Vec<Arc<dyn EvictableCache>>,
    watermarks: WatermarksLock,
    eviction_interval: Duration,
}

impl StreamedCacheEvictionTask {
    pub(crate) fn new(
        caches: Vec<Arc<dyn EvictableCache>>,
        watermarks: WatermarksLock,
        eviction_interval: Duration,
    ) -> Self {
        Self {
            caches,
            watermarks,
            eviction_interval,
        }
    }

    pub(crate) fn run(self) -> Service {
        let Self {
            caches,
            watermarks,
            eviction_interval,
        } = self;

        Service::new().spawn_aborting(async move {
            let mut interval = tokio::time::interval(eviction_interval);
            loop {
                interval.tick().await;

                let watermarks = watermarks.read().await;
                for cache in &caches {
                    let Some(indexed_hi) = watermarks
                        .per_pipeline()
                        .get(cache.watermark_pipeline())
                        .map(|p: &Pipeline| p.hi().checkpoint())
                    else {
                        continue;
                    };
                    cache.evict_up_to(indexed_hi);
                    debug!(
                        pipeline = cache.watermark_pipeline(),
                        checkpoint = indexed_hi,
                        "Evicted streamed cache up to checkpoint"
                    );
                }
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::task::watermark::Watermarks;

    /// Records the checkpoint it was last evicted up to, so tests can assert the task's behaviour.
    struct MockCache {
        pipeline: &'static str,
        evicted_up_to: Mutex<Option<u64>>,
    }

    impl MockCache {
        fn new(pipeline: &'static str) -> Arc<Self> {
            Arc::new(Self {
                pipeline,
                evicted_up_to: Mutex::new(None),
            })
        }

        fn evicted_up_to(&self) -> Option<u64> {
            *self.evicted_up_to.lock().unwrap()
        }
    }

    impl EvictableCache for MockCache {
        fn watermark_pipeline(&self) -> &'static str {
            self.pipeline
        }

        fn evict_up_to(&self, indexed_checkpoint: u64) {
            *self.evicted_up_to.lock().unwrap() = Some(indexed_checkpoint);
        }
    }

    async fn set_watermarks(watermarks: &WatermarksLock, entries: &[(&'static str, u64)]) {
        *watermarks.write().await = Arc::new(Watermarks::for_test(entries));
    }

    async fn run_one_tick(caches: Vec<Arc<dyn EvictableCache>>, watermarks: WatermarksLock) {
        let _service =
            StreamedCacheEvictionTask::new(caches, watermarks, Duration::from_secs(5)).run();
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn evicts_up_to_its_pipeline_watermark() {
        let cache = MockCache::new("ledger_grpc");
        let watermarks: WatermarksLock = Default::default();
        set_watermarks(&watermarks, &[("ledger_grpc", 5)]).await;

        run_one_tick(vec![cache.clone()], watermarks).await;

        assert_eq!(cache.evicted_up_to(), Some(5));
    }

    #[tokio::test(start_paused = true)]
    async fn skips_cache_whose_pipeline_is_untracked() {
        let cache = MockCache::new("ledger_grpc");
        let watermarks: WatermarksLock = Default::default();
        set_watermarks(&watermarks, &[("kv_packages", 100)]).await;

        run_one_tick(vec![cache.clone()], watermarks).await;

        assert_eq!(cache.evicted_up_to(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn flushes_each_cache_by_its_own_pipeline() {
        let packages = MockCache::new("kv_packages");
        let transactions = MockCache::new("ledger_grpc");
        let watermarks: WatermarksLock = Default::default();
        set_watermarks(&watermarks, &[("kv_packages", 4), ("ledger_grpc", 2)]).await;

        run_one_tick(vec![packages.clone(), transactions.clone()], watermarks).await;

        assert_eq!(packages.evicted_up_to(), Some(4));
        assert_eq!(transactions.evicted_up_to(), Some(2));
    }
}

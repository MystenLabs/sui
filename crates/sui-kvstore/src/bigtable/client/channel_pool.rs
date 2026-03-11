// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use anyhow::Result;
use arc_swap::ArcSwap;
use http::{Request, Response};
use rand::Rng;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};

const MAX_REPLACEMENTS_PER_CYCLE: usize = 2;

pub(crate) struct PoolConfig {
    pub initial_pool_size: usize,
    pub min_pool_size: usize,
    pub max_pool_size: usize,
    pub min_rpcs_per_channel: usize,
    pub max_rpcs_per_channel: usize,
    pub max_resize_delta: usize,
    pub downscale_threshold: usize,
    pub maintenance_interval: Duration,
    pub refresh_age: Duration,
    pub refresh_jitter: Duration,
}

pub(crate) struct ChannelPool {
    inner: Arc<ChannelPoolInner>,
    // Stores the entry and a *clone* of its channel after poll_ready. We need the clone
    // because Channel::poll_ready takes &mut self, and entry.channel isn't mutable through Arc.
    reserved: Option<(Arc<PoolEntry>, Channel)>,
}

pub(crate) struct ChannelPoolInner {
    entries: ArcSwap<Vec<Arc<PoolEntry>>>,
    ticker: AtomicUsize,
    consecutive_low_load: AtomicUsize,
    endpoint: Endpoint,
    config: PoolConfig,
    primer: Option<Box<dyn ChannelPrimer>>,
    metrics: Option<Arc<dyn PoolMetrics>>,
}

// Entries are wrapped in Arc and held by both the pool's entry list and any in-flight
// RPCs (via Service::call). When an entry is replaced or removed from the pool, in-flight
// RPCs keep their Arc<PoolEntry> alive until the response completes — no explicit drain needed.
struct PoolEntry {
    channel: Channel,
    refresh_at: Instant,
    in_flight: AtomicUsize,
    // Per-channel counters reset each maintenance cycle. Error count triggers a warn log;
    // success count is useful for verifying round-robin distribution in tests.
    // Neither counter influences selection or replacement decisions.
    success_count: AtomicUsize,
    error_count: AtomicUsize,
}

pub(crate) trait ChannelPrimer: Send + Sync + 'static {
    fn prime<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

pub trait PoolMetrics: Send + Sync + 'static {
    fn set_pool_size(&self, size: usize);
    fn channels_replaced(&self, count: usize);
    fn rpc_completed(&self);
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_pool_size: 10,
            min_pool_size: 1,
            max_pool_size: 200,
            min_rpcs_per_channel: 5,
            max_rpcs_per_channel: 50,
            max_resize_delta: 2,
            downscale_threshold: 3,
            maintenance_interval: Duration::from_secs(60),
            // GFE forcibly disconnects channels after ~60 minutes for load balancing.
            // Refresh at 45m + up to 5m jitter to replace channels before reaping.
            refresh_age: Duration::from_secs(45 * 60),
            refresh_jitter: Duration::from_secs(5 * 60),
        }
    }
}

impl ChannelPool {
    /// Create a pool without connecting or spawning background tasks.
    pub(crate) fn new(
        endpoint: Endpoint,
        config: PoolConfig,
        primer: Option<Box<dyn ChannelPrimer>>,
        metrics: Option<Arc<dyn PoolMetrics>>,
    ) -> Self {
        Self {
            inner: Arc::new(ChannelPoolInner {
                entries: ArcSwap::from_pointee(Vec::new()),
                ticker: AtomicUsize::new(0),
                consecutive_low_load: AtomicUsize::new(0),
                endpoint,
                config,
                primer,
                metrics,
            }),
            reserved: None,
        }
    }

    /// Connect `initial_pool_size` channels, priming each one.
    pub(crate) async fn connect(&self) -> Result<()> {
        let mut entries = Vec::with_capacity(self.inner.config.initial_pool_size);
        for _ in 0..self.inner.config.initial_pool_size {
            entries.push(Arc::new(
                create_primed_entry(
                    &self.inner.endpoint,
                    self.inner.primer.as_deref(),
                    self.inner.config.refresh_age,
                    self.inner.config.refresh_jitter,
                )
                .await?,
            ));
        }
        if let Some(m) = &self.inner.metrics {
            m.set_pool_size(entries.len());
        }
        self.inner.entries.store(Arc::new(entries));
        Ok(())
    }

    /// Spawn the background maintenance loop.
    pub(crate) fn spawn_background_tasks(&self) {
        self.spawn_maintenance();
    }

    /// Convenience: create, connect, and spawn background tasks.
    pub(crate) async fn new_connected(
        endpoint: Endpoint,
        config: PoolConfig,
        primer: Option<Box<dyn ChannelPrimer>>,
        metrics: Option<Arc<dyn PoolMetrics>>,
    ) -> Result<Self> {
        let pool = Self::new(endpoint, config, primer, metrics);
        pool.connect().await?;
        pool.spawn_background_tasks();
        Ok(pool)
    }

    fn spawn_maintenance(&self) {
        let weak = Arc::downgrade(&self.inner);
        let interval = self.inner.config.maintenance_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                inner.run_maintenance().await;
            }
        });
    }
}

impl ChannelPoolInner {
    async fn run_maintenance(&self) {
        self.refresh().await;
        self.resize().await;
    }

    async fn refresh(&self) {
        let snapshot = self.entries.load();
        let mut new_vec: Vec<Arc<PoolEntry>> = (**snapshot).clone();
        let now = Instant::now();
        let mut replacements = 0;

        for (idx, entry) in snapshot.iter().enumerate() {
            let successes = entry.success_count.swap(0, Ordering::Relaxed);
            let errors = entry.error_count.swap(0, Ordering::Relaxed);
            if errors > 0 {
                warn!(
                    channel_idx = idx,
                    errors, successes, "channel errors since last maintenance cycle"
                );
            }

            if replacements >= MAX_REPLACEMENTS_PER_CYCLE {
                continue;
            }

            if now < entry.refresh_at {
                continue;
            }

            match self.create_primed_entry().await {
                Ok(new_entry) => {
                    info!(channel_idx = idx, "replacing bigtable channel");
                    new_vec[idx] = Arc::new(new_entry);
                    replacements += 1;
                }
                Err(e) => {
                    warn!(channel_idx = idx, error = %e, "failed to create replacement channel");
                }
            }
        }

        if replacements > 0 {
            self.entries.store(Arc::new(new_vec));
            if let Some(m) = &self.metrics {
                m.channels_replaced(replacements);
            }
        }
    }

    async fn resize(&self) {
        let entries = self.entries.load();
        let current_size = entries.len();
        if current_size == 0 {
            return;
        }

        let total_in_flight: usize = entries
            .iter()
            .map(|e| e.in_flight.load(Ordering::Relaxed))
            .sum();

        let load = total_in_flight as f64 / current_size as f64;
        let target_load = (self.config.min_rpcs_per_channel as f64
            + self.config.max_rpcs_per_channel as f64)
            / 2.0;

        if load <= self.config.min_rpcs_per_channel as f64 {
            // Scale down: load is too low. Require consecutive low-load observations
            // before actually shrinking, to avoid flapping (mirrors official Google Go
            // client continuousDownscaleRuns logic).
            let runs = self.consecutive_low_load.fetch_add(1, Ordering::Relaxed) + 1;
            if runs < self.config.downscale_threshold {
                return;
            }

            // Ideal pool size to bring per-channel load back to target_load.
            // We don't jump straight to this size — delta is capped by max_resize_delta
            // to avoid removing too many channels at once and causing a load spike.
            let pool_size_target = (total_in_flight as f64 / target_load.max(1.0)).ceil() as usize;
            let pool_size_target = pool_size_target.max(self.config.min_pool_size);
            let delta =
                (current_size.saturating_sub(pool_size_target)).min(self.config.max_resize_delta);
            if delta == 0 {
                self.consecutive_low_load.store(0, Ordering::Relaxed);
                return;
            }

            let mut current = (**self.entries.load()).clone();
            let old_size = current.len();
            let new_len = current
                .len()
                .saturating_sub(delta)
                .max(self.config.min_pool_size);
            current.truncate(new_len);
            let new_size = current.len();
            info!(old_size, new_size, total_in_flight, "pool resize: shrunk");
            self.entries.store(Arc::new(current));
            self.consecutive_low_load.store(0, Ordering::Relaxed);
            if let Some(m) = &self.metrics {
                m.set_pool_size(new_size);
            }
        } else {
            // Load is above the low threshold — reset the downscale streak.
            self.consecutive_low_load.store(0, Ordering::Relaxed);

            if load >= self.config.max_rpcs_per_channel as f64 {
                // Scale up: load is too high.
                let pool_size_target = ((total_in_flight as f64 / target_load.max(1.0)).ceil()
                    as usize)
                    .min(self.config.max_pool_size);
                let delta = pool_size_target.saturating_sub(current_size);
                if delta == 0 {
                    return;
                }

                let mut current = (**self.entries.load()).clone();
                let old_size = current.len();
                for _ in 0..delta {
                    match self.create_primed_entry().await {
                        Ok(entry) => current.push(Arc::new(entry)),
                        Err(e) => {
                            warn!(error = %e, "failed to create channel during pool expansion");
                            break;
                        }
                    }
                }
                let new_size = current.len();
                info!(old_size, new_size, total_in_flight, "pool resize: expanded");
                self.entries.store(Arc::new(current));
                if let Some(m) = &self.metrics {
                    m.set_pool_size(new_size);
                }
            }
        }
    }

    async fn create_primed_entry(&self) -> Result<PoolEntry> {
        create_primed_entry(
            &self.endpoint,
            self.primer.as_deref(),
            self.config.refresh_age,
            self.config.refresh_jitter,
        )
        .await
    }
}

impl Clone for ChannelPool {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            reserved: None,
        }
    }
}

impl Drop for ChannelPool {
    fn drop(&mut self) {
        if let Some((entry, _)) = self.reserved.take() {
            entry.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Service<Request<Body>> for ChannelPool {
    type Response = Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Some((_, ref mut channel)) = self.reserved {
            return channel.poll_ready(cx).map_err(Into::into);
        }

        let entries = self.inner.entries.load();
        let idx = self.inner.ticker.fetch_add(1, Ordering::Relaxed) % entries.len();
        let entry = &entries[idx];
        let mut channel = entry.channel.clone();
        entry.in_flight.fetch_add(1, Ordering::Relaxed);
        match channel.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                self.reserved = Some((entry.clone(), channel));
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                self.reserved = Some((entry.clone(), channel));
                Poll::Pending
            }
            Poll::Ready(Err(e)) => {
                entry.in_flight.fetch_sub(1, Ordering::Relaxed);
                entry.error_count.fetch_add(1, Ordering::Relaxed);
                Poll::Ready(Err(e.into()))
            }
        }
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let (entry, mut channel) = self.reserved.take().expect("called before poll_ready");
        let metrics = self.inner.metrics.clone();

        Box::pin(async move {
            let result = channel.call(request).await;
            entry.in_flight.fetch_sub(1, Ordering::Relaxed);
            if let Some(m) = &metrics {
                m.rpc_completed();
            }
            match &result {
                Ok(_) => {
                    entry.success_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    entry.error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(result?)
        })
    }
}

fn compute_refresh_at(refresh_age: Duration, refresh_jitter: Duration) -> Instant {
    let jitter = if refresh_jitter.is_zero() {
        Duration::ZERO
    } else {
        Duration::from_millis(rand::thread_rng().gen_range(0..refresh_jitter.as_millis() as u64))
    };
    Instant::now() + refresh_age + jitter
}

async fn create_primed_entry(
    endpoint: &Endpoint,
    primer: Option<&dyn ChannelPrimer>,
    refresh_age: Duration,
    refresh_jitter: Duration,
) -> Result<PoolEntry> {
    let channel = endpoint.connect().await?;
    if let Some(primer) = primer
        && let Err(e) = primer.prime(&channel).await
    {
        warn!(error = %e, "channel priming failed (non-fatal)");
    }
    Ok(PoolEntry {
        channel,
        refresh_at: compute_refresh_at(refresh_age, refresh_jitter),
        in_flight: AtomicUsize::new(0),
        success_count: AtomicUsize::new(0),
        error_count: AtomicUsize::new(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::proto::bigtable::v2::PingAndWarmRequest;
    use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;

    struct TestPoolMetrics {
        pool_size: AtomicUsize,
        channels_replaced: AtomicUsize,
        rpcs_completed: AtomicUsize,
    }

    impl TestPoolMetrics {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                pool_size: AtomicUsize::new(0),
                channels_replaced: AtomicUsize::new(0),
                rpcs_completed: AtomicUsize::new(0),
            })
        }
    }

    impl PoolMetrics for TestPoolMetrics {
        fn set_pool_size(&self, size: usize) {
            self.pool_size.store(size, Ordering::Relaxed);
        }
        fn channels_replaced(&self, count: usize) {
            self.channels_replaced.fetch_add(count, Ordering::Relaxed);
        }
        fn rpc_completed(&self) {
            self.rpcs_completed.fetch_add(1, Ordering::Relaxed);
        }
    }

    struct TestPrimer;

    impl ChannelPrimer for TestPrimer {
        fn prime<'a>(
            &'a self,
            channel: &'a Channel,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                let mut client = BigtableInternalClient::new(channel.clone());
                client
                    .ping_and_warm(PingAndWarmRequest {
                        name: String::new(),
                        app_profile_id: String::new(),
                    })
                    .await?;
                Ok(())
            })
        }
    }

    async fn start_mock_server() -> (MockBigtableServer, Endpoint, tokio::task::JoinHandle<()>) {
        let mock = MockBigtableServer::new();
        let (addr, handle) = mock.start().await.unwrap();
        let endpoint = Channel::from_shared(format!("http://{addr}")).unwrap();
        (mock, endpoint, handle)
    }

    #[tokio::test]
    async fn test_pool_creation() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(metrics.clone()));
        pool.connect().await.unwrap();

        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 4);
    }

    #[tokio::test]
    async fn test_pool_creation_with_primer() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        let config = PoolConfig {
            initial_pool_size: 3,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(
            endpoint,
            config,
            Some(Box::new(TestPrimer)),
            Some(metrics.clone()),
        );
        pool.connect().await.unwrap();

        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_round_robin_rpcs() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        let config = PoolConfig {
            initial_pool_size: 2,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(
            endpoint,
            config,
            Some(Box::new(TestPrimer)),
            Some(metrics.clone()),
        );
        pool.connect().await.unwrap();

        // Send RPCs through the pool via PingAndWarm.
        let n = 6;
        for _ in 0..n {
            let mut client = BigtableInternalClient::new(pool.clone());
            client
                .ping_and_warm(PingAndWarmRequest {
                    name: String::new(),
                    app_profile_id: String::new(),
                })
                .await
                .unwrap();
        }

        assert_eq!(metrics.rpcs_completed.load(Ordering::Relaxed), n);
        // Mock counts primer pings (2 from connect) + our 6 RPCs.
        assert_eq!(mock.request_count.load(Ordering::Relaxed), 2 + n);

        // Verify round-robin: 6 RPCs split evenly across 2 channels.
        let entries = pool.inner.entries.load();
        assert_eq!(entries[0].success_count.load(Ordering::Relaxed), 3);
        assert_eq!(entries[1].success_count.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_dynamic_resize() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        // target_load = (5 + 50) / 2 = 27
        let config = PoolConfig {
            initial_pool_size: 1,
            min_pool_size: 1,
            max_pool_size: 10,
            max_resize_delta: 2,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(metrics.clone()));
        pool.connect().await.unwrap();
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 1);

        let set_total_in_flight = |n: usize| {
            let entries = pool.inner.entries.load();
            // Put all load on entry[0], zero the rest.
            for (i, e) in entries.iter().enumerate() {
                e.in_flight
                    .store(if i == 0 { n } else { 0 }, Ordering::Relaxed);
            }
        };

        // Scale up: 1 channel with 100 in-flight → load=100 ≥ 50.
        // target = ceil(100/27) = 4, delta = 3 (uncapped). Pool: 1 → 4.
        set_total_in_flight(100);
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 4);

        // Scale up again: load=200/4=50 ≥ 50.
        // target = ceil(200/27) = 8, delta = 4. Pool: 4 → 8.
        set_total_in_flight(200);
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 8);

        // Remove load. Scale down requires downscale_threshold (default 3) consecutive
        // low-load observations before actually shrinking, then is capped by max_resize_delta=2.
        set_total_in_flight(0);

        // First two low-load observations: no shrink yet (threshold not met).
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 8);
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 8);

        // Third consecutive low-load observation crosses the threshold.
        // load=0 ≤ 5 → target=max(ceil(0/27),1)=1, delta=min(8-1,2)=2. Pool: 8 → 6.
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 6);

        // Counter was reset after scaling. Need 3 more consecutive observations.
        // 6 → 4
        pool.inner.resize().await;
        pool.inner.resize().await;
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 4);

        // 4 → 2
        pool.inner.resize().await;
        pool.inner.resize().await;
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 2);

        // 2 → 1 (min_pool_size)
        pool.inner.resize().await;
        pool.inner.resize().await;
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 1);

        // Already at min — no change even after threshold.
        pool.inner.resize().await;
        pool.inner.resize().await;
        pool.inner.resize().await;
        assert_eq!(metrics.pool_size.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_age_refresh_replaces_old_channels() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        let config = PoolConfig {
            initial_pool_size: 2,
            min_pool_size: 1,
            max_pool_size: 10,
            refresh_age: Duration::from_millis(0),
            refresh_jitter: Duration::ZERO,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(metrics.clone()));
        pool.connect().await.unwrap();

        pool.inner.refresh().await;

        // Both entries expired, both replaced (within MAX_REPLACEMENTS_PER_CYCLE=2).
        assert_eq!(metrics.channels_replaced.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_age_refresh_caps_replacements_per_cycle() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let metrics = TestPoolMetrics::new();

        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 1,
            max_pool_size: 10,
            refresh_age: Duration::from_millis(0),
            refresh_jitter: Duration::ZERO,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(metrics.clone()));
        pool.connect().await.unwrap();

        // All 4 entries are expired, but only MAX_REPLACEMENTS_PER_CYCLE=2 replaced per cycle.
        pool.inner.refresh().await;
        assert_eq!(metrics.channels_replaced.load(Ordering::Relaxed), 2);

        // Second cycle replaces the remaining 2.
        pool.inner.refresh().await;
        assert_eq!(metrics.channels_replaced.load(Ordering::Relaxed), 4);
    }
}

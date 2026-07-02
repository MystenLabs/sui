// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Dedicated, fixed-size OS-thread pool for the RocksDB reads backing the
//! v2alpha `List*` gRPC streams.
//!
//! These chunk reads used to run on `tokio::task::spawn_blocking`, which shares
//! one global `Mutex<Shared>` across the whole blocking pool. Under load that
//! mutex becomes the bottleneck (core workers park in `lock_slow`). This pool
//! owns a single MPMC bounded queue, pre-spawns a fixed set of worker threads,
//! and sheds with `ResourceExhausted` once the queue is full, keeping the read
//! path off tokio's global blocking mutex and giving the operator explicit
//! admission control plus per-op latency/shedding metrics.

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Sender, TrySendError, bounded};
use prometheus::{
    HistogramVec, IntCounterVec, IntGauge, Registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_with_registry,
};

use crate::RpcError;
use crate::metrics::LATENCY_SEC_BUCKETS;

/// Prometheus metrics for the RPC RocksDB read pool. The `op` label is a
/// low-cardinality `&'static str` from the fixed set
/// `{"list_checkpoints", "list_transactions", "list_events"}`.
pub(crate) struct ReadPoolMetrics {
    queue_wait_seconds: HistogramVec,
    work_seconds: HistogramVec,
    submitted_total: IntCounterVec,
    abandoned_total: IntCounterVec,
    queue_depth: IntGauge,
    active_threads: IntGauge,
    size: IntGauge,
}

impl ReadPoolMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            queue_wait_seconds: register_histogram_vec_with_registry!(
                "read_pool_queue_wait_seconds",
                "Time a read-pool job waited in queue before pickup",
                &["op"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            work_seconds: register_histogram_vec_with_registry!(
                "read_pool_work_seconds",
                "Time a read-pool job spent executing (read+render)",
                &["op"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            submitted_total: register_int_counter_vec_with_registry!(
                "read_pool_submitted_total",
                "Read-pool submissions by outcome",
                &["op", "outcome"],
                registry,
            )
            .unwrap(),
            abandoned_total: register_int_counter_vec_with_registry!(
                "read_pool_abandoned_total",
                "Read-pool jobs skipped because the caller was gone at pickup",
                &["op"],
                registry,
            )
            .unwrap(),
            queue_depth: register_int_gauge_with_registry!(
                "read_pool_queue_depth",
                "Read-pool jobs currently queued",
                registry,
            )
            .unwrap(),
            active_threads: register_int_gauge_with_registry!(
                "read_pool_active_threads",
                "Read-pool threads currently executing a job",
                registry,
            )
            .unwrap(),
            size: register_int_gauge_with_registry!(
                "read_pool_size",
                "Configured read-pool thread count",
                registry,
            )
            .unwrap(),
        }
    }
}

/// A unit of blocking work handed to a worker thread. `run` is invoked exactly
/// once with the pool's metrics; it decides (via a closed response channel)
/// whether the caller is still waiting before doing any read.
struct Job {
    enqueued: Instant,
    op: &'static str,
    run: Box<dyn FnOnce(&ReadPoolMetrics) + Send + 'static>,
}

/// Fixed-size OS-thread pool with a bounded MPMC queue and admission control.
pub(crate) struct ReadPool {
    tx: Sender<Job>,
    metrics: Arc<ReadPoolMetrics>,
}

impl ReadPool {
    /// Pre-spawn `threads` worker threads draining a queue bounded at
    /// `queue_capacity`. A 0-thread pool would deadlock (nothing drains the
    /// queue); callers must pass `threads >= 1` (see [`resolve_pool_sizing`]).
    pub(crate) fn new(
        threads: usize,
        queue_capacity: usize,
        metrics: Arc<ReadPoolMetrics>,
    ) -> Self {
        let (tx, rx) = bounded::<Job>(queue_capacity);
        metrics.size.set(threads as i64);
        for _ in 0..threads {
            let rx = rx.clone();
            let metrics = metrics.clone();
            std::thread::Builder::new()
                .name("rpc-read-pool".to_string())
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        metrics.queue_depth.dec();
                        metrics
                            .queue_wait_seconds
                            .with_label_values(&[job.op])
                            .observe(job.enqueued.elapsed().as_secs_f64());
                        metrics.active_threads.inc();
                        (job.run)(&metrics);
                        metrics.active_threads.dec();
                    }
                })
                .expect("failed to spawn rpc-read-pool thread");
        }
        Self { tx, metrics }
    }

    /// Submit `f` to the pool and await its result. Returns
    /// `ResourceExhausted` when the queue is full (shedding), `Internal` when
    /// the pool has stopped, and `Cancelled` if the worker skipped the job
    /// because the caller went away before pickup.
    pub(crate) async fn run<F, R>(&self, op: &'static str, f: F) -> Result<R, RpcError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<R>();
        let job = Job {
            enqueued: Instant::now(),
            op,
            run: Box::new(move |metrics: &ReadPoolMetrics| {
                // Caller already gone (deadline elapsed / client cancelled):
                // skip the read entirely rather than burn a worker on a result
                // nobody will receive.
                if resp_tx.is_closed() {
                    metrics.abandoned_total.with_label_values(&[op]).inc();
                    return;
                }
                let started = Instant::now();
                let r = f();
                metrics
                    .work_seconds
                    .with_label_values(&[op])
                    .observe(started.elapsed().as_secs_f64());
                let _ = resp_tx.send(r);
            }),
        };

        // INVARIANT: increment the depth gauge before `try_send` so the
        // worker's matching `dec` (which fires at pickup, possibly before this
        // line returns on another thread) can never drive the gauge negative.
        self.metrics.queue_depth.inc();
        match self.tx.try_send(job) {
            Ok(()) => {
                self.metrics
                    .submitted_total
                    .with_label_values(&[op, "accepted"])
                    .inc();
            }
            Err(TrySendError::Full(_)) => {
                self.metrics.queue_depth.dec();
                self.metrics
                    .submitted_total
                    .with_label_values(&[op, "rejected_full"])
                    .inc();
                return Err(RpcError::new(
                    tonic::Code::ResourceExhausted,
                    "read pool saturated",
                ));
            }
            Err(TrySendError::Disconnected(_)) => {
                self.metrics.queue_depth.dec();
                self.metrics
                    .submitted_total
                    .with_label_values(&[op, "disconnected"])
                    .inc();
                return Err(RpcError::new(tonic::Code::Internal, "read pool stopped"));
            }
        }

        resp_rx
            .await
            .map_err(|_| RpcError::new(tonic::Code::Cancelled, "read cancelled"))
    }
}

/// Resolve the read pool's thread count and queue capacity. Env overrides win
/// over config, which falls back to its defaults; both are clamped to `>= 1`
/// because a 0-thread pool would deadlock.
pub(crate) fn resolve_pool_sizing(config: &sui_config::RpcConfig) -> (usize, usize) {
    let threads = std::env::var("SUI_RPC_READ_POOL_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| config.read_pool_threads())
        .max(1);
    let capacity = std::env::var("SUI_RPC_READ_POOL_QUEUE_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| config.read_pool_queue_capacity())
        .max(1);
    (threads, capacity)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    fn pool(threads: usize, capacity: usize) -> Arc<ReadPool> {
        let registry = Registry::new();
        let metrics = Arc::new(ReadPoolMetrics::new(&registry));
        Arc::new(ReadPool::new(threads, capacity, metrics))
    }

    /// Poll `cond` until it holds, failing fast (rather than hanging the test
    /// harness) if the pool never reaches the expected state.
    async fn wait_until(mut cond: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !cond() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("pool did not reach expected state within 10s");
    }

    #[tokio::test]
    async fn run_returns_result() {
        let pool = pool(2, 4);
        assert_eq!(pool.run("list_checkpoints", || 42).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn saturation_sheds() {
        let pool = pool(2, 2);
        let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();

        // Park a blocker on each of the two worker threads.
        for _ in 0..2 {
            let p = pool.clone();
            let release = release_rx.clone();
            tokio::spawn(async move {
                let _ = p
                    .run("list_checkpoints", move || release.recv().unwrap())
                    .await;
            });
        }
        // Both workers are busy once they enter their (parked) job bodies.
        wait_until(|| pool.metrics.active_threads.get() >= 2).await;

        // Fill the queue (capacity 2) with jobs the busy workers cannot pick up.
        for _ in 0..2 {
            let p = pool.clone();
            tokio::spawn(async move {
                let _ = p.run("list_checkpoints", || {}).await;
            });
        }
        wait_until(|| pool.metrics.queue_depth.get() >= 2).await;

        let err = pool.run("list_checkpoints", || {}).await.unwrap_err();
        assert_eq!(
            tonic::Status::from(err).code(),
            tonic::Code::ResourceExhausted
        );
        assert_eq!(
            pool.metrics
                .submitted_total
                .with_label_values(&["list_checkpoints", "rejected_full"])
                .get(),
            1
        );

        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
    }

    #[tokio::test]
    async fn abandoned_job_skipped() {
        let pool = pool(1, 4);
        let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();

        // Occupy the single worker with a parked blocker.
        {
            let p = pool.clone();
            let release = release_rx.clone();
            tokio::spawn(async move {
                let _ = p
                    .run("list_transactions", move || release.recv().unwrap())
                    .await;
            });
        }
        wait_until(|| pool.metrics.active_threads.get() >= 1).await;

        // Queue a victim behind the blocker.
        let ran = Arc::new(AtomicBool::new(false));
        let victim_ran = ran.clone();
        let p = pool.clone();
        let victim = tokio::spawn(async move {
            p.run("list_events", move || {
                victim_ran.store(true, Ordering::SeqCst);
                7usize
            })
            .await
        });

        wait_until(|| {
            pool.metrics
                .submitted_total
                .with_label_values(&["list_events", "accepted"])
                .get()
                >= 1
                && pool.metrics.queue_depth.get() >= 1
        })
        .await;

        // Abandon the victim: dropping the `run` future closes its oneshot, so
        // the worker sees `resp_tx.is_closed()` at pickup.
        victim.abort();
        let _ = victim.await;

        // Release the blocker so the worker reaches the abandoned victim.
        release_tx.send(()).unwrap();
        wait_until(|| {
            pool.metrics
                .abandoned_total
                .with_label_values(&["list_events"])
                .get()
                >= 1
        })
        .await;

        assert!(!ran.load(Ordering::SeqCst), "victim body must not run");
        assert_eq!(
            pool.metrics
                .abandoned_total
                .with_label_values(&["list_events"])
                .get(),
            1
        );
        assert_eq!(
            pool.metrics
                .work_seconds
                .with_label_values(&["list_events"])
                .get_sample_count(),
            0
        );
    }

    #[tokio::test]
    async fn no_head_of_line_blocking() {
        let pool = pool(2, 4);
        let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();

        {
            let p = pool.clone();
            let release = release_rx.clone();
            tokio::spawn(async move {
                let _ = p
                    .run("list_checkpoints", move || release.recv().unwrap())
                    .await;
            });
        }
        wait_until(|| pool.metrics.active_threads.get() >= 1).await;

        // The second thread must service this while the first is parked.
        assert_eq!(pool.run("list_checkpoints", || 99).await.unwrap(), 99);
        release_tx.send(()).unwrap();
    }

    #[tokio::test]
    async fn metrics_sanity() {
        let pool = pool(2, 8);
        for i in 0..5usize {
            assert_eq!(pool.run("list_events", move || i).await.unwrap(), i);
        }
        assert_eq!(
            pool.metrics
                .work_seconds
                .with_label_values(&["list_events"])
                .get_sample_count(),
            5
        );
        assert_eq!(
            pool.metrics
                .queue_wait_seconds
                .with_label_values(&["list_events"])
                .get_sample_count(),
            5
        );
        assert_eq!(pool.metrics.queue_depth.get(), 0);
    }
}

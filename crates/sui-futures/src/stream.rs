// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::future::poll_fn;
use std::panic;
use std::pin::pin;
use std::sync::Arc;

use futures::FutureExt;
use futures::future::try_join_all;
use futures::stream::Stream;
use futures::try_join;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// Runtime configuration for adaptive concurrency control.
///
/// For fixed concurrency, use [`ConcurrencyConfig::fixed`].
/// For adaptive concurrency, use [`ConcurrencyConfig::adaptive`] which requires the three core
/// parameters (initial, min, max). The dead-band thresholds have sensible defaults and can be
/// overridden with a chainable setter:
///
/// ```ignore
/// ConcurrencyConfig::adaptive(10, 1, 20)
///     .with_dead_band(0.5, 0.9)
/// ```
#[derive(Debug, Clone)]
pub struct ConcurrencyConfig {
    pub initial: usize,
    pub min: usize,
    pub max: usize,
    /// Fill fraction below which the controller may increase the limit (if saturated).
    /// Default: 0.6.
    pub dead_band_low: f64,
    /// Fill fraction at or above which the controller decreases the limit. Default: 0.85.
    pub dead_band_high: f64,
}

/// Snapshot of concurrency stats passed to the `report` callback.
#[derive(Debug, Clone, Copy)]
pub struct ConcurrencyStats {
    pub limit: usize,
    pub inflight: usize,
}

/// Wrapper type for errors to allow the body of a `try_for_each_spawned` call to signal that it
/// either wants to return early (`Break`) out of the loop, or propagate an error (`Err(E)`).
#[derive(Debug)]
pub enum Break<E> {
    Break,
    Err(E),
}

/// Extension trait introducing `try_for_each_spawned` to all streams.
pub trait TrySpawnStreamExt: Stream {
    /// Attempts to run this stream to completion, executing the provided asynchronous closure on
    /// each element from the stream as elements become available.
    ///
    /// This is similar to [`futures::stream::StreamExt::for_each_concurrent`], but it may take advantage of any
    /// parallelism available in the underlying runtime, because each unit of work is spawned as
    /// its own tokio task.
    ///
    /// The first argument is an optional limit on the number of tasks to spawn concurrently.
    /// Values of `0` and `None` are interpreted as no limit, and any other value will result in no
    /// more than that many tasks being spawned at one time.
    ///
    /// ## Safety
    ///
    /// This function will panic if any of its futures panics, will return early with success if
    /// the runtime it is running on is cancelled, and will return early with an error propagated
    /// from any worker that produces an error.
    fn try_for_each_spawned<Fut, F, E>(
        self,
        limit: impl Into<Option<usize>>,
        f: F,
    ) -> impl Future<Output = Result<(), E>>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static;

    /// Process each stream item through a spawned task, sending results to a single channel.
    ///
    /// Each item is passed to `f` which returns a future producing `Result<T, Break<E>>`. The
    /// resulting `T` is sent to `tx`. Concurrency is controlled by `config`: for fixed configs,
    /// the limit never changes; for adaptive configs, the limit adjusts based on the fill fraction
    /// of the output channel.
    ///
    /// Unlike [`try_for_each_broadcast_spawned`](TrySpawnStreamExt::try_for_each_broadcast_spawned),
    /// `T` does not need to be `Clone` since there is only a single receiver.
    ///
    /// The `report` callback is invoked each iteration with concurrency stats for metrics.
    fn try_for_each_send_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        tx: mpsc::Sender<T>,
        report: R,
    ) -> impl Future<Output = Result<(), Break<E>>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Send + 'static,
        E: Send + 'static,
        R: Fn(ConcurrencyStats);

    /// Process each stream item through a spawned task, broadcasting results to multiple channels.
    ///
    /// Same as [`try_for_each_send_spawned`](TrySpawnStreamExt::try_for_each_send_spawned) but
    /// sends a clone of each result to every channel in `txs`. Fill fraction is measured as the
    /// maximum across all channels. Requires `T: Clone` since values are cloned to each receiver.
    fn try_for_each_broadcast_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        txs: Vec<mpsc::Sender<T>>,
        report: R,
    ) -> impl Future<Output = Result<(), Break<E>>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Clone + Send + Sync + 'static,
        E: Send + 'static,
        R: Fn(ConcurrencyStats);
}

/// Abstraction over single-channel and broadcast sending so that
/// `adaptive_spawn_send` can be generic over both. This avoids requiring `T: Clone`
/// on the single-sender path (`SingleSender` moves the value without cloning).
///
/// Implementors must be cheaply cloneable (they are cloned for each spawned task).
trait Sender: Clone + Send + Sync + 'static {
    type Value: Send + 'static;

    /// Send a value downstream. Returns `Err(())` if the channel(s) are closed.
    fn send(&self, value: Self::Value) -> impl Future<Output = Result<(), ()>> + Send;

    /// Measure the fill fraction of the downstream channel(s).
    /// Returns a value in [0.0, 1.0] where 1.0 means completely full.
    fn fill(&self) -> f64;
}

/// Single-channel sender.
struct SingleSender<T>(mpsc::Sender<T>);

/// Broadcast sender that clones the value to all channels.
struct BroadcastSender<T>(Arc<Vec<mpsc::Sender<T>>>);

impl ConcurrencyConfig {
    pub fn fixed(n: usize) -> Self {
        Self {
            initial: n,
            min: n,
            max: n,
            dead_band_low: 0.6,
            dead_band_high: 0.85,
        }
    }

    pub fn adaptive(initial: usize, min: usize, max: usize) -> Self {
        Self {
            initial,
            min,
            max,
            dead_band_low: 0.6,
            dead_band_high: 0.85,
        }
    }

    pub fn with_dead_band(mut self, low: f64, high: f64) -> Self {
        self.dead_band_low = low;
        self.dead_band_high = high;
        self
    }
}

impl<E> From<E> for Break<E> {
    fn from(e: E) -> Self {
        Break::Err(e)
    }
}

impl<S: Stream + Sized + 'static> TrySpawnStreamExt for S {
    async fn try_for_each_spawned<Fut, F, E>(
        self,
        limit: impl Into<Option<usize>>,
        mut f: F,
    ) -> Result<(), E>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static,
    {
        // Maximum number of tasks to spawn concurrently.
        let limit = match limit.into() {
            Some(0) | None => usize::MAX,
            Some(n) => n,
        };

        // Number of permits to spawn tasks left.
        let mut permits = limit;
        // Handles for already spawned tasks.
        let mut join_set = JoinSet::new();
        // Whether the worker pool has stopped accepting new items and is draining.
        let mut draining = false;
        // Error that occurred in one of the workers, to be propagated to the called on exit.
        let mut error = None;

        let mut self_ = pin!(self);

        loop {
            // Eager inner loop: spawn tasks while permits allow and items are ready,
            // avoiding select! overhead when items are immediately available.
            while !draining && permits > 0 {
                match poll_fn(|cx| self_.as_mut().poll_next(cx)).now_or_never() {
                    Some(Some(item)) => {
                        permits -= 1;
                        join_set.spawn(f(item));
                    }
                    Some(None) => {
                        // If the stream is empty, signal that the worker pool is going to
                        // start draining now, so that once we get all our permits back, we
                        // know we can wind down the pool.
                        draining = true;
                    }
                    None => break,
                }
            }

            tokio::select! {
                biased;

                Some(res) = join_set.join_next() => {
                    match res {
                        Ok(Err(e)) if error.is_none() => {
                            error = Some(e);
                            permits += 1;
                            draining = true;
                        }

                        Ok(_) => permits += 1,

                        // Worker panicked, propagate the panic.
                        Err(e) if e.is_panic() => {
                            panic::resume_unwind(e.into_panic())
                        }

                        // Worker was cancelled -- this can only happen if its join handle was
                        // cancelled (not possible because that was created in this function),
                        // or the runtime it was running in was wound down, in which case,
                        // prepare the worker pool to drain.
                        Err(e) => {
                            assert!(e.is_cancelled());
                            permits += 1;
                            draining = true;
                        }
                    }
                }

                next = poll_fn(|cx| self_.as_mut().poll_next(cx)),
                    if !draining && permits > 0 => {
                    if let Some(item) = next {
                        permits -= 1;
                        join_set.spawn(f(item));
                    } else {
                        draining = true;
                    }
                }

                else => {
                    if permits == limit && draining {
                        break;
                    }
                }
            }
        }

        if let Some(e) = error { Err(e) } else { Ok(()) }
    }

    async fn try_for_each_send_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        tx: mpsc::Sender<T>,
        report: R,
    ) -> Result<(), Break<E>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Send + 'static,
        E: Send + 'static,
        R: Fn(ConcurrencyStats),
    {
        adaptive_spawn_send(self, config, f, SingleSender(tx), report).await
    }

    async fn try_for_each_broadcast_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        txs: Vec<mpsc::Sender<T>>,
        report: R,
    ) -> Result<(), Break<E>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Clone + Send + Sync + 'static,
        E: Send + 'static,
        R: Fn(ConcurrencyStats),
    {
        adaptive_spawn_send(self, config, f, BroadcastSender(Arc::new(txs)), report).await
    }
}

impl<T> Clone for SingleSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Send + 'static> Sender for SingleSender<T> {
    type Value = T;

    async fn send(&self, value: T) -> Result<(), ()> {
        self.0.send(value).await.map_err(|_| ())
    }

    fn fill(&self) -> f64 {
        1.0 - (self.0.capacity() as f64 / self.0.max_capacity() as f64)
    }
}

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Clone + Send + Sync + 'static> Sender for BroadcastSender<T> {
    type Value = T;

    async fn send(&self, value: T) -> Result<(), ()> {
        let (last, rest) = self.0.split_last().ok_or(())?;
        let rest_fut = try_join_all(rest.iter().map(|tx| {
            let v = value.clone();
            async move { tx.send(v).await.map_err(|_| ()) }
        }));
        let last_fut = last.send(value).map(|r| r.map_err(|_| ()));
        try_join!(rest_fut, last_fut)?;
        Ok(())
    }

    fn fill(&self) -> f64 {
        self.0
            .iter()
            .map(|tx| 1.0 - (tx.capacity() as f64 / tx.max_capacity() as f64))
            .fold(0.0f64, f64::max)
    }
}

/// Shared adaptive concurrency loop used by both `try_for_each_send_spawned` and
/// `try_for_each_broadcast_spawned`.
///
/// The algorithm is a result of weeks of trial and error guided by lots of Claude Research queries.
/// I tried to document where each of the ideas came from in the footnotes of this comment.
///
/// # Adaptive concurrency control
///
/// When `config.min < config.max`, this function dynamically adjusts how many tasks run
/// concurrently based on how full the downstream channel(s) are. The goal is to find and
/// hold a stable concurrency limit that keeps downstream fed without using excessive memory.
///
/// ## The core problem
///
/// N tasks share one set of output channels. If we naively cut the limit every time any task
/// sees a full channel, all N tasks observe the same fullness at once and each triggers a
/// cut — collapsing the limit exponentially (`limit * ratio^N`) instead of once. This causes
/// wild oscillation: the limit crashes to near-zero then slowly climbs back, wasting ~50%
/// of throughput.
///
/// ## How this controller avoids that
///
/// **Epoch-gated reductions** — Each task captures the current `epoch` when spawned. A
/// reduction increments the epoch. If a completing task's epoch is stale (from before the
/// last reduction), its congestion signal is ignored. This ensures only one reduction fires
/// per congestion event, no matter how many tasks see it. [1]
///
/// **Severity-scaled cuts** — Instead of "full → cut by fixed ratio", a `severity` score
/// derived from the fill fraction drives the size of the cut linearly:
/// `keep = 0.8 - 0.3 * severity` where `severity = (fill - 0.85) / 0.15`.
/// At fill = 0.85 (threshold), severity = 0 and the controller keeps 80% (gentle 20%
/// cut). At fill = 1.0 (hard saturation), severity = 1 and the controller keeps 50%
/// (aggressive halving). [2]
///
/// **Guaranteed progress** — After computing the proportional cut, the new limit is capped
/// at `limit - 1` so that `ceil()` rounding cannot stall decreases at small values.
///
/// **Dead band between increase and decrease** — Three zones based on fill fraction:
/// - `fill >= 0.85`: decrease (proportionally, epoch-gated)
/// - `fill < 0.60`: increase (if the limit was actually being used)
/// - `0.60–0.85`: do nothing
///
/// The gap between the increase and decrease thresholds prevents the controller from
/// endlessly flip-flopping at the boundary — it finds a stable operating point and stays
/// there. [3]
///
/// **Log10-scaled increase** — The limit grows by `ceil(log10(limit))` instead of +1. This
/// scales sub-linearly: recovery isn't painfully slow at high limits (e.g. +3 at limit=200
/// vs +1), but it's slower than proportional growth so it doesn't overshoot. [4]
///
/// **Saturation guard** — The limit only increases when inflight actually reached the limit
/// (`was_saturated`). Without this, the limit would grow unboundedly during low-load periods
/// since fill stays low and every completion triggers an increase — even though the extra
/// concurrency was never actually used or tested against real backpressure. When load
/// eventually does arrive, the inflated limit allows a burst that overwhelms the channel.
/// Crucially, `was_saturated` resets on *both* increase and decrease. The decrease-side
/// reset acts as a cooling-off period: after cutting the limit, the controller must see
/// the new lower limit fully utilized before raising it again. Without this, the old
/// saturation proof carries over, and the increase branch fires on the very next low-fill
/// completion — undoing the decrease and causing oscillation.
///
/// ---
/// [1] Same idea as TCP NewReno's `recover` variable (RFC 6582): record a high-water mark
///     when congestion is detected, suppress further reductions until tasks past that mark
///     start completing.
/// [2] Analogous to DCTCP's proportional window reduction (RFC 8257):
///     `cwnd = cwnd * (1 - α/2)` where α is the marked fraction.
/// [3] TCP Vegas uses the same approach with alpha/beta thresholds around estimated queue
///     depth to avoid oscillation at the operating point.
/// [4] Netflix's `concurrency-limits` VegasLimit uses the same `log10(limit)` additive
///     increase with a `max(1)` floor.
async fn adaptive_spawn_send<S, Fut, F, E, Tx, R>(
    stream: S,
    config: ConcurrencyConfig,
    mut f: F,
    sender: Tx,
    report: R,
) -> Result<(), Break<E>>
where
    S: Stream + 'static,
    Fut: Future<Output = Result<Tx::Value, Break<E>>> + Send + 'static,
    F: FnMut(S::Item) -> Fut,
    E: Send + 'static,
    Tx: Sender,
    R: Fn(ConcurrencyStats),
{
    assert!(config.min >= 1, "ConcurrencyConfig::min must be >= 1");
    let mut limit = config.initial;
    let mut epoch: u64 = 0;
    let mut was_saturated = false;
    let mut tasks: JoinSet<Result<u64, Break<E>>> = JoinSet::new();
    let mut stream_done = false;
    let mut error: Option<Break<E>> = None;

    let mut stream = pin!(stream);

    loop {
        if tasks.is_empty() && (stream_done || error.is_some()) {
            break;
        }

        // Eager inner loop: spawn tasks while under the limit and items are ready.
        while tasks.len() < limit && !stream_done && error.is_none() {
            match poll_fn(|cx| stream.as_mut().poll_next(cx)).now_or_never() {
                Some(Some(item)) => {
                    let fut = f(item);
                    let tx = sender.clone();
                    let spawn_epoch = epoch;
                    tasks.spawn(async move {
                        let value = fut.await?;
                        tx.send(value).await.map_err(|_| Break::Break)?;
                        Ok(spawn_epoch)
                    });
                    if tasks.len() >= limit {
                        was_saturated = true;
                    }
                }
                Some(None) => stream_done = true,
                None => break,
            }
        }

        let completed = tokio::select! {
            biased;

            Some(r) = tasks.join_next(), if !tasks.is_empty() => Some(r),

            next = poll_fn(|cx| stream.as_mut().poll_next(cx)),
                if tasks.len() < limit && !stream_done && error.is_none() =>
            {
                if let Some(item) = next {
                    let fut = f(item);
                    let tx = sender.clone();
                    let spawn_epoch = epoch;
                    tasks.spawn(async move {
                        let value = fut.await?;
                        tx.send(value).await.map_err(|_| Break::Break)?;
                        Ok(spawn_epoch)
                    });
                    if tasks.len() >= limit {
                        was_saturated = true;
                    }
                } else {
                    stream_done = true;
                }
                None
            }

            else => {
                if tasks.is_empty() && (stream_done || error.is_some()) {
                    break;
                }
                None
            }
        };

        // Handle all completions: the one from select (if any) + drain ready ones.
        for join_result in completed.into_iter().chain(std::iter::from_fn(|| {
            tasks.join_next().now_or_never().flatten()
        })) {
            match join_result {
                Ok(Ok(spawn_epoch)) => {
                    // Adjust concurrency limit based on channel fill fraction.
                    // - fill >= dead_band_high: severity-scaled decrease (epoch-gated)
                    // - fill < dead_band_low and limit was saturated: log10-scaled increase
                    // - fill in [dead_band_low, dead_band_high): hold steady
                    // When config is fixed (min == max), clamp/min keep limit unchanged.
                    let fill = sender.fill();
                    if fill >= config.dead_band_high && spawn_epoch == epoch {
                        // Proportional cut that gets aggressive near saturation.
                        // At dead_band_high: keep 80% (cut 20%)
                        // At midpoint:       keep 65% (cut 35%)
                        // At 1.00:           keep 50% (cut 50%)
                        let severity =
                            (fill - config.dead_band_high) / (1.0 - config.dead_band_high);
                        let keep = 0.8 - 0.3 * severity;
                        let new_limit = ((limit as f64) * keep).ceil() as usize;
                        limit = new_limit.min(limit.saturating_sub(1)).max(config.min);
                        limit = limit.clamp(config.min, config.max);
                        epoch += 1;
                        // Reset was_saturated so the controller must re-prove the new
                        // lower limit is fully utilized before increasing again. This
                        // cooling-off period prevents decrease→increase oscillation:
                        // without it, the old saturation proof carries over and the
                        // increase branch fires immediately on the next low-fill
                        // completion, undoing the decrease.
                        was_saturated = false;
                    } else if fill < config.dead_band_low && was_saturated {
                        let increment = ((limit as f64).log10().ceil() as usize).max(1);
                        limit = (limit + increment).min(config.max);
                        was_saturated = false;
                    }
                }
                Ok(Err(e)) if error.is_none() => error = Some(e),
                Ok(Err(_)) => {}
                Err(e) if e.is_panic() => panic::resume_unwind(e.into_panic()),
                Err(e) => {
                    assert!(e.is_cancelled());
                    stream_done = true;
                }
            }
        }

        report(ConcurrencyStats {
            limit,
            inflight: tasks.len(),
        });
    }

    if let Some(e) = error { Err(e) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use futures::stream;

    use super::*;

    #[tokio::test]
    async fn for_each_explicit_sequential_iteration() {
        let actual = Arc::new(Mutex::new(vec![]));
        let result = stream::iter(0..20)
            .try_for_each_spawned(1, |i| {
                let actual = actual.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(20 - i)).await;
                    actual.lock().unwrap().push(i);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let actual = Arc::try_unwrap(actual).unwrap().into_inner().unwrap();
        let expect: Vec<_> = (0..20).collect();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn for_each_concurrent_iteration() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned(16, |i| {
                let actual = actual.clone();
                async move {
                    actual.fetch_add(i, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        let expect = 99 * 100 / 2;
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn for_each_implicit_unlimited_iteration() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned(None, |i| {
                let actual = actual.clone();
                async move {
                    actual.fetch_add(i, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        let expect = 99 * 100 / 2;
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn for_each_explicit_unlimited_iteration() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned(0, |i| {
                let actual = actual.clone();
                async move {
                    actual.fetch_add(i, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        let expect = 99 * 100 / 2;
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    async fn for_each_max_concurrency() {
        #[derive(Default, Debug)]
        struct Jobs {
            max: AtomicUsize,
            curr: AtomicUsize,
        }

        let jobs = Arc::new(Jobs::default());

        let result = stream::iter(0..32)
            .try_for_each_spawned(4, |_| {
                let jobs = jobs.clone();
                async move {
                    jobs.curr.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let prev = jobs.curr.fetch_sub(1, Ordering::Relaxed);
                    jobs.max.fetch_max(prev, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let Jobs { max, curr } = Arc::try_unwrap(jobs).unwrap();
        assert_eq!(curr.into_inner(), 0);
        assert!(max.into_inner() <= 4);
    }

    #[tokio::test]
    async fn for_each_error_propagation() {
        let actual = Arc::new(Mutex::new(vec![]));
        let result = stream::iter(0..100)
            .try_for_each_spawned(None, |i| {
                let actual = actual.clone();
                async move {
                    if i < 42 {
                        actual.lock().unwrap().push(i);
                        Ok(())
                    } else {
                        Err(())
                    }
                }
            })
            .await;

        assert!(result.is_err());

        let actual = Arc::try_unwrap(actual).unwrap().into_inner().unwrap();
        let expect: Vec<_> = (0..42).collect();
        assert_eq!(expect, actual);
    }

    #[tokio::test]
    #[should_panic]
    async fn for_each_panic_propagation() {
        let _ = stream::iter(0..100)
            .try_for_each_spawned(None, |i| async move {
                assert!(i < 42);
                Ok::<(), ()>(())
            })
            .await;
    }

    #[tokio::test]
    async fn send_spawned_basic() {
        let (tx, mut rx) = mpsc::channel(100);
        let result = stream::iter(0..10u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(4),
                |i| async move { Ok::<_, Break<()>>(i * 2) },
                tx,
                |_| {},
            )
            .await;

        assert!(result.is_ok());

        let mut values = Vec::new();
        while let Ok(v) = rx.try_recv() {
            values.push(v);
        }
        values.sort();
        let expected: Vec<u64> = (0..10).map(|i| i * 2).collect();
        assert_eq!(values, expected);
    }

    #[tokio::test]
    async fn send_spawned_error_propagation() {
        let (tx, _rx) = mpsc::channel(100);
        let result: Result<(), Break<String>> = stream::iter(0..10u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(1),
                |i| async move {
                    if i < 3 {
                        Ok(i)
                    } else {
                        Err(Break::Err("fail".to_string()))
                    }
                },
                tx,
                |_| {},
            )
            .await;

        assert!(matches!(result, Err(Break::Err(ref s)) if s == "fail"));
    }

    #[tokio::test]
    async fn send_spawned_channel_closed() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        let result: Result<(), Break<()>> = stream::iter(0..10u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(1),
                |i| async move { Ok(i) },
                tx,
                |_| {},
            )
            .await;

        assert!(matches!(result, Err(Break::Break)));
    }

    #[tokio::test]
    async fn send_spawned_reports_stats() {
        let reported: Arc<Mutex<Vec<ConcurrencyStats>>> = Arc::new(Mutex::new(Vec::new()));
        let (tx, _rx) = mpsc::channel(100);

        let reported2 = reported.clone();
        let _ = stream::iter(0..5u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(2),
                |i| async move { Ok::<_, Break<()>>(i) },
                tx,
                move |stats| {
                    reported2.lock().unwrap().push(stats);
                },
            )
            .await;

        let reports = reported.lock().unwrap();
        for stats in reports.iter() {
            assert_eq!(stats.limit, 2);
        }
    }

    #[tokio::test]
    async fn broadcast_spawned_basic() {
        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);
        let txs = vec![tx1, tx2];

        let result = stream::iter(0..5u64)
            .try_for_each_broadcast_spawned(
                ConcurrencyConfig::fixed(2),
                |i| async move { Ok::<_, Break<()>>(i * 3) },
                txs,
                |_| {},
            )
            .await;

        assert!(result.is_ok());

        let mut v1 = Vec::new();
        while let Ok(v) = rx1.try_recv() {
            v1.push(v);
        }
        let mut v2 = Vec::new();
        while let Ok(v) = rx2.try_recv() {
            v2.push(v);
        }
        v1.sort();
        v2.sort();
        let expected: Vec<u64> = (0..5).map(|i| i * 3).collect();
        assert_eq!(v1, expected);
        assert_eq!(v2, expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_spawned_adaptive_decreases_limit() {
        // Use a small channel to force fill to rise quickly.
        let (tx, mut rx) = mpsc::channel(4);
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..100u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::adaptive(10, 1, 20),
                    |i| async move {
                        // Simulate work so tasks take time
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Drain slowly so the channel fills up
        let mut received = Vec::new();
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            match rx.try_recv() {
                Ok(v) => received.push(v),
                Err(mpsc::error::TryRecvError::Empty) => {
                    if handle.is_finished() {
                        // Drain remaining
                        while let Ok(v) = rx.try_recv() {
                            received.push(v);
                        }
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    while let Ok(v) = rx.try_recv() {
                        received.push(v);
                    }
                    break;
                }
            }
        }

        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        let min_limit = limits.iter().copied().min().unwrap_or(10);
        assert!(
            min_limit < 10,
            "Limit should have decreased from initial=10, min observed: {min_limit}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_spawned_adaptive_recovers_after_decrease() {
        let (tx, mut rx) = mpsc::channel(4);
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..200u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::adaptive(10, 1, 20),
                    |i| async move {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Phase 1: drain slowly so the channel fills and the limit decreases.
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = rx.try_recv();
        }

        // Record the lowest limit observed so far.
        let low_water = {
            let lims = limits.lock().unwrap();
            lims.iter().copied().min().unwrap_or(10)
        };
        assert!(
            low_water < 10,
            "Limit should have decreased, min={low_water}"
        );

        // Phase 2: drain eagerly so fill drops and the limit recovers.
        while (rx.recv().await).is_some() {
            if handle.is_finished() {
                while rx.try_recv().is_ok() {}
                break;
            }
        }

        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        let recovered_max = limits.iter().copied().rev().take(30).max().unwrap_or(0);
        assert!(
            recovered_max > low_water,
            "Limit should have recovered above {low_water}, best late limit: {recovered_max}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_spawned_adaptive_respects_min() {
        let (tx, mut rx) = mpsc::channel(2);
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..100u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::adaptive(10, 5, 20),
                    |i| async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Drain very slowly to force congestion.
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            match rx.try_recv() {
                Ok(_) => {}
                Err(mpsc::error::TryRecvError::Empty) => {
                    if handle.is_finished() {
                        while rx.try_recv().is_ok() {}
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    while rx.try_recv().is_ok() {}
                    break;
                }
            }
        }

        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        let min_limit = limits.iter().copied().min().unwrap_or(10);
        assert!(
            min_limit >= 5,
            "Limit should never drop below min=5, observed: {min_limit}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_spawned_adaptive_respects_max() {
        let (tx, mut rx) = mpsc::channel(1000);
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..200u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::adaptive(2, 1, 8),
                    |i| async move {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Drain eagerly so fill stays low and the limit keeps trying to increase.
        while rx.recv().await.is_some() {}

        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        let max_limit = limits.iter().copied().max().unwrap_or(0);
        assert!(
            max_limit <= 8,
            "Limit should never exceed max=8, observed: {max_limit}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_spawned_epoch_prevents_stampede() {
        let (tx, mut rx) = mpsc::channel(2);
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..60u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::adaptive(20, 1, 20),
                    |i| async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Don't drain initially so the channel fills up. Many tasks will complete while
        // the channel is full, exercising the epoch guard.
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Now drain to let the producer finish.
        while rx.recv().await.is_some() {}
        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        // Deduplicate consecutive equal values to get actual transitions.
        let transitions: Vec<usize> = limits
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .windows(2)
            .filter_map(|w| if w[0] != w[1] { Some(w[1]) } else { None })
            .collect();

        // For every decrease, verify it's at most a single proportional cut.
        // The maximum single-step cut at fill=1.0 is: new = ceil(old * 0.5).
        for pair in limits.iter().copied().collect::<Vec<_>>().windows(2) {
            let (old, new) = (pair[0], pair[1]);
            if new < old {
                let min_allowed = ((old as f64) * 0.5).ceil() as usize;
                assert!(
                    new >= min_allowed,
                    "Stampede detected: limit dropped from {old} to {new}, \
                     minimum allowed single-step is {min_allowed}. Transitions: {transitions:?}"
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn broadcast_spawned_slow_receiver_triggers_decrease() {
        let (tx_fast, mut rx_fast) = mpsc::channel(100);
        let (tx_slow, mut rx_slow) = mpsc::channel(4);
        let txs = vec![tx_fast, tx_slow];
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..100u64)
                .try_for_each_broadcast_spawned(
                    ConcurrencyConfig::adaptive(10, 1, 20),
                    |i| async move {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    txs,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Drain the fast channel eagerly.
        let fast_drain = tokio::spawn(async move { while rx_fast.recv().await.is_some() {} });

        // Drain the slow channel slowly.
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            match rx_slow.try_recv() {
                Ok(_) => {}
                Err(mpsc::error::TryRecvError::Empty) => {
                    if handle.is_finished() {
                        while rx_slow.try_recv().is_ok() {}
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    while rx_slow.try_recv().is_ok() {}
                    break;
                }
            }
        }

        handle.await.unwrap().unwrap();
        fast_drain.await.unwrap();

        let limits = limits.lock().unwrap();
        let min_limit = limits.iter().copied().min().unwrap_or(10);
        assert!(
            min_limit < 10,
            "Limit should have decreased due to slow receiver, min observed: {min_limit}"
        );
    }

    #[tokio::test]
    async fn broadcast_spawned_channel_closed() {
        let (tx1, _rx1) = mpsc::channel(100);
        let (tx2, rx2) = mpsc::channel(100);
        drop(rx2);

        let result: Result<(), Break<()>> = stream::iter(0..10u64)
            .try_for_each_broadcast_spawned(
                ConcurrencyConfig::fixed(2),
                |i| async move { Ok(i) },
                vec![tx1, tx2],
                |_| {},
            )
            .await;

        assert!(matches!(result, Err(Break::Break)));
    }

    #[tokio::test]
    async fn fixed_concurrency_limit_never_changes() {
        let limits: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
        let (tx, mut rx) = mpsc::channel(2);

        let limits2 = limits.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..20u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::fixed(5),
                    |i| async move { Ok::<_, Break<()>>(i) },
                    tx,
                    move |stats| {
                        limits2.lock().unwrap().push(stats.limit);
                    },
                )
                .await
        });

        // Drain the receiver so sends don't block.
        while rx.recv().await.is_some() {}

        handle.await.unwrap().unwrap();

        let limits = limits.lock().unwrap();
        for &g in limits.iter() {
            assert_eq!(g, 5, "Fixed limit should never change");
        }
    }
}

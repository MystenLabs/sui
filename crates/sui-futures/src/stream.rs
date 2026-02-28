// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, future::poll_fn, panic, pin::pin};

use futures::{FutureExt, stream::Stream};
use tokio::task::JoinSet;
use tracing::debug;

/// Adaptive concurrency gauge that encapsulates the fill-proportional control algorithm.
///
/// When adaptive, the gauge adjusts based on a fill signal:
/// - fill >= fill_high: proportional decrease (severity scales with congestion)
/// - fill < fill_low and previously saturated: sqrt-scaled increase
/// - fill in [fill_low, fill_high): dead zone, hold steady
///
/// An epoch counter prevents cascading reductions: a decrease is only applied when the
/// completing task's epoch matches the current one (epoch is bumped on every decrease).
pub struct AdaptiveGauge {
    value: usize,
    min: usize,
    max: usize,
    fill_high: f64,
    fill_low: f64,
    epoch: u64,
    was_saturated: bool,
    adaptive: bool,
}

impl AdaptiveGauge {
    /// Create a fixed gauge that never adjusts.
    pub fn fixed(n: usize) -> Self {
        Self {
            value: n,
            min: n,
            max: n,
            fill_high: 0.85,
            fill_low: 0.6,
            epoch: 0,
            was_saturated: false,
            adaptive: false,
        }
    }

    /// Create an adaptive gauge that adjusts via fill signal.
    pub fn adaptive(initial: usize, min: usize, max: usize) -> Self {
        assert!(min >= 1, "min concurrency must be >= 1");
        assert!(min <= max, "min must be <= max");
        Self {
            value: initial.clamp(min, max),
            min,
            max,
            fill_high: 0.85,
            fill_low: 0.6,
            epoch: 0,
            was_saturated: false,
            adaptive: true,
        }
    }

    /// Set custom fill thresholds.
    pub fn with_fill_thresholds(mut self, high: f64, low: f64) -> Self {
        self.fill_high = high;
        self.fill_low = low;
        self
    }

    /// Adjust the gauge based on the spawn epoch and current fill fraction.
    pub fn adjust(&mut self, spawn_epoch: u64, fill: f64) {
        if !self.adaptive {
            return;
        }

        if fill >= self.fill_high && spawn_epoch == self.epoch {
            self.value = ((self.value as f64) * (1.0 - fill / 2.0)).ceil() as usize;
            self.value = self.value.clamp(self.min, self.max);
            self.epoch += 1;
            self.was_saturated = false;
            debug!(
                gauge = self.value,
                fill,
                epoch = self.epoch,
                "Concurrency decreased"
            );
        } else if fill < self.fill_low && self.was_saturated {
            let increment = ((self.value as f64).sqrt().ceil() as usize).max(1);
            self.value = (self.value + increment).min(self.max);
            self.was_saturated = false;
        }
    }

    /// Mark that the concurrency limit was the bottleneck (stream had work but gauge was full).
    pub fn mark_saturated(&mut self) {
        self.was_saturated = true;
    }

    pub fn value(&self) -> usize {
        self.value
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn is_adaptive(&self) -> bool {
        self.adaptive
    }
}

/// Trait for reporting concurrency metrics. Callers implement this to wire in their
/// prometheus gauges.
pub trait ConcurrencyMetrics {
    fn report(&self, limit: usize, inflight: usize);
}

impl ConcurrencyMetrics for () {
    fn report(&self, _: usize, _: usize) {}
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

    /// Like [`try_for_each_spawned`](TrySpawnStreamExt::try_for_each_spawned), but with an
    /// [`AdaptiveGauge`] controlling concurrency. After each task completion, `measure_fill` is
    /// called to sample the downstream fill fraction, and the gauge adjusts accordingly.
    fn try_for_each_spawned_adaptive<Fut, F, E, M, Met>(
        self,
        gauge: AdaptiveGauge,
        f: F,
        measure_fill: M,
        metrics: Met,
    ) -> impl Future<Output = Result<(), E>>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static,
        M: Fn() -> f64,
        Met: ConcurrencyMetrics;
}

/// Wrapper type for errors to allow the body of a `try_for_each_spawned` call to signal that it
/// either wants to return early (`Break`) out of the loop, or propagate an error (`Err(E)`).
pub enum Break<E> {
    Break,
    Err(E),
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

    async fn try_for_each_spawned_adaptive<Fut, F, E, M, Met>(
        self,
        mut gauge: AdaptiveGauge,
        mut f: F,
        measure_fill: M,
        metrics: Met,
    ) -> Result<(), E>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static,
        M: Fn() -> f64,
        Met: ConcurrencyMetrics,
    {
        // Each spawned task captures the epoch at spawn time and returns it on completion,
        // so the gauge can gate cascading reductions.
        let mut join_set: JoinSet<Result<u64, E>> = JoinSet::new();
        let mut draining = false;
        let mut error = None;

        let mut self_ = pin!(self);

        /// Spawn a future into the join set, tagging it with the epoch at spawn time.
        fn spawn_with_epoch<Fut, E>(join_set: &mut JoinSet<Result<u64, E>>, fut: Fut, epoch: u64)
        where
            Fut: Future<Output = Result<(), E>> + Send + 'static,
            E: Send + 'static,
        {
            join_set.spawn(async move {
                fut.await?;
                Ok(epoch)
            });
        }

        /// Process a single task completion, adjusting the gauge when adaptive.
        fn handle_completion<E>(
            result: Result<Result<u64, E>, tokio::task::JoinError>,
            gauge: &mut AdaptiveGauge,
            draining: &mut bool,
            error: &mut Option<E>,
            measure_fill: &dyn Fn() -> f64,
        ) {
            match result {
                Ok(Ok(spawn_epoch)) => {
                    if gauge.is_adaptive() {
                        let fill = measure_fill();
                        gauge.adjust(spawn_epoch, fill);
                    }
                }
                Ok(Err(e)) if error.is_none() => {
                    *error = Some(e);
                    *draining = true;
                }
                Ok(Err(_)) => {}
                Err(e) if e.is_panic() => panic::resume_unwind(e.into_panic()),
                Err(e) => {
                    assert!(e.is_cancelled());
                    *draining = true;
                }
            }
        }

        loop {
            if join_set.is_empty() && draining {
                break;
            }

            // Eager inner loop: spawn tasks while the gauge allows and items are ready.
            while !draining && join_set.len() < gauge.value() {
                match poll_fn(|cx| self_.as_mut().poll_next(cx)).now_or_never() {
                    Some(Some(item)) => {
                        spawn_with_epoch(&mut join_set, f(item), gauge.epoch());
                    }
                    Some(None) => {
                        draining = true;
                    }
                    None => break,
                }
            }

            if !draining && join_set.len() >= gauge.value() {
                gauge.mark_saturated();
            }

            metrics.report(gauge.value(), join_set.len());

            tokio::select! {
                biased;

                Some(res) = join_set.join_next(), if !join_set.is_empty() => {
                    handle_completion(res, &mut gauge, &mut draining, &mut error, &measure_fill);
                }

                item = poll_fn(|cx| self_.as_mut().poll_next(cx)),
                    if !draining && join_set.len() < gauge.value() => {
                    match item {
                        Some(item) => {
                            spawn_with_epoch(&mut join_set, f(item), gauge.epoch());
                            if join_set.len() >= gauge.value() {
                                gauge.mark_saturated();
                            }
                        }
                        None => {
                            draining = true;
                        }
                    }
                }

                else => {
                    if join_set.is_empty() && draining {
                        break;
                    }
                }
            }

            // Drain all other ready completions before re-spawning.
            while let Some(res) = join_set.try_join_next() {
                handle_completion(res, &mut gauge, &mut draining, &mut error, &measure_fill);
            }
        }

        if let Some(e) = error { Err(e) } else { Ok(()) }
    }
}

impl<E> From<E> for Break<E> {
    fn from(e: E) -> Self {
        Break::Err(e)
    }
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
    async fn adaptive_fixed_processes_all() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned_adaptive(
                AdaptiveGauge::fixed(8),
                |i| {
                    let actual = actual.clone();
                    async move {
                        actual.fetch_add(i, Ordering::Relaxed);
                        Ok::<(), ()>(())
                    }
                },
                || 0.0,
                (),
            )
            .await;

        assert!(result.is_ok());
        let actual = actual.load(Ordering::Relaxed);
        assert_eq!(actual, 99 * 100 / 2);
    }

    #[tokio::test]
    async fn adaptive_error_propagation() {
        let result = stream::iter(0..100)
            .try_for_each_spawned_adaptive(
                AdaptiveGauge::fixed(4),
                |i| async move { if i < 42 { Ok(()) } else { Err(()) } },
                || 0.0,
                (),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adaptive_gauge_decreases_on_high_fill() {
        let mut gauge = AdaptiveGauge::adaptive(10, 1, 50);
        gauge.adjust(0, 0.9);
        assert!(gauge.value() < 10);
        assert_eq!(gauge.epoch(), 1);
    }

    #[tokio::test]
    async fn adaptive_gauge_increases_on_low_fill_when_saturated() {
        let mut gauge = AdaptiveGauge::adaptive(10, 1, 50);
        gauge.mark_saturated();
        gauge.adjust(0, 0.3);
        assert!(gauge.value() > 10);
    }

    #[tokio::test]
    async fn adaptive_gauge_holds_in_dead_zone() {
        let mut gauge = AdaptiveGauge::adaptive(10, 1, 50);
        gauge.mark_saturated();
        let before = gauge.value();
        gauge.adjust(0, 0.7);
        assert_eq!(gauge.value(), before);
    }

    #[tokio::test]
    async fn adaptive_gauge_no_increase_without_saturation() {
        let mut gauge = AdaptiveGauge::adaptive(10, 1, 50);
        let before = gauge.value();
        gauge.adjust(0, 0.3);
        assert_eq!(gauge.value(), before);
    }

    #[tokio::test]
    async fn adaptive_gauge_epoch_prevents_cascade() {
        let mut gauge = AdaptiveGauge::adaptive(10, 1, 50);
        // First decrease at epoch 0 succeeds
        gauge.adjust(0, 0.9);
        let after_first = gauge.value();
        // Second decrease with stale epoch 0 is ignored (epoch is now 1)
        gauge.adjust(0, 0.9);
        assert_eq!(gauge.value(), after_first);
    }

    #[tokio::test]
    async fn adaptive_gauge_clamps_to_min() {
        let mut gauge = AdaptiveGauge::adaptive(2, 2, 50);
        gauge.adjust(0, 0.99);
        assert!(gauge.value() >= 2);
    }

    #[tokio::test]
    async fn adaptive_gauge_clamps_to_max() {
        let mut gauge = AdaptiveGauge::adaptive(49, 1, 50);
        gauge.mark_saturated();
        gauge.adjust(0, 0.1);
        assert!(gauge.value() <= 50);
    }
}

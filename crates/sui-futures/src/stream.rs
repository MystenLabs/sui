// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, future::poll_fn, panic, pin::pin, sync::Arc};

use futures::{FutureExt, future::try_join_all, stream::Stream};
use tokio::{sync::mpsc, task::JoinSet};

/// Runtime configuration for adaptive concurrency control.
///
/// For fixed concurrency, use [`ConcurrencyConfig::fixed`] which sets `initial == min == max`.
/// For adaptive concurrency, set `min < max` and the controller will adjust the gauge based on
/// downstream channel fill fraction.
#[derive(Debug, Clone)]
pub struct ConcurrencyConfig {
    pub initial: usize,
    pub min: usize,
    pub max: usize,
    pub fill_high: f64,
    pub fill_low: f64,
}

impl ConcurrencyConfig {
    pub fn fixed(n: usize) -> Self {
        Self {
            initial: n,
            min: n,
            max: n,
            fill_high: 0.85,
            fill_low: 0.6,
        }
    }

    pub fn is_adaptive(&self) -> bool {
        self.min != self.max
    }
}

/// Abstraction over single-channel and broadcast sending.
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

/// Single-channel sender for the processor use case.
struct SingleSender<T>(mpsc::Sender<T>);

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

/// Broadcast sender that clones the value to all channels.
struct BroadcastSender<T>(Arc<Vec<mpsc::Sender<T>>>);

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Clone + Send + Sync + 'static> Sender for BroadcastSender<T> {
    type Value = T;

    async fn send(&self, value: T) -> Result<(), ()> {
        try_join_all(self.0.iter().map(|tx| {
            let v = value.clone();
            async move { tx.send(v).await.map_err(|_| ()) }
        }))
        .await?;
        Ok(())
    }

    fn fill(&self) -> f64 {
        self.0
            .iter()
            .map(|tx| 1.0 - (tx.capacity() as f64 / tx.max_capacity() as f64))
            .fold(0.0f64, f64::max)
    }
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
    /// the gauge never changes; for adaptive configs, the gauge adjusts based on the fill fraction
    /// of the output channel.
    ///
    /// The `report` callback is invoked each iteration with `(gauge, inflight)` for metrics.
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
        R: Fn(usize, usize);

    /// Process each stream item through a spawned task, broadcasting results to multiple channels.
    ///
    /// Same as [`try_for_each_send_spawned`](TrySpawnStreamExt::try_for_each_send_spawned) but
    /// sends a clone of each result to every channel in `txs`. Fill fraction is measured as the
    /// maximum across all channels.
    fn try_for_each_broadcast_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        txs: Arc<Vec<mpsc::Sender<T>>>,
        report: R,
    ) -> impl Future<Output = Result<(), Break<E>>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Clone + Send + Sync + 'static,
        E: Send + 'static,
        R: Fn(usize, usize);
}

/// Wrapper type for errors to allow the body of a `try_for_each_spawned` call to signal that it
/// either wants to return early (`Break`) out of the loop, or propagate an error (`Err(E)`).
#[derive(Debug)]
pub enum Break<E> {
    Break,
    Err(E),
}

/// Adjust the concurrency gauge based on downstream channel fill fraction.
///
/// Uses a fill-proportional controller with a dead band:
/// - `fill >= fill_high`: proportional decrease (severity scales with congestion)
/// - `fill < fill_low` and gauge was saturated: sqrt-scaled increase
/// - `fill in [fill_low, fill_high)`: dead zone, hold steady
///
/// An epoch counter prevents cascading reductions: each task captures the epoch at spawn time, and
/// a decrease is only applied when the completing task's epoch matches the current one.
fn adjust_gauge(
    spawn_epoch: u64,
    fill: f64,
    config: &ConcurrencyConfig,
    gauge: &mut usize,
    epoch: &mut u64,
    was_saturated: &mut bool,
) {
    if fill >= config.fill_high && spawn_epoch == *epoch {
        *gauge = ((*gauge as f64) * (1.0 - fill / 2.0)).ceil() as usize;
        *gauge = (*gauge).clamp(config.min, config.max);
        *epoch += 1;
        *was_saturated = false;
    } else if fill < config.fill_low && *was_saturated {
        let increment = ((*gauge as f64).sqrt().ceil() as usize).max(1);
        *gauge = (*gauge + increment).min(config.max);
        *was_saturated = false;
    }
}

/// Shared adaptive concurrency loop used by both `try_for_each_send_spawned` and
/// `try_for_each_broadcast_spawned`.
async fn adaptive_for_each_spawned<S, Fut, F, E, Tx, R>(
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
    R: Fn(usize, usize),
{
    let is_adaptive = config.is_adaptive();
    let mut gauge = config.initial;
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

        // Eager inner loop: spawn tasks while under the gauge and items are ready.
        while tasks.len() < gauge && !stream_done && error.is_none() {
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
                    if tasks.len() >= gauge {
                        was_saturated = true;
                    }
                }
                Some(None) => stream_done = true,
                None => break,
            }
        }

        report(gauge, tasks.len());

        tokio::select! {
            biased;

            Some(join_result) = tasks.join_next(), if !tasks.is_empty() => {
                match join_result {
                    Ok(Ok(spawn_epoch)) => {
                        if is_adaptive {
                            let fill = sender.fill();
                            adjust_gauge(
                                spawn_epoch, fill, &config,
                                &mut gauge, &mut epoch, &mut was_saturated,
                            );
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

            next = poll_fn(|cx| stream.as_mut().poll_next(cx)),
                if tasks.len() < gauge && !stream_done && error.is_none() =>
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
                    if tasks.len() >= gauge {
                        was_saturated = true;
                    }
                } else {
                    stream_done = true;
                }
            }

            else => {
                // All select branches are disabled: tasks drained and stream not ready
                // or gauge reached zero. Re-check the loop termination condition.
                if tasks.is_empty() && (stream_done || error.is_some()) {
                    break;
                }
            }
        }

        // Drain all other ready completions before re-looping.
        while let Some(join_result) = tasks.try_join_next() {
            match join_result {
                Ok(Ok(spawn_epoch)) => {
                    if is_adaptive {
                        let fill = sender.fill();
                        adjust_gauge(
                            spawn_epoch,
                            fill,
                            &config,
                            &mut gauge,
                            &mut epoch,
                            &mut was_saturated,
                        );
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
    }

    if let Some(e) = error { Err(e) } else { Ok(()) }
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
        R: Fn(usize, usize),
    {
        adaptive_for_each_spawned(self, config, f, SingleSender(tx), report).await
    }

    async fn try_for_each_broadcast_spawned<Fut, F, T, E, R>(
        self,
        config: ConcurrencyConfig,
        f: F,
        txs: Arc<Vec<mpsc::Sender<T>>>,
        report: R,
    ) -> Result<(), Break<E>>
    where
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        T: Clone + Send + Sync + 'static,
        E: Send + 'static,
        R: Fn(usize, usize),
    {
        adaptive_for_each_spawned(self, config, f, BroadcastSender(txs), report).await
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
    async fn map_spawned_basic() {
        let (tx, mut rx) = mpsc::channel(100);
        let result = stream::iter(0..10u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(4),
                |i| async move { Ok::<_, Break<()>>(i * 2) },
                tx,
                |_, _| {},
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
    async fn map_spawned_error_propagation() {
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
                |_, _| {},
            )
            .await;

        assert!(matches!(result, Err(Break::Err(ref s)) if s == "fail"));
    }

    #[tokio::test]
    async fn map_spawned_channel_closed() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        let result: Result<(), Break<()>> = stream::iter(0..10u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(1),
                |i| async move { Ok(i) },
                tx,
                |_, _| {},
            )
            .await;

        assert!(matches!(result, Err(Break::Break)));
    }

    #[tokio::test]
    async fn map_spawned_reports_gauge() {
        let reported = Arc::new(Mutex::new(Vec::new()));
        let (tx, _rx) = mpsc::channel(100);

        let reported2 = reported.clone();
        let _ = stream::iter(0..5u64)
            .try_for_each_send_spawned(
                ConcurrencyConfig::fixed(2),
                |i| async move { Ok::<_, Break<()>>(i) },
                tx,
                move |gauge, inflight| {
                    reported2.lock().unwrap().push((gauge, inflight));
                },
            )
            .await;

        let reports = reported.lock().unwrap();
        for &(gauge, _) in reports.iter() {
            assert_eq!(gauge, 2);
        }
    }

    #[tokio::test]
    async fn broadcast_spawned_basic() {
        let (tx1, mut rx1) = mpsc::channel(100);
        let (tx2, mut rx2) = mpsc::channel(100);
        let txs = Arc::new(vec![tx1, tx2]);

        let result = stream::iter(0..5u64)
            .try_for_each_broadcast_spawned(
                ConcurrencyConfig::fixed(2),
                |i| async move { Ok::<_, Break<()>>(i * 3) },
                txs,
                |_, _| {},
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
    async fn map_spawned_adaptive_decreases_gauge() {
        // Use a small channel to force fill to rise quickly.
        let (tx, mut rx) = mpsc::channel(4);
        let gauges = Arc::new(Mutex::new(Vec::new()));

        let gauges2 = gauges.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..100u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig {
                        initial: 10,
                        min: 1,
                        max: 20,
                        fill_high: 0.5,
                        fill_low: 0.2,
                    },
                    |i| async move {
                        // Simulate work so tasks take time
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        Ok::<_, Break<()>>(i)
                    },
                    tx,
                    move |gauge, _| {
                        gauges2.lock().unwrap().push(gauge);
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

        let gauges = gauges.lock().unwrap();
        // With a slow consumer and small channel, the gauge should have decreased
        // at some point below the initial value of 10
        let min_gauge = gauges.iter().copied().min().unwrap_or(10);
        assert!(
            min_gauge < 10,
            "Gauge should have decreased from initial=10, min observed: {min_gauge}"
        );
    }

    #[tokio::test]
    async fn fixed_concurrency_gauge_never_changes() {
        let gauges = Arc::new(Mutex::new(Vec::new()));
        let (tx, mut rx) = mpsc::channel(2);

        let gauges2 = gauges.clone();
        let handle = tokio::spawn(async move {
            stream::iter(0..20u64)
                .try_for_each_send_spawned(
                    ConcurrencyConfig::fixed(5),
                    |i| async move { Ok::<_, Break<()>>(i) },
                    tx,
                    move |gauge, _| {
                        gauges2.lock().unwrap().push(gauge);
                    },
                )
                .await
        });

        // Drain the receiver so sends don't block.
        while rx.recv().await.is_some() {}

        handle.await.unwrap().unwrap();

        let gauges = gauges.lock().unwrap();
        for &g in gauges.iter() {
            assert_eq!(g, 5, "Fixed gauge should never change");
        }
    }
}

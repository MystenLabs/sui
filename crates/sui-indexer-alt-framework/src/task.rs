// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, panic, pin::pin, time::Duration};

use futures::stream::{Stream, StreamExt};
use tokio::{task::JoinSet, time::sleep};

/// Extension trait introducing `try_for_each_spawned` to all streams.
pub trait TrySpawnStreamExt: Stream {
    /// Attempts to run this stream to completion, executing the provided asynchronous closure on
    /// each element from the stream as elements become available.
    ///
    /// This is similar to [StreamExt::for_each_concurrent], but it may take advantage of any
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
            tokio::select! {
                next = self_.next(), if !draining && permits > 0 => {
                    if let Some(item) = next {
                        permits -= 1;
                        join_set.spawn(f(item));
                    } else {
                        // If the stream is empty, signal that the worker pool is going to
                        // start draining now, so that once we get all our permits back, we
                        // know we can wind down the pool.
                        draining = true;
                    }
                }

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

                else => {
                    // Not accepting any more items from the stream, and all our workers are
                    // idle, so we stop.
                    if permits == limit && draining {
                        break;
                    }
                }
            }
        }

        if let Some(e) = error {
            Err(e)
        } else {
            Ok(())
        }
    }
}

/// Wraps a future with slow/stuck detection using `tokio::select!`
///
/// This implementation races the future against a timer. If the timer expires first, the callback
/// is executed (exactly once) but the future continues to run. This approach can detect stuck
/// futures that never wake their waker.
pub async fn with_slow_future_monitor<F, C>(
    future: F,
    threshold: Duration,
    callback: C,
) -> F::Output
where
    F: Future,
    C: FnOnce(),
{
    // The select! macro needs to take a reference to the future, which requires it to be pinned
    tokio::pin!(future);

    tokio::select! {
        result = &mut future => {
            // Future completed before timeout
            return result;
        }
        _ = sleep(threshold) => {
            // Timeout elapsed - fire the warning
            callback();
        }
    }

    // If we get here, the timeout fired but the future is still running. Continue waiting for the
    // future to complete
    future.await
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use futures::stream;
    use tokio::time::timeout;

    use super::*;

    #[derive(Clone)]
    struct Counter(Arc<AtomicUsize>);

    impl Counter {
        fn new() -> Self {
            Self(Arc::new(AtomicUsize::new(0)))
        }

        fn increment(&self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn count(&self) -> usize {
            self.0.load(Ordering::Relaxed)
        }
    }

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
    async fn slow_monitor_callback_called_once_when_threshold_exceeded() {
        let c = Counter::new();

        let result = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(200)).await;
                42 // Return a value to verify completion
            },
            Duration::from_millis(100),
            || c.increment(),
        )
        .await;

        assert_eq!(c.count(), 1);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn slow_monitor_callback_not_called_when_threshold_not_exceeded() {
        let c = Counter::new();

        let result = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(50)).await;
                42 // Return a value to verify completion
            },
            Duration::from_millis(200),
            || c.increment(),
        )
        .await;

        assert_eq!(c.count(), 0);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn slow_monitor_error_propagation() {
        let c = Counter::new();

        let result: Result<i32, &str> = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(150)).await;
                Err("Something went wrong")
            },
            Duration::from_millis(100),
            || c.increment(),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Something went wrong");
        assert_eq!(c.count(), 1);
    }

    #[tokio::test]
    async fn slow_monitor_error_propagation_without_callback() {
        let c = Counter::new();

        let result: Result<i32, &str> = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(50)).await;
                Err("Quick error")
            },
            Duration::from_millis(200),
            || c.increment(),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quick error");
        assert_eq!(c.count(), 0);
    }

    #[tokio::test]
    async fn slow_monitor_stuck_future_detection() {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        // A future that returns Pending but never wakes the waker
        struct StuckFuture;
        impl Future for StuckFuture {
            type Output = ();
            fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
                Poll::Pending
            }
        }

        let c = Counter::new();

        // Even though StuckFuture never wakes, our monitor will detect it!
        let monitored =
            with_slow_future_monitor(StuckFuture, Duration::from_millis(200), || c.increment());

        // Use a timeout to prevent the test from hanging
        timeout(Duration::from_secs(2), monitored)
            .await
            .unwrap_err();
        assert_eq!(c.count(), 1);
    }
}

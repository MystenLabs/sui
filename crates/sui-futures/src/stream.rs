// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, panic, pin::pin};

use futures::stream::{Stream, StreamExt};
use tokio::sync::watch;
use tokio::task::JoinSet;

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

    /// Like [`try_for_each_spawned`](TrySpawnStreamExt::try_for_each_spawned), but the
    /// concurrency limit is dynamic and controlled by a `watch::Receiver<usize>`.
    ///
    /// When the limit changes (via the watch channel), the loop re-evaluates whether it can
    /// spawn new tasks. Decreasing the limit will not abort in-flight tasks, but will prevent
    /// new ones from spawning until enough complete.
    fn try_for_each_spawned_dynamic<Fut, F, E>(
        self,
        limit: watch::Receiver<usize>,
        f: F,
    ) -> impl Future<Output = Result<(), E>>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static;
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

        if let Some(e) = error { Err(e) } else { Ok(()) }
    }

    async fn try_for_each_spawned_dynamic<Fut, F, E>(
        self,
        mut limit: watch::Receiver<usize>,
        mut f: F,
    ) -> Result<(), E>
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        F: FnMut(Self::Item) -> Fut,
        E: Send + 'static,
    {
        let mut active: usize = 0;
        let mut join_set = JoinSet::new();
        let mut draining = false;
        let mut error = None;

        let mut self_ = pin!(self);

        loop {
            let current_limit = *limit.borrow();
            let can_spawn = !draining && active < current_limit;

            tokio::select! {
                next = self_.next(), if can_spawn => {
                    if let Some(item) = next {
                        active += 1;
                        join_set.spawn(f(item));
                    } else {
                        draining = true;
                    }
                }

                Some(res) = join_set.join_next() => {
                    active -= 1;
                    match res {
                        Ok(Err(e)) if error.is_none() => {
                            error = Some(e);
                            draining = true;
                        }

                        Ok(_) => {}

                        Err(e) if e.is_panic() => {
                            panic::resume_unwind(e.into_panic())
                        }

                        Err(e) => {
                            assert!(e.is_cancelled());
                            draining = true;
                        }
                    }
                }

                // Re-evaluate the select guards when the limit changes, which may
                // unblock spawning if the limit increased.
                Ok(()) = limit.changed(), if !draining && active >= current_limit => {}

                else => {
                    if active == 0 && draining {
                        break;
                    }
                }
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

    // ---- dynamic concurrency tests ----

    #[tokio::test]
    async fn dynamic_static_limit_behaves_like_static() {
        let (_tx, rx) = watch::channel(16usize);
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned_dynamic(rx, |i| {
                let actual = actual.clone();
                async move {
                    actual.fetch_add(i, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());
        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        assert_eq!(actual, 99 * 100 / 2);
    }

    #[tokio::test]
    async fn dynamic_max_concurrency() {
        #[derive(Default, Debug)]
        struct Jobs {
            max: AtomicUsize,
            curr: AtomicUsize,
        }

        let (_tx, rx) = watch::channel(4usize);
        let jobs = Arc::new(Jobs::default());

        let result = stream::iter(0..32)
            .try_for_each_spawned_dynamic(rx, |_| {
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
    async fn dynamic_decrease_stops_new_spawns() {
        #[derive(Default, Debug)]
        struct Jobs {
            max: AtomicUsize,
            curr: AtomicUsize,
        }

        let (tx, rx) = watch::channel(8usize);
        let jobs = Arc::new(Jobs::default());

        let result = stream::iter(0..32)
            .try_for_each_spawned_dynamic(rx, |i| {
                let jobs = jobs.clone();
                let tx = tx.clone();
                async move {
                    // After a few items, reduce the limit
                    if i == 4 {
                        let _ = tx.send(2);
                    }
                    jobs.curr.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let prev = jobs.curr.fetch_sub(1, Ordering::Relaxed);
                    jobs.max.fetch_max(prev, Ordering::Relaxed);
                    Ok::<(), ()>(())
                }
            })
            .await;

        assert!(result.is_ok());

        let Jobs { max, curr } = Arc::try_unwrap(jobs).unwrap();
        assert_eq!(curr.into_inner(), 0);
        // The max may have been up to 8 before the decrease, but should not exceed 8
        assert!(max.into_inner() <= 8);
    }

    #[tokio::test]
    async fn dynamic_error_propagation() {
        let (_tx, rx) = watch::channel(4usize);
        let actual = Arc::new(Mutex::new(vec![]));
        let result = stream::iter(0..100)
            .try_for_each_spawned_dynamic(rx, |i| {
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
}

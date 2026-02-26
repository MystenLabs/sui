// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::{Future, poll_fn};
use std::panic;
use std::pin::pin;

use futures::FutureExt;
use futures::stream::Stream;
use tokio::task::JoinSet;

use crate::{Limiter, Outcome};

/// Error type for concurrency-limited stream operations.
///
/// Variants control both error propagation and limiter feedback:
/// - `Break`: clean early exit, no error propagated.
/// - `Dropped(E)`: overload signal (timeout, rejection) — records `Outcome::Dropped`.
/// - `Err(E)`: other failure — records `Outcome::Ignore`.
pub enum Error<E> {
    Break,
    Dropped(E),
    Err(E),
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Self {
        Error::Err(e)
    }
}

/// Extension trait introducing concurrency-limited stream processing backed by a [`Limiter`].
///
/// Each spawned task acquires a limiter token, runs the closure, and records the outcome
/// (success, dropped, or ignore) as a sample for the limiter algorithm.
pub trait ConcurrencyLimitedStreamExt: Stream {
    /// Runs this stream to completion with dynamic concurrency.
    ///
    /// Each spawned task acquires a token, runs `f`, and records the sample.
    ///
    /// Outcome mapping:
    /// - `Ok(())` → `Outcome::Success`
    /// - `Err(Error::Dropped(_))` → `Outcome::Dropped`
    /// - `Err(Error::Err(_))` → `Outcome::Ignore`
    /// - `Err(Error::Break)` → `Outcome::Ignore`
    fn try_for_each_spawned<F, Fut, E>(
        self,
        limiter: Limiter,
        f: F,
    ) -> impl Future<Output = Result<(), Error<E>>>
    where
        F: FnMut(Self::Item) -> Fut,
        Fut: Future<Output = Result<(), Error<E>>> + Send + 'static,
        E: Send + 'static;
}

impl<S: Stream + Sized + 'static> ConcurrencyLimitedStreamExt for S {
    async fn try_for_each_spawned<F, Fut, E>(
        self,
        limiter: Limiter,
        mut f: F,
    ) -> Result<(), Error<E>>
    where
        F: FnMut(Self::Item) -> Fut,
        Fut: Future<Output = Result<(), Error<E>>> + Send + 'static,
        E: Send + 'static,
    {
        let mut active: usize = 0;
        let mut join_set = JoinSet::new();
        let mut draining = false;
        let mut error = None;

        let mut self_ = pin!(self);

        loop {
            // Eager inner loop: spawn tasks while the limit allows and items are
            // ready, avoiding select! overhead when items are immediately available.
            while !draining && active < limiter.current() {
                match poll_fn(|cx| self_.as_mut().poll_next(cx)).now_or_never() {
                    Some(Some(item)) => {
                        active += 1;
                        let fut = f(item);
                        let token = limiter.acquire();

                        join_set.spawn(async move {
                            match fut.await {
                                Ok(()) => {
                                    token.record_sample(Outcome::Success);
                                    Ok(())
                                }
                                Err(Error::Dropped(e)) => {
                                    token.record_sample(Outcome::Dropped);
                                    Err(Error::Dropped(e))
                                }
                                Err(e) => {
                                    token.record_sample(Outcome::Ignore);
                                    Err(e)
                                }
                            }
                        });
                    }
                    Some(None) => {
                        draining = true;
                    }
                    None => break,
                }
            }

            let can_spawn = !draining && active < limiter.current();

            tokio::select! {
                biased;

                Some(res) = join_set.join_next() => {
                    active -= 1;
                    match res {
                        // Dropped: limiter already recorded the drop (via token),
                        // but the task completed — keep going with reduced limit.
                        Ok(Err(Error::Dropped(_))) => {}

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

                next = poll_fn(|cx| self_.as_mut().poll_next(cx)),
                    if can_spawn => {
                    if let Some(item) = next {
                        active += 1;
                        let fut = f(item);
                        let token = limiter.acquire();

                        join_set.spawn(async move {
                            match fut.await {
                                Ok(()) => {
                                    token.record_sample(Outcome::Success);
                                    Ok(())
                                }
                                Err(Error::Dropped(e)) => {
                                    token.record_sample(Outcome::Dropped);
                                    Err(Error::Dropped(e))
                                }
                                Err(e) => {
                                    token.record_sample(Outcome::Ignore);
                                    Err(e)
                                }
                            }
                        });
                    } else {
                        draining = true;
                    }
                }

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

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use std::sync::Arc;

    use futures::stream;

    use super::*;

    #[tokio::test]
    async fn stream_all_succeed() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned(Limiter::fixed(16), |i: usize| {
                let actual = actual.clone();
                async move {
                    actual.fetch_add(i, Ordering::Relaxed);
                    Ok::<(), Error<()>>(())
                }
            })
            .await;

        assert!(result.is_ok());
        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        assert_eq!(actual, 99 * 100 / 2);
    }

    #[tokio::test]
    async fn stream_error_stops() {
        let processed = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned(Limiter::fixed(1), |i: usize| {
                let processed = processed.clone();
                async move {
                    processed.fetch_add(1, Ordering::Relaxed);
                    if i >= 5 {
                        Err(Error::Err(format!("error at {i}")))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        assert!(matches!(result, Err(Error::Err(_))));
        assert_eq!(processed.load(Ordering::Relaxed), 6);
    }

    #[tokio::test]
    async fn stream_break_clean_shutdown() {
        let result = stream::iter(0..1)
            .try_for_each_spawned(Limiter::fixed(4), |_: usize| async move {
                Err(Error::<()>::Break)
            })
            .await;

        match result {
            Err(Error::Break) => {}
            _ => panic!("expected Break"),
        }
    }

    #[tokio::test]
    async fn stream_concurrency_limit_respected() {
        #[derive(Default, Debug)]
        struct Jobs {
            max: AtomicUsize,
            curr: AtomicUsize,
        }

        let jobs = Arc::new(Jobs::default());

        let result = stream::iter(0..32)
            .try_for_each_spawned(Limiter::fixed(4), |_: usize| {
                let jobs = jobs.clone();
                async move {
                    jobs.curr.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let prev = jobs.curr.fetch_sub(1, Ordering::Relaxed);
                    jobs.max.fetch_max(prev, Ordering::Relaxed);
                    Ok::<(), Error<()>>(())
                }
            })
            .await;

        assert!(result.is_ok());
        let Jobs { max, curr } = Arc::try_unwrap(jobs).unwrap();
        assert_eq!(curr.into_inner(), 0);
        assert!(max.into_inner() <= 4);
    }

    #[tokio::test]
    async fn stream_dropped_signals_limiter() {
        let limiter = Limiter::aimd(crate::AimdConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 20,
            ..crate::AimdConfig::default()
        });
        let initial = limiter.current();

        // Dropped is non-fatal: the stream completes successfully but the
        // limiter records the drop and reduces the concurrency limit.
        let result = stream::iter(0..1)
            .try_for_each_spawned(limiter.clone(), |_: usize| async move {
                Err(Error::Dropped("timeout"))
            })
            .await;

        assert!(result.is_ok());
        assert!(
            limiter.current() < initial,
            "Dropped should reduce AIMD limit"
        );
    }
}

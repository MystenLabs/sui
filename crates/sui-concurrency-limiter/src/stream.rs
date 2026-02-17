// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::panic;
use std::pin::pin;
use std::sync::Arc;

use futures::stream::{Stream, StreamExt};
use tokio::task::JoinSet;

use crate::{Limiter, Outcome};

/// Wrapper type for errors to allow the body of a `try_for_each_spawned` call to signal that it
/// either wants to return early (`Break`) out of the loop, or propagate an error (`Err(E)`).
pub enum Break<E> {
    Break,
    Err(E),
}

impl<E> From<E> for Break<E> {
    fn from(e: E) -> Self {
        Break::Err(e)
    }
}

/// Extension trait introducing adaptive concurrency stream methods backed by a [`Limiter`].
///
/// Each method uses a two-closure pattern:
/// - `f` (measured work): runs under a limiter token, its RTT is recorded as a sample.
/// - `g` (unmeasured work): runs after the token is consumed, so it does not affect the
///   limiter signal.
pub trait AdaptiveStreamExt: Stream {
    /// Runs this stream to completion with dynamic concurrency.
    ///
    /// Each spawned task acquires a token, runs `f` (measured), records the sample,
    /// then runs `g` (unmeasured). The token is consumed before `g` executes.
    ///
    /// Outcome mapping for `f`:
    /// - `Ok(value)` → `Outcome::Success`, then `g(value)` is called
    /// - `Err(_)` → `Outcome::Ignore`
    fn try_for_each_spawned_adaptive<F, Fut, T, G, GFut, E>(
        self,
        limiter: Limiter,
        f: F,
        g: G,
    ) -> impl Future<Output = Result<(), Break<E>>>
    where
        F: FnMut(Self::Item) -> Fut,
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        T: Send + 'static,
        G: Fn(T) -> GFut + Send + Sync + 'static,
        GFut: Future<Output = Result<(), Break<E>>> + Send + 'static,
        E: Send + 'static;
}

impl<S: Stream + Sized + 'static> AdaptiveStreamExt for S {
    async fn try_for_each_spawned_adaptive<F, Fut, T, G, GFut, E>(
        self,
        limiter: Limiter,
        mut f: F,
        g: G,
    ) -> Result<(), Break<E>>
    where
        F: FnMut(Self::Item) -> Fut,
        Fut: Future<Output = Result<T, Break<E>>> + Send + 'static,
        T: Send + 'static,
        G: Fn(T) -> GFut + Send + Sync + 'static,
        GFut: Future<Output = Result<(), Break<E>>> + Send + 'static,
        E: Send + 'static,
    {
        let g = Arc::new(g);
        let mut active: usize = 0;
        let mut join_set = JoinSet::new();
        let mut draining = false;
        let mut error = None;

        let mut self_ = pin!(self);

        loop {
            let current_limit = limiter.current();
            let can_spawn = !draining && active < current_limit;

            tokio::select! {
                next = self_.next(), if can_spawn => {
                    if let Some(item) = next {
                        active += 1;
                        let fut = f(item);
                        let limiter = limiter.clone();
                        let g = g.clone();

                        join_set.spawn(async move {
                            let token = limiter.acquire();
                            match fut.await {
                                Ok(value) => {
                                    token.record_sample(Outcome::Success);
                                    g(value).await
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
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use std::sync::Arc;

    use futures::stream;

    use super::*;

    // ---- simple adaptive stream tests ----

    #[tokio::test]
    async fn adaptive_all_succeed() {
        let actual = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned_adaptive(
                Limiter::fixed(16),
                |i: usize| {
                    let actual = actual.clone();
                    async move {
                        actual.fetch_add(i, Ordering::Relaxed);
                        Ok::<(), Break<()>>(())
                    }
                },
                |()| async { Ok(()) },
            )
            .await;

        assert!(result.is_ok());
        let actual = Arc::try_unwrap(actual).unwrap().into_inner();
        assert_eq!(actual, 99 * 100 / 2);
    }

    #[tokio::test]
    async fn adaptive_error_stops() {
        let processed = Arc::new(AtomicUsize::new(0));
        let result = stream::iter(0..100)
            .try_for_each_spawned_adaptive(
                Limiter::fixed(1),
                |i: usize| {
                    let processed = processed.clone();
                    async move {
                        processed.fetch_add(1, Ordering::Relaxed);
                        if i >= 5 {
                            Err(Break::Err(format!("error at {i}")))
                        } else {
                            Ok(())
                        }
                    }
                },
                |()| async { Ok(()) },
            )
            .await;

        assert!(matches!(result, Err(Break::Err(_))));
        assert_eq!(processed.load(Ordering::Relaxed), 6);
    }

    #[tokio::test]
    async fn adaptive_break_clean_shutdown() {
        let result = stream::iter(0..1)
            .try_for_each_spawned_adaptive(
                Limiter::fixed(4),
                |_: usize| async move { Err(Break::<()>::Break) },
                |()| async { Ok(()) },
            )
            .await;

        match result {
            Err(Break::Break) => {}
            _ => panic!("expected Break"),
        }
    }

    #[tokio::test]
    async fn adaptive_concurrency_limit_respected() {
        #[derive(Default, Debug)]
        struct Jobs {
            max: AtomicUsize,
            curr: AtomicUsize,
        }

        let jobs = Arc::new(Jobs::default());

        let result = stream::iter(0..32)
            .try_for_each_spawned_adaptive(
                Limiter::fixed(4),
                |_: usize| {
                    let jobs = jobs.clone();
                    async move {
                        jobs.curr.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        let prev = jobs.curr.fetch_sub(1, Ordering::Relaxed);
                        jobs.max.fetch_max(prev, Ordering::Relaxed);
                        Ok::<(), Break<()>>(())
                    }
                },
                |()| async { Ok(()) },
            )
            .await;

        assert!(result.is_ok());
        let Jobs { max, curr } = Arc::try_unwrap(jobs).unwrap();
        assert_eq!(curr.into_inner(), 0);
        assert!(max.into_inner() <= 4);
    }

    // ---- two-closure specific tests ----

    #[tokio::test]
    async fn adaptive_g_runs_after_sample() {
        let g_ran = Arc::new(AtomicUsize::new(0));
        let inflight_during_g = Arc::new(Mutex::new(Vec::new()));

        let limiter = Limiter::fixed(4);
        let result = stream::iter(0..1)
            .try_for_each_spawned_adaptive(
                limiter.clone(),
                |_: usize| async move { Ok::<_, Break<()>>(42) },
                {
                    let g_ran = g_ran.clone();
                    let inflight_during_g = inflight_during_g.clone();
                    let limiter = limiter.clone();
                    move |value: i32| {
                        let g_ran = g_ran.clone();
                        let inflight_during_g = inflight_during_g.clone();
                        let limiter = limiter.clone();
                        async move {
                            inflight_during_g.lock().unwrap().push(limiter.inflight());
                            g_ran.fetch_add(1, Ordering::Relaxed);
                            assert_eq!(value, 42);
                            Ok(())
                        }
                    }
                },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(g_ran.load(Ordering::Relaxed), 1);
        // Token was consumed before g ran, so inflight should be 0 during g
        let recorded = inflight_during_g.lock().unwrap().clone();
        assert_eq!(recorded, vec![0]);
    }

    #[tokio::test]
    async fn adaptive_g_error_propagates() {
        let result = stream::iter(0..1)
            .try_for_each_spawned_adaptive(
                Limiter::fixed(4),
                |_: usize| async move { Ok::<_, Break<String>>(42) },
                |_value: i32| async move { Err(Break::Err("g failed".to_string())) },
            )
            .await;

        match result {
            Err(Break::Err(e)) => assert_eq!(e, "g failed"),
            _ => panic!("expected Err from g"),
        }
    }
}

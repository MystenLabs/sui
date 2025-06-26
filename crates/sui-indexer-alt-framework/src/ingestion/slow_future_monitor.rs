// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::Instant;

// We need pin_project to safely poll the inner future from our Future implementation
pin_project! {
    /// A Future wrapper that calls a callback when the wrapped future takes too long,
    /// but continues execution without canceling the future.
    pub(crate) struct SlowFutureMonitor<F, C> {
        #[pin] inner: F,
        on_threshold_exceeded: Option<C>,
        threshold: Duration,
        start_time: Instant,
    }
}

impl<F, C> Future for SlowFutureMonitor<F, C>
where
    F: Future,
    C: FnOnce(),
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // Check if we should call the callback (only once)
        if this.on_threshold_exceeded.is_some() {
            let elapsed = this.start_time.elapsed();
            if elapsed >= *this.threshold {
                if let Some(callback) = this.on_threshold_exceeded.take() {
                    callback();
                }
            }
        }

        // Poll the inner future
        this.inner.poll(cx)
    }
}

/// Helper function to wrap a future with slow future monitoring
pub(crate) fn with_slow_future_monitor<F, C>(
    future: F,
    threshold: Duration,
    callback: C,
) -> SlowFutureMonitor<F, C>
where
    F: Future,
    C: FnOnce(),
{
    SlowFutureMonitor {
        inner: future,
        on_threshold_exceeded: Some(callback),
        threshold,
        start_time: Instant::now(),
    }
}

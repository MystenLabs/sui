// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, time::Duration};

use tokio::time::sleep;

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
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use tokio::time::{sleep, timeout};

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

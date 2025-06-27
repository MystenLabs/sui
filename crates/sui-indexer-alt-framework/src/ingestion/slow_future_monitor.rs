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
    /// but continues execution without canceling the future. The callback is guaranteed
    /// to be called once if the threshold is exceeded.
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
        let elapsed = this.start_time.elapsed();
        if elapsed >= *this.threshold {
            if let Some(callback) = this.on_threshold_exceeded.take() {
                callback();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::time::{sleep, Duration};

    // Helper to create a counter that can be shared across async boundaries
    fn new_counter() -> (Arc<Mutex<usize>>, impl Fn() + Send + 'static) {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        let increment_counter = move || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
        };
        (counter, increment_counter)
    }

    // Helper to get counter value
    fn get_count(counter: &Arc<Mutex<usize>>) -> usize {
        *counter.lock().unwrap()
    }

    #[tokio::test]
    async fn test_callback_called_once_when_threshold_exceeded() {
        let (counter, increment_counter) = new_counter();

        let monitored_future = with_slow_future_monitor(
            sleep(Duration::from_millis(200)),
            Duration::from_millis(100),
            increment_counter,
        );

        monitored_future.await;
        assert_eq!(get_count(&counter), 1);
    }

    #[tokio::test]
    async fn test_callback_not_called_when_threshold_not_exceeded() {
        let (counter, increment_counter) = new_counter();

        let monitored_future = with_slow_future_monitor(
            sleep(Duration::from_millis(50)),
            Duration::from_millis(200),
            increment_counter,
        );

        monitored_future.await;
        assert_eq!(get_count(&counter), 0);
    }

    #[tokio::test]
    async fn test_future_returns_correct_value() {
        // Create a future that returns a specific value
        let value_future = async { 1512 };
        let threshold = Duration::from_millis(100);

        let monitored_future = with_slow_future_monitor(value_future, threshold, || {
            // Callback doesn't matter for this test
        });

        // Wait for the future to complete and check the return value
        let result = monitored_future.await;
        assert_eq!(result, 1512);
    }

    #[tokio::test]
    async fn test_zero_threshold() {
        let (counter, increment_counter) = new_counter();

        let monitored_future = with_slow_future_monitor(
            async { "done" },
            Duration::from_millis(0),
            increment_counter,
        );

        let result = monitored_future.await;
        assert_eq!(get_count(&counter), 1);
        assert_eq!(result, "done");
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let (counter, increment_counter) = new_counter();

        let monitored_future = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(150)).await;
                Err("Something went wrong")
            },
            Duration::from_millis(100),
            increment_counter,
        );

        let result: Result<i32, &str> = monitored_future.await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Something went wrong");
        assert_eq!(get_count(&counter), 1);
    }

    #[tokio::test]
    async fn test_error_propagation_without_callback() {
        let (counter, increment_counter) = new_counter();

        let monitored_future = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(50)).await;
                Err("Quick error")
            },
            Duration::from_millis(200),
            increment_counter,
        );

        let result: Result<i32, &str> = monitored_future.await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quick error");
        assert_eq!(get_count(&counter), 0);
    }
}

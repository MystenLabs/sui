// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, time::Duration};
use tokio::time::sleep;

/// Wraps a future with slow/stuck detection using tokio::select!
///
/// This implementation races the future against a timer. If the timer expires first,
/// the callback is executed (exactly once) but the future continues to run.
/// This approach can detect stuck futures that never wake their waker.
pub(crate) async fn with_slow_future_monitor<F, C>(
    future: F,
    threshold: Duration,
    callback: C,
) -> F::Output
where
    F: Future,
    C: FnOnce(),
{
    // The select! macro needs to poll the future, which requires it to be pinned
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

    // If we get here, the timeout fired but the future is still running
    // Continue waiting for the future to complete
    future.await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::time::{sleep, timeout, Duration};

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

        let result = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(200)).await;
                42 // Return a value to verify completion
            },
            Duration::from_millis(100),
            increment_counter,
        )
        .await;

        assert_eq!(get_count(&counter), 1);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_callback_not_called_when_threshold_not_exceeded() {
        let (counter, increment_counter) = new_counter();

        let result = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(50)).await;
                42 // Return a value to verify completion
            },
            Duration::from_millis(200),
            increment_counter,
        )
        .await;

        assert_eq!(get_count(&counter), 0);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let (counter, increment_counter) = new_counter();

        let result: Result<i32, &str> = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(150)).await;
                Err("Something went wrong")
            },
            Duration::from_millis(100),
            increment_counter,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Something went wrong");
        assert_eq!(get_count(&counter), 1);
    }

    #[tokio::test]
    async fn test_error_propagation_without_callback() {
        let (counter, increment_counter) = new_counter();

        let result: Result<i32, &str> = with_slow_future_monitor(
            async {
                sleep(Duration::from_millis(50)).await;
                Err("Quick error")
            },
            Duration::from_millis(200),
            increment_counter,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quick error");
        assert_eq!(get_count(&counter), 0);
    }

    #[tokio::test]
    async fn test_stuck_future_detection() {
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

        let (counter, increment_counter) = new_counter();

        // Even though StuckFuture never wakes, our monitor will detect it!
        let monitored =
            with_slow_future_monitor(StuckFuture, Duration::from_millis(200), increment_counter);

        // Use a timeout to prevent the test from hanging
        match timeout(Duration::from_secs(2), monitored).await {
            Ok(_) => panic!("Stuck future should not complete"),
            Err(_) => {
                // The outer timeout fired, but our callback should have fired first
                assert_eq!(get_count(&counter), 1);
            }
        }
    }
}

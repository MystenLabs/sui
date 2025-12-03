// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{iter::Iterator, time::Duration};

use rand::Rng as _;

/// Creates a generator which yields an approximately exponential series of durations, as back-off delays.
/// Jitters are added to each delay by default to prevent thundering herd on retries.
///
/// The API is inspired by tokio-retry::strategy::ExponentialBackoff for ease of use.
/// But bugs in the original implementation have been fixed.
///
/// ```rust,no_run
/// use std::time::Duration;
/// use mysten_common::backoff::ExponentialBackoff;
///
/// // Basic example:
/// let mut backoff = ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(10));
/// for (attempt, delay) in backoff.enumerate() {
///     println!("Attempt {attempt}: Delay: {:?}", delay);
/// }
///
/// // Specifying backoff factor and maximum jitter:
/// let mut backoff = ExponentialBackoff::new(Duration::from_secs(5), Duration::from_secs(60))
///     .factor(2.0)
///     .max_jitter(Duration::from_secs(1));
/// loop {
///     // next() should always return a Some(Duration).
///     let delay = backoff.next().unwrap();
///     println!("Delay: {:?}", delay);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    next: Duration,
    factor: f64,
    max_delay: Duration,
    max_jitter: Duration,
}

impl ExponentialBackoff {
    /// Constructs a new exponential backoff generator, specifying the maximum delay.
    pub fn new(initial_delay: Duration, max_delay: Duration) -> ExponentialBackoff {
        ExponentialBackoff {
            next: initial_delay,
            factor: 1.2,
            max_delay,
            max_jitter: initial_delay,
        }
    }

    /// Sets the approximate ratio of consecutive backoff delays, before jitters are applied.
    /// Setting this to Duration::ZERO disables jittering.
    ///
    /// Default is 1.2.
    pub fn factor(mut self, factor: f64) -> ExponentialBackoff {
        self.factor = factor;
        self
    }

    /// Sets the maximum jitter per delay.
    ///
    /// Default is the initial delay.
    pub fn max_jitter(mut self, max_jitter: Duration) -> ExponentialBackoff {
        self.max_jitter = max_jitter;
        self
    }
}

impl Iterator for ExponentialBackoff {
    type Item = Duration;

    /// Yields backoff delays. Never terminates.
    fn next(&mut self) -> Option<Duration> {
        let current = self.next;

        let jitter = if self.max_jitter.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(
                rand::thread_rng().gen_range(0.0..self.max_jitter.as_secs_f64()),
            )
        };
        self.next = current
            .mul_f64(self.factor)
            .min(self.max_delay)
            .saturating_add(jitter);

        Some(current)
    }
}

#[test]
fn test_exponential_backoff_default() {
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(50), Duration::from_secs(10));

    let bounds = vec![
        (Duration::from_millis(50), Duration::from_millis(100)),
        (Duration::from_millis(60), Duration::from_millis(170)),
    ];
    for ((lower, upper), delay) in bounds.into_iter().zip(backoff.next()) {
        assert!(delay >= lower && delay <= upper);
    }
}

#[test]
fn test_exponential_backoff_base_100_factor_2_no_jitter() {
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(10))
        .factor(2.0)
        .max_jitter(Duration::ZERO);

    assert_eq!(backoff.next(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next(), Some(Duration::from_millis(200)));
    assert_eq!(backoff.next(), Some(Duration::from_millis(400)));
    assert_eq!(backoff.next(), Some(Duration::from_millis(800)));
}

#[test]
fn test_exponential_backoff_max_delay() {
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(200), Duration::from_secs(1))
        .factor(3.0)
        .max_jitter(Duration::ZERO);

    assert_eq!(backoff.next(), Some(Duration::from_millis(200)));
    assert_eq!(backoff.next(), Some(Duration::from_millis(600)));

    for _ in 0..10 {
        assert_eq!(backoff.next(), Some(Duration::from_secs(1)));
    }
}

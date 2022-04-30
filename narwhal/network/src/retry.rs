// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, time::Duration};

/// Retry configurations for establishing connections and sending messages.
/// Determines the retry behaviour of requests, by setting the back off strategy used.
#[derive(Clone, Debug, Copy)]
pub struct RetryConfig {
    /// The initial retry interval.
    ///
    /// This is the first delay before a retry, for establishing connections and sending messages.
    /// The subsequent delay will be decided by the `retry_delay_multiplier`.
    pub initial_retry_interval: Duration,

    /// The maximum value of the back off period. Once the retry interval reaches this
    /// value it stops increasing.
    ///
    /// This is the longest duration we will have,
    /// for establishing connections and sending messages.
    /// Retrying continues even after the duration times have reached this duration.
    /// The number of retries before that happens, will be decided by the `retry_delay_multiplier`.
    /// The number of retries after that, will be decided by the `retrying_max_elapsed_time`.
    pub max_retry_interval: Duration,

    /// The value to multiply the current interval with for each retry attempt.
    pub retry_delay_multiplier: f64,

    /// The randomization factor to use for creating a range around the retry interval.
    ///
    /// A randomization factor of 0.5 results in a random period ranging between 50% below and 50%
    /// above the retry interval.
    pub retry_delay_rand_factor: f64,

    /// The maximum elapsed time after instantiating
    ///
    /// Retrying continues until this time has elapsed.
    /// The number of retries before that happens, will be decided by the other retry config options.
    pub retrying_max_elapsed_time: Option<Duration>,
}

impl RetryConfig {
    // Together with the default max and multiplier,
    // default gives 5-6 retries in ~30 s total retry time.

    /// Default for [`RetryConfig::max_retry_interval`] (500 ms).
    pub const DEFAULT_INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

    /// Default for [`RetryConfig::max_retry_interval`] (15 s).
    pub const DEFAULT_MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);

    /// Default for [`RetryConfig::retry_delay_multiplier`] (x1.5).
    pub const DEFAULT_RETRY_INTERVAL_MULTIPLIER: f64 = 1.5;

    /// Default for [`RetryConfig::retry_delay_rand_factor`] (0.3).
    pub const DEFAULT_RETRY_DELAY_RAND_FACTOR: f64 = 0.3;

    /// Default for [`RetryConfig::retrying_max_elapsed_time`] (30 s).
    pub const DEFAULT_RETRYING_MAX_ELAPSED_TIME: Duration = Duration::from_secs(30);

    // Perform `op` and retry on errors as specified by this configuration.
    //
    // Note that `backoff::Error<E>` implements `From<E>` for any `E` by creating a
    // `backoff::Error::Transient`, meaning that errors will be retried unless explicitly returning
    // `backoff::Error::Permanent`.
    pub fn retry<R, E, Fn, Fut>(self, op: Fn) -> impl Future<Output = Result<R, E>>
    where
        Fn: FnMut() -> Fut,
        Fut: Future<Output = Result<R, backoff::Error<E>>>,
    {
        let backoff = backoff::ExponentialBackoff {
            initial_interval: self.initial_retry_interval,
            randomization_factor: self.retry_delay_rand_factor,
            multiplier: self.retry_delay_multiplier,
            max_interval: self.max_retry_interval,
            max_elapsed_time: self.retrying_max_elapsed_time,
            ..Default::default()
        };
        backoff::future::retry(backoff, op)
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_retry_interval: RetryConfig::DEFAULT_INITIAL_RETRY_INTERVAL,
            max_retry_interval: RetryConfig::DEFAULT_MAX_RETRY_INTERVAL,
            retry_delay_multiplier: RetryConfig::DEFAULT_RETRY_INTERVAL_MULTIPLIER,
            retry_delay_rand_factor: RetryConfig::DEFAULT_RETRY_DELAY_RAND_FACTOR,
            retrying_max_elapsed_time: Some(RetryConfig::DEFAULT_RETRYING_MAX_ELAPSED_TIME),
        }
    }
}

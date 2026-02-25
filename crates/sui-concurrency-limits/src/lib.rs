// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Dynamic concurrency limiters based on Netflix's
//! [concurrency-limits](https://github.com/Netflix/concurrency-limits) library.
//!
//! Two algorithms are provided:
//!
//! - **AIMD** (`Aimd`): loss-based. Additive increase on success, multiplicative decrease on
//!   drop. Simple and effective when the backing store signals overload via errors/throttling
//!   rather than latency degradation (e.g. GCS returning HTTP 429).
//!
//! - **Gradient** (`Gradient`, based on Netflix's Gradient2): latency-based. Computes a gradient
//!   from the ratio of long-term to short-term RTT and scales the limit proportionally. Effective
//!   when the backing store degrades gradually under load (e.g. Bigtable write latency increasing).
//!
//! # Differences from Netflix's reference implementation
//!
//! **Architecture.** Netflix's library couples inflight tracking into each algorithm. We split it:
//! each [`Algorithm`] variant computes the new limit and returns it, while [`Limiter`] owns the
//! shared [`AtomicUsize`] gauge and handles inflight counting via a separate atomic. This avoids
//! duplicating acquire/release across algorithms and keeps inflight on the fastest possible path
//! (a single atomic) since it's called by 10k+ concurrent futures. The [`Token`] RAII guard
//! captures inflight at acquire time and passes it to `Algorithm::update` on sample, matching
//! Netflix's `AbstractLimiter.createListener()` which snapshots `inFlight.incrementAndGet()` at
//! request start.
//!
//! All Gradient parameters (`smoothing`, `tolerance`, `long_window`, `queue_size`, EMA warmup,
//! defaults) match Netflix's Gradient2.

pub mod stream;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Serde helper for `Option<Duration>` as fractional seconds.
mod option_duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(val: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        val.map(|d| d.as_secs_f64()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Option::<f64>::deserialize(d).map(|o| o.map(Duration::from_secs_f64))
    }
}

fn default_aimd_timeout() -> Option<Duration> {
    Some(Duration::from_secs(5))
}

/// Outcome of a concurrency-limited operation.
///
/// Three signals inspired by Netflix's concurrency-limits library:
/// - `Success`: completed normally, algorithm may increase the limit.
/// - `Dropped`: failed or timed out, algorithm decreases the limit.
/// - `Ignore`: ambiguous result, algorithm makes no adjustment.
pub enum Outcome {
    Success,
    Dropped,
    Ignore,
}

type QueueSizeFn = dyn Fn(usize) -> usize + Send + Sync + 'static;

// ---------------------------------------------------------------------------
// Algorithm enum — replaces the former LimitAlgorithm trait
// ---------------------------------------------------------------------------

enum Algorithm {
    Fixed { limit: usize },
    Aimd(Aimd),
    Gradient(Gradient),
}

impl Algorithm {
    /// Returns the new limit value. Caller writes to the shared gauge.
    fn update(&self, inflight: usize, outcome: Outcome, rtt: Duration) -> usize {
        match self {
            Self::Fixed { limit } => *limit,
            Self::Aimd(a) => a.update(inflight, outcome, rtt),
            Self::Gradient(g) => g.update(inflight, outcome, rtt),
        }
    }
}

// ---------------------------------------------------------------------------
// Limiter
// ---------------------------------------------------------------------------

/// Shared state between [`Limiter`] and [`Token`].
struct LimiterInner {
    algorithm: Algorithm,
    gauge: AtomicUsize,
    inflight: AtomicUsize,
    peak_inflight: AtomicUsize,
    peak_limit: AtomicUsize,
    clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>,
    on_limit_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

/// Cloneable handle wrapping a dynamic concurrency limit algorithm.
///
/// This is the user-facing API for concurrency limiting. Call [`Limiter::acquire`] to obtain a
/// [`Token`] that automatically releases the inflight slot on drop.
#[derive(Clone)]
pub struct Limiter(Arc<LimiterInner>);

/// Builder for [`Limiter`], allowing clock and callback injection before construction.
pub struct LimiterBuilder {
    algorithm: Algorithm,
    initial_limit: usize,
    clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>,
    on_limit_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl LimiterBuilder {
    /// Attach a custom monotonic clock for RTT measurement.
    pub fn clock(mut self, f: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        self.clock = Some(Arc::new(f));
        self
    }

    /// Attach a callback invoked whenever the concurrency limit changes.
    ///
    /// The callback receives the new limit value. Useful for exporting the limit as a metric
    /// without coupling this crate to a specific metrics library.
    pub fn on_limit_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_limit_change = Some(Arc::new(f));
        self
    }

    /// Build the [`Limiter`].
    pub fn build(self) -> Limiter {
        Limiter(Arc::new(LimiterInner {
            algorithm: self.algorithm,
            gauge: AtomicUsize::new(self.initial_limit),
            inflight: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(self.initial_limit),
            clock: self.clock,
            on_limit_change: self.on_limit_change,
        }))
    }
}

impl Limiter {
    pub fn fixed(limit: usize) -> Self {
        LimiterBuilder {
            algorithm: Algorithm::Fixed { limit },
            initial_limit: limit,
            clock: None,
            on_limit_change: None,
        }
        .build()
    }

    pub fn aimd(config: AimdConfig) -> Self {
        Self::builder_aimd(config).build()
    }

    /// Return a [`LimiterBuilder`] pre-configured with an AIMD algorithm.
    pub fn builder_aimd(config: AimdConfig) -> LimiterBuilder {
        assert!(
            config.backoff_ratio < 1.0 && config.backoff_ratio >= 0.5,
            "backoff_ratio must be in [0.5, 1.0)"
        );
        assert!(
            config.timeout.is_none_or(|t| t > Duration::ZERO),
            "timeout must be positive"
        );
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        LimiterBuilder {
            algorithm: Algorithm::Aimd(Aimd::new(&config)),
            initial_limit: initial,
            clock: None,
            on_limit_change: None,
        }
    }

    pub fn gradient(config: GradientConfig) -> Self {
        Self::builder_gradient(config).build()
    }

    /// Return a [`LimiterBuilder`] pre-configured with a Gradient algorithm.
    pub fn builder_gradient(config: GradientConfig) -> LimiterBuilder {
        assert!(config.tolerance >= 1.0, "tolerance must be >= 1.0");
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        LimiterBuilder {
            algorithm: Algorithm::Gradient(Gradient::new(&config)),
            initial_limit: initial,
            clock: None,
            on_limit_change: None,
        }
    }

    /// Build a Gradient limiter with a dynamic queue-size function.
    ///
    /// The callback receives the current estimated limit and returns the additive queue term.
    pub fn gradient_with_queue_size_fn<F>(config: GradientConfig, queue_size_fn: F) -> Self
    where
        F: Fn(usize) -> usize + Send + Sync + 'static,
    {
        Self::builder_gradient_with_queue_size_fn(config, queue_size_fn).build()
    }

    /// Return a [`LimiterBuilder`] pre-configured with a Gradient algorithm and a dynamic
    /// queue-size function.
    pub fn builder_gradient_with_queue_size_fn<F>(
        config: GradientConfig,
        queue_size_fn: F,
    ) -> LimiterBuilder
    where
        F: Fn(usize) -> usize + Send + Sync + 'static,
    {
        assert!(config.tolerance >= 1.0, "tolerance must be >= 1.0");
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        LimiterBuilder {
            algorithm: Algorithm::Gradient(Gradient::new_with_queue_size_fn(
                &config,
                Arc::new(queue_size_fn),
            )),
            initial_limit: initial,
            clock: None,
            on_limit_change: None,
        }
    }

    /// Acquire an inflight slot, returning an RAII [`Token`] that releases it on drop.
    ///
    /// The current inflight count (after incrementing) is captured in the token so that
    /// [`Token::record_sample`] passes the load at request start, not at completion —
    /// matching Netflix's AbstractLimiter behavior.
    pub fn acquire(&self) -> Token {
        let inflight = self.0.inflight.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.peak_inflight.fetch_max(inflight, Ordering::Relaxed);
        let start = match &self.0.clock {
            Some(f) => f(),
            None => Instant::now(),
        };
        Token {
            inner: Some(self.0.clone()),
            inflight,
            start,
        }
    }

    /// Returns the current concurrency limit.
    pub fn current(&self) -> usize {
        self.0.gauge.load(Ordering::Acquire)
    }

    /// Returns the current number of inflight operations.
    pub fn inflight(&self) -> usize {
        self.0.inflight.load(Ordering::Relaxed)
    }

    /// Returns the peak inflight count since the last call, resetting it to the current inflight
    /// so the next interval's peak starts from the right baseline.
    pub fn take_peak_inflight(&self) -> usize {
        let current = self.0.inflight.load(Ordering::Relaxed);
        self.0.peak_inflight.swap(current, Ordering::Relaxed)
    }

    /// Returns the peak concurrency limit since the last call, resetting it to the current limit
    /// so the next interval's peak starts from the right baseline.
    pub fn take_peak_limit(&self) -> usize {
        let current = self.0.gauge.load(Ordering::Acquire);
        self.0.peak_limit.swap(current, Ordering::Relaxed)
    }
}

/// RAII guard returned by [`Limiter::acquire`]. Decrements the inflight counter on drop.
///
/// Uses `Option<Arc<LimiterInner>>` so that [`record_sample`](Token::record_sample) can
/// consume the token (taking the `Arc`) while [`Drop`] handles the panic/cancellation
/// path without double-decrementing.
pub struct Token {
    inner: Option<Arc<LimiterInner>>,
    /// Inflight count captured at acquire time (includes this token).
    inflight: usize,
    /// Timestamp captured at acquire time for automatic RTT measurement.
    start: Instant,
}

impl Token {
    /// Record a sample using the inflight count captured at acquire time and the
    /// elapsed time since acquisition. Consumes the token.
    /// Returns the current concurrency limit.
    pub fn record_sample(mut self, outcome: Outcome) -> usize {
        let inner = self.inner.take().expect("record_sample called twice");
        if matches!(outcome, Outcome::Ignore) {
            inner.inflight.fetch_sub(1, Ordering::Relaxed);
            return inner.gauge.load(Ordering::Acquire);
        }
        let prev = inner.gauge.load(Ordering::Acquire);
        let now = match &inner.clock {
            Some(f) => f(),
            None => Instant::now(),
        };
        let rtt = now.saturating_duration_since(self.start);
        inner.inflight.fetch_sub(1, Ordering::Relaxed);
        let result = inner.algorithm.update(self.inflight, outcome, rtt);
        inner.gauge.store(result, Ordering::Release);
        inner.peak_limit.fetch_max(result, Ordering::Relaxed);
        if result != prev
            && let Some(ref cb) = inner.on_limit_change
        {
            cb(result);
        }
        result
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            inner.inflight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// ConcurrencyLimit enum — serializable config that builds a Limiter
// ---------------------------------------------------------------------------

/// Serializable concurrency limit configuration.
///
/// Selects the algorithm used to manage concurrent writers or ingest workers.
/// `Fixed` uses a static limit (the default); `Aimd` and `Gradient` adjust the
/// limit dynamically based on commit outcomes or latency.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyLimit {
    Fixed { limit: usize },
    Aimd(AimdConfig),
    Gradient(GradientConfig),
}

impl<'de> Deserialize<'de> for ConcurrencyLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Tagged {
            Fixed { limit: usize },
            Aimd(AimdConfig),
            Gradient(GradientConfig),
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Integer(usize),
            Tagged(Tagged),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Integer(limit) => Ok(ConcurrencyLimit::Fixed { limit }),
            Helper::Tagged(Tagged::Fixed { limit }) => Ok(ConcurrencyLimit::Fixed { limit }),
            Helper::Tagged(Tagged::Aimd(c)) => Ok(ConcurrencyLimit::Aimd(c)),
            Helper::Tagged(Tagged::Gradient(c)) => Ok(ConcurrencyLimit::Gradient(c)),
        }
    }
}

impl ConcurrencyLimit {
    /// Build a [`Limiter`] from this configuration.
    pub fn build(&self) -> Limiter {
        match self {
            Self::Fixed { limit } => Limiter::fixed(*limit),
            Self::Aimd(config) => Limiter::aimd(config.clone()),
            Self::Gradient(config) => Limiter::gradient(config.clone()),
        }
    }

    /// Build a [`Limiter`] from this configuration, registering `on_limit_change` as a callback
    /// invoked whenever the adaptive algorithm adjusts the limit. For fixed limiters, the callback
    /// is never invoked (the limit is constant), but the caller can read the initial limit via
    /// [`Limiter::current`].
    pub fn build_with_on_limit_change(
        &self,
        on_limit_change: impl Fn(usize) + Send + Sync + 'static,
    ) -> Limiter {
        match self {
            Self::Fixed { limit } => Limiter::fixed(*limit),
            Self::Aimd(config) => Limiter::builder_aimd(config.clone())
                .on_limit_change(on_limit_change)
                .build(),
            Self::Gradient(config) => Limiter::builder_gradient(config.clone())
                .on_limit_change(on_limit_change)
                .build(),
        }
    }
}

// ---------------------------------------------------------------------------
// AIMD algorithm
// ---------------------------------------------------------------------------

/// Configuration for the AIMD (Additive Increase / Multiplicative Decrease) algorithm.
///
/// Uses Netflix-style gentle backoff (default `backoff_ratio = 0.9`, i.e. 10% cut) rather
/// than TCP-style halving. On each successful sample where the system is under pressure
/// (inflight >= limit/2), the limit increases by 1.
///
/// If `timeout` is set, any successful sample whose RTT exceeds the timeout is treated as a
/// drop (multiplicative decrease), matching Netflix's `AIMDLimit` behavior.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct AimdConfig {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Multiplicative decrease factor on drop, in `[0.5, 1.0)`.
    pub backoff_ratio: f64,
    /// RTT threshold beyond which a successful sample is treated as a drop.
    /// `None` disables timeout-based backoff (the caller must classify drops explicitly).
    #[serde(
        default = "default_aimd_timeout",
        with = "option_duration_secs",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout: Option<Duration>,
}

impl Default for AimdConfig {
    fn default() -> Self {
        Self {
            initial_limit: 20,
            min_limit: 20,
            max_limit: 200,
            backoff_ratio: 0.9,
            timeout: default_aimd_timeout(),
        }
    }
}

/// AIMD concurrency limit algorithm.
///
/// On each sample:
/// - **Dropped** (or RTT > timeout): `limit = max(min_limit, floor(limit * backoff_ratio))`
/// - **Success** (inflight >= limit/2): `limit = min(max_limit, limit + 1)`
/// - **Success** (inflight < limit/2): no change (app-limited)
struct Aimd {
    state: Mutex<usize>,
    backoff_ratio: f64,
    timeout: Option<Duration>,
    min_limit: usize,
    max_limit: usize,
}

impl Aimd {
    fn new(config: &AimdConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self {
            state: Mutex::new(initial),
            backoff_ratio: config.backoff_ratio,
            timeout: config.timeout,
            min_limit: config.min_limit,
            max_limit: config.max_limit,
        }
    }

    fn update(&self, inflight: usize, outcome: Outcome, rtt: Duration) -> usize {
        let mut current = self.state.lock().unwrap();
        let is_drop = matches!(outcome, Outcome::Dropped)
            || (matches!(outcome, Outcome::Success) && self.timeout.is_some_and(|t| rtt > t));

        if is_drop {
            *current = ((*current as f64) * self.backoff_ratio).floor() as usize;
        } else if matches!(outcome, Outcome::Success) && inflight >= (*current / 2) + (*current % 2)
        {
            *current = current.saturating_add(1);
        }
        *current = (*current).clamp(self.min_limit, self.max_limit);
        *current
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        *self.state.lock().unwrap()
    }
}

// ---------------------------------------------------------------------------
// Gradient algorithm — Netflix's latency-predictive concurrency limiter
// ---------------------------------------------------------------------------

/// Configuration for the Gradient concurrency limit algorithm (based on Netflix's Gradient2).
///
/// Adjusts the limit based on the ratio of long-term to short-term RTT, making it sensitive to
/// latency changes rather than errors alone. When latency increases (gradient < 1), the limit
/// decreases; when latency is stable or improving (gradient ~1), the limit grows additively by
/// `queue_size`.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct GradientConfig {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Exponential smoothing factor in `(0.0, 1.0]` for blending old and new limit estimates.
    pub smoothing: f64,
    /// Multiplier on the long/short RTT ratio; values > 1.0 tolerate some latency increase.
    pub tolerance: f64,
    /// Window size for the long-term RTT exponential moving average.
    pub long_window: usize,
    /// Fixed additive growth term when latency is stable (gradient ~1.0).
    pub queue_size: usize,
}

impl Default for GradientConfig {
    fn default() -> Self {
        Self {
            initial_limit: 20,
            min_limit: 20,
            max_limit: 200,
            smoothing: 0.2,
            tolerance: 1.5,
            long_window: 600,
            queue_size: 4,
        }
    }
}

/// Exponential moving average with a warmup phase.
///
/// During the warmup period (first `warmup` samples), a simple arithmetic mean is used.
/// After warmup, the value transitions to an EMA with `factor = 2.0 / (window + 1)`.
///
/// The warmup exists because the EMA is initialized to 0.0. With a small smoothing factor
/// (e.g. `2/601 ≈ 0.003` for `window=600`), a cold EMA would take hundreds of samples to
/// reach the true baseline. In the meantime, `long_rtt / short_rtt ≈ 0` would clamp the
/// gradient to 0.5, causing the algorithm to halve the limit on every sample during startup.
/// The arithmetic mean converges to the real baseline within a few samples, giving the EMA
/// a sensible starting point.
struct ExpAvgMeasurement {
    value: f64,
    sum: f64,
    count: usize,
    warmup: usize,
    factor: f64,
}

impl ExpAvgMeasurement {
    fn new(window: usize, warmup: usize) -> Self {
        Self {
            value: 0.0,
            sum: 0.0,
            count: 0,
            warmup,
            factor: 2.0 / (window as f64 + 1.0),
        }
    }

    fn update_average(&mut self, sample: f64) -> f64 {
        if self.count < self.warmup {
            self.count += 1;
            self.sum += sample;
            self.value = self.sum / self.count as f64;
        } else {
            self.value = self.value * (1.0 - self.factor) + sample * self.factor;
        }
        self.value
    }

    /// Apply an arbitrary transformation to the current value (e.g. drift decay).
    fn update(&mut self, f: impl FnOnce(f64) -> f64) {
        self.value = f(self.value);
    }

    #[cfg(test)]
    fn get(&self) -> f64 {
        self.value
    }
}

struct GradientState {
    estimated_limit: f64,
    long_rtt: ExpAvgMeasurement,
}

/// Gradient concurrency limit algorithm (based on Netflix's Gradient2).
///
/// Adjusts the limit by computing a gradient from long-term vs short-term RTT:
/// 1. Record `short_rtt = rtt`, update `long_rtt` EMA
/// 2. Drift recovery: if `long_rtt / short_rtt > 2.0`, decay long_rtt by 5%
/// 3. App-limiting guard: if `inflight < estimated_limit / 2`, return unchanged
/// 4. Gradient: `max(0.5, min(1.0, tolerance * long_rtt / short_rtt))`
/// 5. New limit: `estimated_limit * gradient + queue_size`
/// 6. Smooth: `estimated_limit * (1 - smoothing) + new_limit * smoothing`
/// 7. Clamp to `[min_limit, max_limit]`
struct Gradient {
    state: Mutex<GradientState>,
    queue_size_fn: Arc<QueueSizeFn>,
    smoothing: f64,
    tolerance: f64,
    min_limit: usize,
    max_limit: usize,
}

impl Gradient {
    fn new(config: &GradientConfig) -> Self {
        let queue_size = config.queue_size;
        Self::new_with_queue_size_fn(config, Arc::new(move |_| queue_size))
    }

    fn new_with_queue_size_fn(config: &GradientConfig, queue_size_fn: Arc<QueueSizeFn>) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self {
            state: Mutex::new(GradientState {
                estimated_limit: initial as f64,
                long_rtt: ExpAvgMeasurement::new(config.long_window, 10),
            }),
            queue_size_fn,
            smoothing: config.smoothing,
            tolerance: config.tolerance,
            min_limit: config.min_limit,
            max_limit: config.max_limit,
        }
    }

    fn update(&self, inflight: usize, _outcome: Outcome, rtt: Duration) -> usize {
        let mut state = self.state.lock().unwrap();

        let short_rtt = rtt.as_nanos() as f64;
        if short_rtt == 0.0 {
            return state.estimated_limit as usize;
        }

        let long_rtt = state.long_rtt.update_average(short_rtt);

        // Drift recovery: if the long-term RTT has drifted much higher than current
        // observations, decay it to prevent the limit from being permanently inflated.
        if long_rtt / short_rtt > 2.0 {
            state.long_rtt.update(|v| v * 0.95);
        }

        // App-limiting guard: don't adjust when the system isn't under pressure.
        if (inflight as f64) < state.estimated_limit / 2.0 {
            return state.estimated_limit as usize;
        }

        let gradient = (self.tolerance * long_rtt / short_rtt).clamp(0.5, 1.0);
        let queue_size = (self.queue_size_fn)(state.estimated_limit as usize) as f64;
        let new_limit = state.estimated_limit * gradient + queue_size;
        state.estimated_limit = (state.estimated_limit * (1.0 - self.smoothing)
            + new_limit * self.smoothing)
            .clamp(self.min_limit as f64, self.max_limit as f64);

        state.estimated_limit as usize
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.state.lock().unwrap().estimated_limit as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AimdConfig {
        AimdConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 20,
            backoff_ratio: 0.9,
            timeout: None,
        }
    }

    fn aimd(config: AimdConfig) -> Aimd {
        Aimd::new(&config)
    }

    fn default_gradient_config() -> GradientConfig {
        GradientConfig {
            initial_limit: 20,
            min_limit: 5,
            max_limit: 200,
            smoothing: 0.2,
            tolerance: 1.5,
            long_window: 600,
            queue_size: 4,
        }
    }

    fn gradient(config: GradientConfig) -> Gradient {
        Gradient::new(&config)
    }

    // ======================== AIMD algorithm tests ========================

    #[test]
    fn success_increases_limit_by_one() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 11);
    }

    #[test]
    fn drop_decreases_limit_by_backoff_ratio() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);
        alg.update(0, Outcome::Dropped, Duration::from_millis(10));
        // floor(10 * 0.9) = 9
        assert_eq!(alg.current(), 9);
    }

    #[test]
    fn ignore_has_no_effect() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);
        alg.update(0, Outcome::Ignore, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);
    }

    #[test]
    fn limit_stays_within_min_max() {
        let config = AimdConfig {
            initial_limit: 2,
            min_limit: 2,
            max_limit: 3,
            backoff_ratio: 0.5,
            timeout: None,
        };
        let alg = aimd(config);

        // Decrease should not go below min_limit
        alg.update(0, Outcome::Dropped, Duration::from_millis(10));
        assert_eq!(alg.current(), 2); // max(2, floor(2*0.5)=1) = 2

        // Increase should not go above max_limit
        alg.update(2, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 3);
        alg.update(3, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 3); // clamped at max
    }

    #[test]
    fn multiple_drops_reduce_progressively() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);

        alg.update(0, Outcome::Dropped, Duration::from_millis(10)); // floor(10 * 0.9) = 9
        assert_eq!(alg.current(), 9);

        alg.update(0, Outcome::Dropped, Duration::from_millis(10)); // floor(9 * 0.9) = 8
        assert_eq!(alg.current(), 8);

        alg.update(0, Outcome::Dropped, Duration::from_millis(10)); // floor(8 * 0.9) = 7
        assert_eq!(alg.current(), 7);
    }

    #[test]
    fn recovery_after_drop() {
        let alg = aimd(default_config());

        alg.update(10, Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(alg.current(), 9);

        alg.update(10, Outcome::Success, Duration::from_millis(10)); // 9 -> 10
        assert_eq!(alg.current(), 10);

        alg.update(10, Outcome::Success, Duration::from_millis(10)); // 10 -> 11
        assert_eq!(alg.current(), 11);
    }

    #[test]
    fn timeout_triggers_backoff() {
        let config = AimdConfig {
            timeout: Some(Duration::from_secs(1)),
            ..default_config()
        };
        let alg = aimd(config);
        assert_eq!(alg.current(), 10);

        // A success within the timeout does not trigger backoff.
        alg.update(10, Outcome::Success, Duration::from_millis(500));
        assert_eq!(alg.current(), 11);

        // A success exceeding the timeout is treated as a drop.
        alg.update(10, Outcome::Success, Duration::from_millis(1500));
        // floor(11 * 0.9) = 9
        assert_eq!(alg.current(), 9);
    }

    #[test]
    fn no_timeout_ignores_rtt() {
        let config = AimdConfig {
            timeout: None,
            ..default_config()
        };
        let alg = aimd(config);
        assert_eq!(alg.current(), 10);

        // Even a very slow success should increase when timeout is None.
        alg.update(10, Outcome::Success, Duration::from_secs(60));
        assert_eq!(alg.current(), 11);
    }

    #[test]
    #[should_panic(expected = "timeout must be positive")]
    fn aimd_rejects_zero_timeout() {
        let config = AimdConfig {
            timeout: Some(Duration::ZERO),
            ..default_config()
        };
        let _ = Limiter::aimd(config);
    }

    #[test]
    fn no_increase_when_underutilized() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);

        // With 0 inflight (0*2=0 < limit=10), success should not increase
        alg.update(0, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 4 inflight (4*2=8 < limit=10), should not increase
        alg.update(4, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 5 inflight (5*2=10 >= limit=10), should increase
        alg.update(5, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 11);
    }

    // ======================== ExpAvgMeasurement tests ========================

    #[test]
    fn exp_avg_warmup_phase_uses_simple_average() {
        let mut ema = ExpAvgMeasurement::new(100, 3);
        assert_eq!(ema.update_average(10.0), 10.0); // 10/1
        assert_eq!(ema.update_average(20.0), 15.0); // 30/2
        assert_eq!(ema.update_average(30.0), 20.0); // 60/3
    }

    #[test]
    fn exp_avg_transitions_to_ema_after_warmup() {
        let mut ema = ExpAvgMeasurement::new(100, 2);
        ema.update_average(10.0); // warmup 1
        ema.update_average(20.0); // warmup 2, value = 15.0

        // After warmup, EMA with factor = 2/101 ~= 0.0198
        let factor = 2.0 / 101.0;
        let expected = 15.0 * (1.0 - factor) + 30.0 * factor;
        let result = ema.update_average(30.0);
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn exp_avg_update_modifies_value() {
        let mut ema = ExpAvgMeasurement::new(100, 1);
        ema.update_average(100.0);
        ema.update(|v| v * 0.95);
        assert!((ema.get() - 95.0).abs() < 1e-10);
    }

    // ======================== Gradient algorithm tests ========================

    #[test]
    fn gradient_steady_state_grows() {
        let alg = gradient(default_gradient_config());

        // Feed many samples at the same RTT; gradient should be ~1.0, limit should grow.
        let rtt = Duration::from_millis(50);
        for _ in 0..100 {
            alg.update(20, Outcome::Success, rtt);
        }
        assert!(alg.current() > 20, "Limit should grow under steady RTT");
    }

    #[test]
    fn gradient_increasing_latency_reduces_limit() {
        let config = GradientConfig {
            initial_limit: 100,
            min_limit: 5,
            max_limit: 200,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // Establish a baseline long RTT
        let baseline_rtt = Duration::from_millis(50);
        for _ in 0..20 {
            alg.update(100, Outcome::Success, baseline_rtt);
        }
        let before = alg.current();

        // Now spike the RTT — gradient < 1.0 should reduce the limit
        let high_rtt = Duration::from_millis(500);
        for _ in 0..50 {
            alg.update(100, Outcome::Success, high_rtt);
        }
        assert!(
            alg.current() < before,
            "Limit should decrease when latency spikes (before={before}, after={})",
            alg.current()
        );
    }

    #[test]
    fn gradient_app_limiting_guard() {
        let alg = gradient(default_gradient_config());
        // Pass 0 inflight — guard should prevent changes
        let initial = alg.current();
        for _ in 0..50 {
            alg.update(0, Outcome::Success, Duration::from_millis(50));
        }
        assert_eq!(alg.current(), initial);
    }

    #[test]
    fn gradient_min_max_bounds() {
        let config = GradientConfig {
            initial_limit: 10,
            min_limit: 10,
            max_limit: 15,
            smoothing: 1.0, // aggressive smoothing to hit bounds fast
            long_window: 10,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        let rtt = Duration::from_millis(50);
        for _ in 0..100 {
            alg.update(15, Outcome::Success, rtt);
        }
        assert!(alg.current() <= 15, "Should not exceed max_limit");
    }

    #[test]
    fn gradient_ignore_has_no_effect() {
        let limiter = Limiter::gradient(default_gradient_config());
        let initial = limiter.current();

        let token = limiter.acquire();
        token.record_sample(Outcome::Ignore);
        assert_eq!(limiter.current(), initial);
    }

    #[test]
    fn gradient_drops_flow_through_gradient() {
        let alg = gradient(default_gradient_config());
        let initial = alg.current();

        // Drops go through the same gradient calculation as successes.
        // A very fast drop RTT with tolerance=1.5 gives gradient clamped to 1.0
        // (long_rtt/short_rtt is large), so the limit should not decrease.
        alg.update(20, Outcome::Dropped, Duration::from_millis(1));
        assert!(
            alg.current() >= initial,
            "Fast drop should not decrease limit via gradient"
        );
    }

    #[test]
    fn gradient_drift_recovery() {
        let config = GradientConfig {
            long_window: 10,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // Establish a high long RTT
        for _ in 0..20 {
            alg.update(20, Outcome::Success, Duration::from_millis(200));
        }
        let long_rtt_before = {
            let state = alg.state.lock().unwrap();
            state.long_rtt.get()
        };

        // Now send a much lower RTT — should trigger drift decay
        alg.update(20, Outcome::Success, Duration::from_millis(50));
        let long_rtt_after = {
            let state = alg.state.lock().unwrap();
            state.long_rtt.get()
        };

        // The long_rtt should have been decayed because long/short > 2.0
        // Note: the EMA update itself moves it, but the decay should make it
        // noticeably lower than if only the EMA updated it
        assert!(long_rtt_after < long_rtt_before);
    }

    #[test]
    fn gradient_sustained_latency_spike_decreases_then_stabilizes() {
        let config = GradientConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 200,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // Establish baseline at 30ms. The limit grows during this phase
        // (gradient=1.0, queue_size pushes it up).
        for _ in 0..50 {
            alg.update(200, Outcome::Success, Duration::from_millis(30));
        }
        let limit_before_spike = alg.current();

        // Spike to 90ms (3x baseline). The gradient drops to ~0.5, pulling
        // the limit down. 100 samples is enough to see a clear decrease but
        // not enough for the EMA (half-life ~208 samples) to absorb the new
        // latency.
        for _ in 0..100 {
            alg.update(200, Outcome::Success, Duration::from_millis(90));
        }
        let limit_after_spike = alg.current();
        assert!(
            limit_after_spike < limit_before_spike,
            "Limit should decrease under sustained latency spike \
             (before={limit_before_spike}, after={limit_after_spike})"
        );

        // Continue at 90ms for much longer — the EMA absorbs the new baseline,
        // gradient recovers toward 1.0, and the limit stabilizes above min_limit.
        for _ in 0..2000 {
            alg.update(200, Outcome::Success, Duration::from_millis(90));
        }
        let limit_recovered = alg.current();
        assert!(
            limit_recovered > limit_after_spike,
            "Limit should recover as EMA absorbs new baseline \
             (spike={limit_after_spike}, recovered={limit_recovered})"
        );
    }

    #[test]
    fn gradient_zero_rtt_does_not_propagate_nan() {
        let alg = gradient(default_gradient_config());
        let initial = alg.current();

        alg.update(20, Outcome::Success, Duration::ZERO);
        assert_eq!(
            alg.current(),
            initial,
            "Zero RTT should leave limit unchanged"
        );
    }

    // ======================== Limiter / Token tests ========================

    #[test]
    fn token_acquire_increments_inflight() {
        let limiter = Limiter::aimd(default_config());
        assert_eq!(limiter.inflight(), 0);

        let _t1 = limiter.acquire();
        assert_eq!(limiter.inflight(), 1);

        let _t2 = limiter.acquire();
        assert_eq!(limiter.inflight(), 2);
    }

    #[test]
    fn token_drop_decrements_inflight() {
        let limiter = Limiter::aimd(default_config());

        let t1 = limiter.acquire();
        let t2 = limiter.acquire();
        assert_eq!(limiter.inflight(), 2);

        drop(t1);
        assert_eq!(limiter.inflight(), 1);

        drop(t2);
        assert_eq!(limiter.inflight(), 0);
    }

    #[test]
    fn token_record_sample_updates_limit() {
        let limiter = Limiter::aimd(default_config());
        assert_eq!(limiter.current(), 10);

        // Acquire enough tokens to pass the inflight >= limit/2 guard.
        // Acquire 9 tokens we'll just drop, then one for sampling.
        let _hold: Vec<_> = (0..9).map(|_| limiter.acquire()).collect();
        let sample_token = limiter.acquire();
        assert_eq!(limiter.inflight(), 10);

        sample_token.record_sample(Outcome::Success);
        assert_eq!(limiter.current(), 11);
    }

    #[test]
    fn record_sample_updates_limit() {
        let limiter = Limiter::aimd(default_config());
        assert_eq!(limiter.current(), 10);

        // Acquire enough tokens to pass the inflight >= limit/2 guard.
        let _hold: Vec<_> = (0..9).map(|_| limiter.acquire()).collect();
        let sample_token = limiter.acquire();

        sample_token.record_sample(Outcome::Success);
        assert_eq!(limiter.current(), 11);

        // Drops don't check the inflight guard, so any token works.
        let drop_token = limiter.acquire();
        drop_token.record_sample(Outcome::Dropped);
        assert_eq!(limiter.current(), 9); // floor(11 * 0.9) = 9
    }

    #[test]
    fn initial_limit_is_clamped_to_bounds() {
        let config = AimdConfig {
            initial_limit: 0,
            min_limit: 5,
            max_limit: 10,
            backoff_ratio: 0.9,
            timeout: None,
        };
        let limiter = Limiter::aimd(config);
        assert_eq!(limiter.current(), 5);

        let config = AimdConfig {
            initial_limit: 100,
            min_limit: 5,
            max_limit: 10,
            backoff_ratio: 0.9,
            timeout: None,
        };
        let limiter = Limiter::aimd(config);
        assert_eq!(limiter.current(), 10);
    }

    #[test]
    fn gradient_updates_via_token() {
        let limiter = Limiter::gradient(default_gradient_config());
        assert_eq!(limiter.current(), 20);

        // Hold 19 tokens for the entire loop to keep inflight high enough
        // to pass the app-limiting guard (inflight >= limit/2).
        let _hold: Vec<_> = (0..19).map(|_| limiter.acquire()).collect();
        for _ in 0..20 {
            let token = limiter.acquire();
            std::thread::sleep(Duration::from_micros(1));
            token.record_sample(Outcome::Success);
        }
        assert!(limiter.current() > 20);
    }

    #[test]
    fn gradient_dynamic_queue_size_function_changes_growth() {
        let config = GradientConfig {
            initial_limit: 20,
            min_limit: 5,
            max_limit: 200,
            smoothing: 1.0,
            tolerance: 1.5,
            long_window: 10,
            queue_size: 0,
        };
        let static_alg = Gradient::new(&config);
        let dynamic_alg = Gradient::new_with_queue_size_fn(&config, Arc::new(|_limit| 4));

        for _ in 0..10 {
            static_alg.update(20, Outcome::Success, Duration::from_millis(50));
        }
        assert_eq!(static_alg.current(), 20);

        for _ in 0..5 {
            dynamic_alg.update(20, Outcome::Success, Duration::from_millis(50));
        }
        assert!(dynamic_alg.current() > 20);
    }

    #[test]
    #[should_panic(expected = "tolerance must be >= 1.0")]
    fn gradient_rejects_tolerance_below_one() {
        let config = GradientConfig {
            tolerance: 0.9,
            ..default_gradient_config()
        };
        let _ = Limiter::gradient(config);
    }

    // ======================== Fixed algorithm tests ========================

    #[test]
    fn fixed_limiter_never_changes() {
        let limiter = Limiter::fixed(42);
        assert_eq!(limiter.current(), 42);

        let t1 = limiter.acquire();
        t1.record_sample(Outcome::Success);
        assert_eq!(limiter.current(), 42);

        let t2 = limiter.acquire();
        t2.record_sample(Outcome::Dropped);
        assert_eq!(limiter.current(), 42);
    }

    #[test]
    fn fixed_limiter_current() {
        let limiter = Limiter::fixed(7);
        assert_eq!(limiter.current(), 7);

        let token = limiter.acquire();
        token.record_sample(Outcome::Dropped);
        assert_eq!(limiter.current(), 7);
    }

    // ======================== ConcurrencyLimit enum tests ========================

    #[test]
    fn concurrency_limit_fixed_build() {
        let config = ConcurrencyLimit::Fixed { limit: 5 };
        let limiter = config.build();
        assert_eq!(limiter.current(), 5);
    }

    #[test]
    fn concurrency_limit_aimd_build() {
        let config = ConcurrencyLimit::Aimd(AimdConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 100,
            ..AimdConfig::default()
        });
        let limiter = config.build();
        assert_eq!(limiter.current(), 10);
    }

    #[test]
    fn concurrency_limit_gradient_build() {
        let config = ConcurrencyLimit::Gradient(GradientConfig {
            initial_limit: 20,
            max_limit: 500,
            ..GradientConfig::default()
        });
        let limiter = config.build();
        assert_eq!(limiter.current(), 20);
    }

    #[test]
    fn peak_inflight_tracking() {
        let limiter = Limiter::fixed(10);
        assert_eq!(limiter.take_peak_inflight(), 0);

        let t1 = limiter.acquire();
        let t2 = limiter.acquire();
        let t3 = limiter.acquire();
        assert_eq!(limiter.take_peak_inflight(), 3);
        // After take, peak resets to current inflight (3), not 0.
        assert_eq!(limiter.take_peak_inflight(), 3);

        drop(t1);
        drop(t2);
        let _t4 = limiter.acquire();
        // Peak is 3: at the moment of the previous take, inflight was 3 (t1+t2+t3), so
        // peak was reset to 3. Even though inflight dropped to 1 then rose to 2, the
        // peak since last take never exceeded the baseline of 3.
        assert_eq!(limiter.take_peak_inflight(), 3);

        // Now only t3 and t4 are held (inflight=2), so peak resets to 2.
        drop(t3);
        // t4 still held, inflight=1. Peak since last take was 2 (the baseline).
        assert_eq!(limiter.take_peak_inflight(), 2);
    }

    #[test]
    fn peak_limit_tracking() {
        let limiter = Limiter::aimd(AimdConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 20,
            backoff_ratio: 0.5,
            timeout: None,
        });
        assert_eq!(limiter.take_peak_limit(), 10);

        // Increase: hold enough tokens to pass inflight >= limit/2 guard
        let _hold: Vec<_> = (0..10).map(|_| limiter.acquire()).collect();
        let t = limiter.acquire();
        t.record_sample(Outcome::Success); // 10 -> 11
        assert_eq!(limiter.current(), 11);

        // Peak should be 11 (the new high)
        assert_eq!(limiter.take_peak_limit(), 11);

        // Drop: limit goes 11 -> 5
        let t = limiter.acquire();
        t.record_sample(Outcome::Dropped); // floor(11 * 0.5) = 5
        assert_eq!(limiter.current(), 5);

        // Peak since last take was 11 (the baseline from the swap), not 5
        assert_eq!(limiter.take_peak_limit(), 11);

        // Now peak resets to current (5); no changes, so still 5
        assert_eq!(limiter.take_peak_limit(), 5);
    }

    #[test]
    fn record_sample_prevents_double_decrement() {
        let limiter = Limiter::fixed(10);
        let token = limiter.acquire();
        assert_eq!(limiter.inflight(), 1);

        token.record_sample(Outcome::Success);
        // After consuming record_sample, inflight should be 0
        assert_eq!(limiter.inflight(), 0);
        // Drop is a no-op since inner was taken
    }

    #[test]
    fn with_clock_controls_rtt_measurement() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let base = Instant::now();
        let clock_ticks = ticks.clone();
        let limiter = Limiter::builder_aimd(AimdConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 20,
            backoff_ratio: 0.9,
            timeout: Some(Duration::from_millis(5)),
        })
        .clock(move || {
            let step = clock_ticks.fetch_add(1, Ordering::SeqCst) as u64;
            base + Duration::from_millis(step * 10)
        })
        .build();
        assert_eq!(limiter.current(), 10);
        let token = limiter.acquire();
        token.record_sample(Outcome::Success);
        // RTT is 10ms from the injected clock, so success is treated as drop.
        assert_eq!(limiter.current(), 9);
    }

    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        concurrency: ConcurrencyLimit,
    }

    #[test]
    fn concurrency_limit_toml_fixed() {
        let toml_str = r#"
            [concurrency.fixed]
            limit = 5
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Fixed { limit: 5 }
        ));
    }

    #[test]
    fn concurrency_limit_toml_aimd() {
        let toml_str = r#"
            [concurrency.aimd]
            initial-limit = 10
            min-limit = 1
            max-limit = 200
            backoff-ratio = 0.9
            timeout = 5.0
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Aimd(AimdConfig { max_limit: 200, .. })
        ));
    }

    #[test]
    fn concurrency_limit_toml_aimd_missing_timeout_uses_default() {
        let toml_str = r#"
            [concurrency.aimd]
            initial-limit = 10
            min-limit = 1
            max-limit = 200
            backoff-ratio = 0.9
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        if let ConcurrencyLimit::Aimd(config) = parsed.concurrency {
            assert_eq!(config.timeout, Some(Duration::from_secs(5)));
        } else {
            panic!("Expected Aimd variant");
        }
    }

    #[test]
    fn concurrency_limit_toml_gradient() {
        let toml_str = r#"
            [concurrency.gradient]
            initial-limit = 200
            min-limit = 1
            max-limit = 1000
            smoothing = 0.2
            tolerance = 1.5
            long-window = 600
            queue-size = 4
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Gradient(GradientConfig {
                max_limit: 1000,
                ..
            })
        ));
    }

    #[test]
    fn concurrency_limit_toml_bare_integer() {
        let toml_str = r#"
            concurrency = 5
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Fixed { limit: 5 }
        ));
    }

    #[test]
    fn concurrency_limit_serialize_fixed_is_tagged() {
        let config = ConcurrencyLimit::Fixed { limit: 5 };
        let serialized = serde_json::to_value(&config).unwrap();
        assert_eq!(serialized, serde_json::json!({"fixed": {"limit": 5}}),);
    }
}

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
//! This implementation targets a general-purpose indexing framework that writes to arbitrary
//! production databases, ranging from small Postgres instances (~5 connections) to large-scale
//! stores like Bigtable (~1000+ concurrent writes). Netflix's defaults assume server admission
//! control (hundreds of concurrent HTTP requests to a single service). Our changes reflect the
//! different operating envelope:
//!
//! **`min_limit` default: 1** (Netflix: 20). Netflix assumes dropping below 20 concurrent requests
//! makes a server effectively offline. We target internal worker pools where the backing store may
//! only handle single-digit concurrency; the floor must be low enough to protect small databases.
//!
//! **`queue_size`: fixed 4** (matches Netflix Gradient2). This is the additive growth term when
//! latency is stable. A fixed value prevents compound growth that occurs with dynamic formulas
//! like `sqrt(limit)` — those cause the limit to grow faster the higher it gets, leading to
//! runaway limits (e.g. 26K) that never recover.
//!
//! **`backoff_ratio` on drops** (not in Netflix Gradient2). Netflix's Gradient2 removed the
//! explicit drop handling from v1 — drops fall through to the gradient calculation. We restored it
//! because a database returning fast errors (connection refused, quota exceeded) produces *low* RTT
//! that the gradient misreads as "healthy, increase limit." The multiplicative backoff (default
//! 0.9) catches this; set to 1.0 to disable and get pure Netflix Gradient2 behavior.
//!
//! **Architecture.** Netflix's library couples inflight tracking into each algorithm. We split it:
//! each [`LimitAlgorithm`] owns an `Arc<AtomicUsize>` gauge and writes the new limit directly on
//! each update, while [`Limiter`] handles inflight counting via a separate atomic. This avoids
//! duplicating acquire/release across algorithms and keeps inflight on the fastest possible path
//! (a single atomic) since it's called by 10k+ concurrent futures. The [`Token`] RAII guard
//! captures inflight at acquire time and passes it to `LimitAlgorithm::update` on sample, matching
//! Netflix's `AbstractLimiter.createListener()` which snapshots `inFlight.incrementAndGet()` at
//! request start.
//!
//! All other parameters (`smoothing`, `tolerance`, `long_window`, EMA warmup) match Netflix's
//! Gradient2 defaults.

pub mod stream;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

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

/// Concurrency limit algorithm owning its atomic gauge.
///
/// Implementations contain the mathematical logic for adjusting a concurrency limit and
/// write the new value directly to a shared [`AtomicUsize`] gauge. [`Limiter`] handles
/// inflight counting; the algorithm owns the limit state.
trait LimitAlgorithm: Send + Sync + 'static {
    /// Recompute the limit, write it to the gauge, and return it.
    fn update(&self, inflight: usize, outcome: Outcome, rtt: Duration) -> usize;

    /// Shared atomic gauge tracking the current concurrency limit.
    fn gauge(&self) -> Arc<AtomicUsize>;
}

/// Shared state between [`Limiter`] and [`Token`].
struct LimiterInner {
    algorithm: Box<dyn LimitAlgorithm>,
    inflight: AtomicUsize,
    peak_inflight: AtomicUsize,
}

/// Cloneable handle wrapping a dynamic concurrency limit algorithm.
///
/// This is the user-facing API for concurrency limiting. Call [`Limiter::acquire`] to obtain a
/// [`Token`] that automatically releases the inflight slot on drop.
#[derive(Clone)]
pub struct Limiter(Arc<LimiterInner>);

impl Limiter {
    pub fn fixed(limit: usize) -> Self {
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Fixed {
                gauge: Arc::new(AtomicUsize::new(limit)),
            }),
            inflight: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
        }))
    }

    pub fn aimd(config: AimdConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Aimd::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
        }))
    }

    pub fn gradient(config: GradientConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Gradient::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
        }))
    }

    /// Acquire an inflight slot, returning an RAII [`Token`] that releases it on drop.
    ///
    /// The current inflight count (after incrementing) is captured in the token so that
    /// [`Token::record_sample`] passes the load at request start, not at completion —
    /// matching Netflix's AbstractLimiter behavior.
    pub fn acquire(&self) -> Token {
        let inflight = self.0.inflight.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.peak_inflight.fetch_max(inflight, Ordering::Relaxed);
        Token {
            inner: Some(self.0.clone()),
            inflight,
            start: Instant::now(),
        }
    }

    /// Returns the current concurrency limit.
    pub fn current(&self) -> usize {
        self.0.algorithm.gauge().load(Ordering::Acquire)
    }

    /// Returns the current number of inflight operations.
    pub fn inflight(&self) -> usize {
        self.0.inflight.load(Ordering::Relaxed)
    }

    /// Returns the peak inflight count since the last call, resetting it to zero.
    pub fn take_peak_inflight(&self) -> usize {
        self.0.peak_inflight.swap(0, Ordering::Relaxed)
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
        let rtt = self.start.elapsed();
        let result = inner.algorithm.update(self.inflight, outcome, rtt);
        inner.inflight.fetch_sub(1, Ordering::Relaxed);
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
// Fixed (no-op) algorithm
// ---------------------------------------------------------------------------

/// Fixed concurrency limit — always returns the same value, no dynamic adjustment.
struct Fixed {
    gauge: Arc<AtomicUsize>,
}

impl LimitAlgorithm for Fixed {
    fn update(&self, _inflight: usize, _outcome: Outcome, _rtt: Duration) -> usize {
        self.gauge.load(Ordering::Relaxed)
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
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
}

// ---------------------------------------------------------------------------
// AIMD algorithm
// ---------------------------------------------------------------------------

/// Configuration for the AIMD (Additive Increase / Multiplicative Decrease) algorithm.
///
/// Uses Netflix-style gentle backoff (default `backoff_ratio = 0.9`, i.e. 10% cut) rather
/// than TCP-style halving. Combined with additive +1 increase per `successes_per_increase`
/// consecutive successes, this recovers quickly from transient errors.
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
    /// How many consecutive successes are required before increasing the limit by 1.
    pub successes_per_increase: usize,
}

impl Default for AimdConfig {
    fn default() -> Self {
        Self {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 200,
            backoff_ratio: 0.9,
            successes_per_increase: 10,
        }
    }
}

struct AimdState {
    consecutive_successes: usize,
}

/// AIMD concurrency limit algorithm.
///
/// On each sample:
/// - **Dropped**: `limit = max(min_limit, floor(limit * backoff_ratio))`
/// - **Success**: after `successes_per_increase` consecutive successes, `limit = min(max_limit, limit + 1)`
/// - **Ignore**: no change
pub struct Aimd {
    config: AimdConfig,
    gauge: Arc<AtomicUsize>,
    inner: Mutex<AimdState>,
}

impl Aimd {
    fn new(config: &AimdConfig, initial: usize) -> Self {
        Self {
            gauge: Arc::new(AtomicUsize::new(initial)),
            inner: Mutex::new(AimdState {
                consecutive_successes: 0,
            }),
            config: config.clone(),
        }
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.gauge.load(Ordering::Acquire)
    }
}

impl LimitAlgorithm for Aimd {
    fn update(&self, inflight: usize, outcome: Outcome, _rtt: Duration) -> usize {
        let mut state = self.inner.lock().unwrap();
        let mut current = self.gauge.load(Ordering::Relaxed);
        match outcome {
            Outcome::Dropped => {
                state.consecutive_successes = 0;
                current = ((current as f64) * self.config.backoff_ratio).floor() as usize;
                current = current.max(self.config.min_limit);
                self.gauge.store(current, Ordering::Release);
            }
            Outcome::Success => {
                state.consecutive_successes += 1;
                // Only increase when the system is actually under pressure. Without
                // this guard a lightly-loaded pipeline would ratchet the limit up to
                // max_limit without ever testing the boundary.
                if state.consecutive_successes >= self.config.successes_per_increase
                    && inflight >= current / 2
                {
                    state.consecutive_successes = 0;
                    current = (current + 1).min(self.config.max_limit);
                    self.gauge.store(current, Ordering::Release);
                }
            }
            Outcome::Ignore => {}
        }
        current
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
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
/// `sqrt(currentLimit)`.
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
    /// Multiplicative decrease factor on drops, in `[0.5, 1.0]`. Protects against fast-throttle
    /// scenarios (e.g. HTTP 429) where the server rejects quickly, producing low RTT that the
    /// gradient would misinterpret as healthy. Set to `1.0` to disable and use pure
    /// Netflix-style gradient-only behavior.
    pub backoff_ratio: f64,
}

impl Default for GradientConfig {
    fn default() -> Self {
        Self {
            initial_limit: 20,
            min_limit: 1,
            max_limit: 200,
            smoothing: 0.2,
            tolerance: 1.5,
            long_window: 600,
            backoff_ratio: 0.9,
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
/// 5. New limit: `estimated_limit * gradient + min(sqrt(estimated_limit), 0.2 * estimated_limit)`
/// 6. Smooth: `estimated_limit * (1 - smoothing) + new_limit * smoothing`
/// 7. Clamp to `[min_limit, max_limit]`
pub struct Gradient {
    config: GradientConfig,
    gauge: Arc<AtomicUsize>,
    inner: Mutex<GradientState>,
}

impl Gradient {
    fn new(config: &GradientConfig, initial: usize) -> Self {
        Self {
            gauge: Arc::new(AtomicUsize::new(initial)),
            inner: Mutex::new(GradientState {
                estimated_limit: initial as f64,
                long_rtt: ExpAvgMeasurement::new(config.long_window, 10),
            }),
            config: config.clone(),
        }
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.inner.lock().unwrap().estimated_limit as usize
    }
}

impl LimitAlgorithm for Gradient {
    fn update(&self, inflight: usize, outcome: Outcome, rtt: Duration) -> usize {
        let mut state = self.inner.lock().unwrap();

        if matches!(outcome, Outcome::Ignore) {
            return state.estimated_limit as usize;
        }

        // On drops, apply multiplicative backoff instead of the gradient. This avoids
        // contaminating the long RTT EMA with artificially low RTTs from fast throttle
        // responses (e.g. HTTP 429), which the gradient would misread as healthy.
        if matches!(outcome, Outcome::Dropped) && self.config.backoff_ratio < 1.0 {
            state.estimated_limit = (state.estimated_limit * self.config.backoff_ratio)
                .clamp(self.config.min_limit as f64, self.config.max_limit as f64);
            let current = state.estimated_limit as usize;
            self.gauge.store(current, Ordering::Release);
            return current;
        }

        let short_rtt = rtt.as_secs_f64();
        if short_rtt <= 0.0 {
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

        let gradient = (self.config.tolerance * long_rtt / short_rtt).clamp(0.5, 1.0);
        let queue_size = 4.0;
        let new_limit = state.estimated_limit * gradient + queue_size;
        state.estimated_limit = (state.estimated_limit * (1.0 - self.config.smoothing)
            + new_limit * self.config.smoothing)
            .clamp(self.config.min_limit as f64, self.config.max_limit as f64);

        let current = state.estimated_limit as usize;
        self.gauge.store(current, Ordering::Release);
        current
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
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
            successes_per_increase: 1,
        }
    }

    fn aimd(config: AimdConfig) -> Aimd {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Aimd::new(&config, initial)
    }

    fn default_gradient_config() -> GradientConfig {
        GradientConfig {
            initial_limit: 20,
            min_limit: 5,
            max_limit: 200,
            smoothing: 0.2,
            tolerance: 1.5,
            long_window: 600,
            backoff_ratio: 0.9,
        }
    }

    fn gradient(config: GradientConfig) -> Gradient {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Gradient::new(&config, initial)
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
            successes_per_increase: 1,
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
    fn consecutive_success_counter_resets_on_drop() {
        let config = AimdConfig {
            successes_per_increase: 3,
            ..default_config()
        };
        let alg = aimd(config);
        assert_eq!(alg.current(), 10);

        // Two successes, not enough to increase
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // Drop resets the counter
        alg.update(10, Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(alg.current(), 9);

        // Need 3 consecutive successes again
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 9); // still 9, only 2 successes

        alg.update(10, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10); // now 3 consecutive -> increase
    }

    #[test]
    fn ignore_does_not_affect_consecutive_successes() {
        let config = AimdConfig {
            successes_per_increase: 2,
            ..default_config()
        };
        let alg = aimd(config);

        alg.update(10, Outcome::Success, Duration::from_millis(10));
        alg.update(10, Outcome::Ignore, Duration::from_millis(10)); // should not reset counter
        alg.update(10, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 11); // 2 successes reached
    }

    #[test]
    fn no_increase_when_underutilized() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);

        // With 0 inflight (well under limit/2 = 5), success should not increase
        alg.update(0, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 4 inflight (still under limit/2 = 5), should not increase
        alg.update(4, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 5 inflight (= limit/2), should increase
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
        let alg = gradient(default_gradient_config());
        let initial = alg.current();

        alg.update(20, Outcome::Ignore, Duration::from_millis(50));
        assert_eq!(alg.current(), initial);
    }

    #[test]
    fn gradient_drop_backoff_reduces_limit() {
        let alg = gradient(default_gradient_config());
        assert_eq!(alg.current(), 20);

        // A drop should reduce limit by backoff_ratio (0.9)
        alg.update(20, Outcome::Dropped, Duration::from_millis(1));
        // floor(20 * 0.9) = 18
        assert_eq!(alg.current(), 18);

        alg.update(20, Outcome::Dropped, Duration::from_millis(1));
        // floor(18 * 0.9) = 16
        assert_eq!(alg.current(), 16);
    }

    #[test]
    fn gradient_drop_backoff_respects_min() {
        let config = GradientConfig {
            initial_limit: 6,
            min_limit: 5,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // floor(6 * 0.9) = 5, at min
        alg.update(0, Outcome::Dropped, Duration::from_millis(1));
        assert_eq!(alg.current(), 5);

        // Should not go below min
        alg.update(0, Outcome::Dropped, Duration::from_millis(1));
        assert_eq!(alg.current(), 5);
    }

    #[test]
    fn gradient_drop_backoff_disabled_at_1() {
        let config = GradientConfig {
            backoff_ratio: 1.0,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // With backoff_ratio=1.0, drops fall through to the gradient calculation.
        // A very fast drop RTT with tolerance=1.5 gives gradient clamped to 1.0,
        // so the limit should not decrease.
        let initial = alg.current();
        alg.update(20, Outcome::Dropped, Duration::from_millis(1));
        assert!(
            alg.current() >= initial,
            "With backoff disabled, fast drop should not decrease limit"
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
            let state = alg.inner.lock().unwrap();
            state.long_rtt.get()
        };

        // Now send a much lower RTT — should trigger drift decay
        alg.update(20, Outcome::Success, Duration::from_millis(50));
        let long_rtt_after = {
            let state = alg.inner.lock().unwrap();
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
    fn gradient_sustained_drops_reach_min_limit() {
        let config = GradientConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 200,
            ..default_gradient_config()
        };
        let alg = gradient(config);

        // Sustained drops bypass the gradient and apply multiplicative backoff
        // (limit *= 0.9) each time, grinding down to min_limit.
        for _ in 0..200 {
            alg.update(0, Outcome::Dropped, Duration::from_millis(1));
        }

        assert_eq!(alg.current(), 1);
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
    fn initial_limit_clamped_to_bounds() {
        let config = AimdConfig {
            initial_limit: 0,
            min_limit: 5,
            max_limit: 10,
            backoff_ratio: 0.9,
            successes_per_increase: 1,
        };
        let limiter = Limiter::aimd(config);
        assert_eq!(limiter.current(), 5);

        let config = AimdConfig {
            initial_limit: 100,
            min_limit: 5,
            max_limit: 10,
            backoff_ratio: 0.9,
            successes_per_increase: 1,
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
            token.record_sample(Outcome::Success);
        }
        assert!(limiter.current() > 20);
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
        assert_eq!(limiter.take_peak_inflight(), 0); // reset after take

        drop(t1);
        drop(t2);
        let _t4 = limiter.acquire();
        // Peak should be 2 now (t3 + t4), not 3
        assert_eq!(limiter.take_peak_inflight(), 2);
        drop(t3);
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
            successes-per-increase = 10
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Aimd(AimdConfig { max_limit: 200, .. })
        ));
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
            backoff-ratio = 0.9
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

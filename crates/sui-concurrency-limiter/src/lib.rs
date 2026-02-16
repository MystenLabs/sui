// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Dynamic concurrency limiters based on Netflix's
//! [concurrency-limits](https://github.com/Netflix/concurrency-limits) library.
//!
//! Five algorithms are provided:
//!
//! - **AIMD** (`Aimd`): loss-based. Additive increase on success, multiplicative decrease on
//!   drop. Simple and effective when the backing store signals overload via errors/throttling
//!   rather than latency degradation (e.g. GCS returning HTTP 429).
//!
//! - **Gradient** (`Gradient`, based on Netflix's Gradient2): latency-based. Computes a gradient
//!   from the ratio of long-term to short-term RTT and scales the limit proportionally. Effective
//!   when the backing store degrades gradually under load (e.g. Bigtable write latency increasing).
//!
//! - **Vegas** (`Vegas`, based on Netflix's VegasLimit): absolute-RTT-based. Uses the minimum
//!   RTT observed as a floor to estimate queue depth. Unlike Gradient, the baseline never
//!   drifts, so the limiter stays low when the backing store is saturated from the start.
//!   Supports an optional sliding window for `rtt_noload` (`rtt_noload_window`) to prevent
//!   transient low-latency startup samples from permanently anchoring the baseline.
//!
//! - **BBR** (`Bbr`): throughput-based. Measures bandwidth-delay product (BDP) as
//!   `max_delivery_rate × min_rtt` and sets the limit to `ceil(BDP × gain)`. Unlike
//!   latency-based algorithms that can't distinguish a saturated 1-node cluster from an idle
//!   5-node cluster returning identical latency, BBR detects saturation by observing whether
//!   more concurrency produces more throughput. Self-bootstraps from limit=1 via exponential
//!   growth until the backend saturates.
//!
//! - **Adaptive** (`Adaptive`): interval-based throughput probing with tail-latency braking.
//!   Collects statistics over ~1s probe intervals and uses a three-phase state machine
//!   (Startup → ProbeBW → emergency brake) to discover optimal concurrency. Measures
//!   completions-per-second as an independent throughput signal and asks whether it grows
//!   when concurrency increases. If yes, grow. If no, you've found the knee.
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
//! **Vegas** matches Netflix's `VegasLimit.java` (probe jitter, additive decrease on drops,
//! integer threshold comparisons, clamp-then-smooth ordering) with two differences:
//! `min_limit` defaults to 1 instead of 20 (matching our other algorithms), and drops skip
//! probe counting and `rtt_noload` updates to prevent fast-error RTTs from poisoning the
//! baseline (Netflix doesn't need this because their drops are timeouts, not fast errors).
//!
//! All other Gradient parameters (`smoothing`, `tolerance`, `long_window`, EMA warmup) match
//! Netflix's Gradient2 defaults.

mod adaptive;
pub mod stream;

pub use adaptive::AdaptiveConfig;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rand::Rng;
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
    ///
    /// `delivered` is the number of requests that completed between when this token was
    /// acquired and when it completed (including itself). This is the TCP BBR-style
    /// delivery count, measuring actual backend throughput rather than sender concurrency.
    ///
    /// `weight` is the number of units of work this sample represents (e.g. mutations in
    /// a batch). Algorithms that track throughput use this to count completions in work
    /// units rather than requests.
    fn update(&self, inflight: usize, delivered: usize, weight: usize, outcome: Outcome, rtt: Duration) -> usize;

    /// Shared atomic gauge tracking the current concurrency limit.
    fn gauge(&self) -> Arc<AtomicUsize>;
}

/// Shared state between [`Limiter`] and [`Token`].
struct LimiterInner {
    algorithm: Box<dyn LimitAlgorithm>,
    inflight: AtomicUsize,
    /// Cumulative count of completed requests (incremented in record_sample).
    /// Used to compute per-token delivery counts for BBR-style throughput measurement.
    total_completed: AtomicUsize,
    peak_inflight: AtomicUsize,
    peak_limit: AtomicUsize,
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
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(limit),
        }))
    }

    pub fn aimd(config: AimdConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Aimd::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(initial),
        }))
    }

    pub fn gradient(config: GradientConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Gradient::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(initial),
        }))
    }

    pub fn vegas(config: VegasConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Vegas::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(initial),
        }))
    }

    pub fn bbr(config: BbrConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(Bbr::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(initial),
        }))
    }

    pub fn adaptive(config: AdaptiveConfig) -> Self {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Self(Arc::new(LimiterInner {
            algorithm: Box::new(adaptive::Adaptive::new(&config, initial)),
            inflight: AtomicUsize::new(0),
            total_completed: AtomicUsize::new(0),
            peak_inflight: AtomicUsize::new(0),
            peak_limit: AtomicUsize::new(initial),
        }))
    }

    /// Acquire an inflight slot, returning an RAII [`Token`] that releases it on drop.
    ///
    /// The current inflight count (after incrementing) is captured in the token so that
    /// [`Token::record_sample`] passes the load at request start, not at completion —
    /// matching Netflix's AbstractLimiter behavior.
    pub fn acquire(&self) -> Token {
        self.acquire_weighted(1)
    }

    /// Acquire `weight` inflight slots, returning an RAII [`Token`] that releases them on drop.
    ///
    /// Used for mutation-space concurrency limiting where each batch consumes permits
    /// proportional to its size (e.g. number of mutations).
    pub fn acquire_weighted(&self, weight: usize) -> Token {
        let inflight = self.0.inflight.fetch_add(weight, Ordering::Relaxed) + weight;
        let completed_at_acquire = self.0.total_completed.load(Ordering::Relaxed);
        self.0.peak_inflight.fetch_max(inflight, Ordering::Relaxed);
        Token {
            inner: Some(self.0.clone()),
            inflight,
            completed_at_acquire,
            start: Instant::now(),
            weight,
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

    /// Returns the peak inflight count since the last call, resetting it to the current inflight
    /// so the next interval's peak starts from the right baseline.
    pub fn take_peak_inflight(&self) -> usize {
        let current = self.0.inflight.load(Ordering::Relaxed);
        self.0.peak_inflight.swap(current, Ordering::Relaxed)
    }

    /// Returns the peak concurrency limit since the last call, resetting it to the current limit
    /// so the next interval's peak starts from the right baseline.
    pub fn take_peak_limit(&self) -> usize {
        let current = self.0.algorithm.gauge().load(Ordering::Relaxed);
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
    /// Snapshot of `total_completed` at acquire time for delivery count calculation.
    completed_at_acquire: usize,
    /// Timestamp captured at acquire time for automatic RTT measurement.
    start: Instant,
    /// Number of work units this token represents (e.g. mutations in a batch).
    weight: usize,
}

impl Token {
    /// Record a sample using the inflight count captured at acquire time and the
    /// elapsed time since acquisition. Consumes the token.
    /// Returns the current concurrency limit.
    pub fn record_sample(mut self, outcome: Outcome) -> usize {
        let inner = self.inner.take().expect("record_sample called twice");
        let rtt = self.start.elapsed();
        let completed_now = inner.total_completed.fetch_add(self.weight, Ordering::Relaxed) + self.weight;
        let delivered = completed_now - self.completed_at_acquire;
        let result = inner
            .algorithm
            .update(self.inflight, delivered, self.weight, outcome, rtt);
        inner.peak_limit.fetch_max(result, Ordering::Relaxed);
        inner.inflight.fetch_sub(self.weight, Ordering::Relaxed);
        result
    }

    /// Deprecated alias for [`record_sample`](Token::record_sample). The weight is
    /// already stored in the token from [`Limiter::acquire_weighted`], so the explicit
    /// `_weight` parameter is ignored.
    pub fn record_sample_weighted(self, outcome: Outcome, _weight: usize) -> usize {
        self.record_sample(outcome)
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            inner.inflight.fetch_sub(self.weight, Ordering::Relaxed);
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
    fn update(
        &self,
        _inflight: usize,
        _delivered: usize,
        _weight: usize,
        _outcome: Outcome,
        _rtt: Duration,
    ) -> usize {
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
    Vegas(VegasConfig),
    Bbr(BbrConfig),
    Adaptive(AdaptiveConfig),
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
            Vegas(VegasConfig),
            Bbr(BbrConfig),
            Adaptive(AdaptiveConfig),
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
            Helper::Tagged(Tagged::Vegas(c)) => Ok(ConcurrencyLimit::Vegas(c)),
            Helper::Tagged(Tagged::Bbr(c)) => Ok(ConcurrencyLimit::Bbr(c)),
            Helper::Tagged(Tagged::Adaptive(c)) => Ok(ConcurrencyLimit::Adaptive(c)),
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
            Self::Vegas(config) => Limiter::vegas(config.clone()),
            Self::Bbr(config) => Limiter::bbr(config.clone()),
            Self::Adaptive(config) => Limiter::adaptive(config.clone()),
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
    fn update(
        &self,
        inflight: usize,
        _delivered: usize,
        _weight: usize,
        outcome: Outcome,
        _rtt: Duration,
    ) -> usize {
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
                    && inflight * 2 >= current
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

// ---------------------------------------------------------------------------
// Vegas algorithm — Netflix's absolute-RTT concurrency limiter
// ---------------------------------------------------------------------------

/// Configuration for the Vegas concurrency limit algorithm.
///
/// Based on Netflix's `VegasLimit.java` from the concurrency-limits library.
///
/// Unlike Gradient (which compares long-term vs short-term RTT), Vegas uses an absolute
/// reference: the minimum RTT observed (`rtt_noload`). It estimates queue depth as
/// `limit * (1 - rtt_noload / actual_rtt)` and backs off when the queue exceeds a threshold.
/// When `rtt_noload_window` is 0, this is the all-time minimum (matching Netflix). When
/// non-zero, it's the minimum over a sliding window, which prevents startup transients from
/// permanently anchoring the baseline too low.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct VegasConfig {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Exponential smoothing factor in `(0.0, 1.0]` for blending old and new limit estimates.
    /// Netflix default is 1.0 (immediate updates).
    pub smoothing: f64,
    /// Alpha threshold factor: `alpha = alpha_factor * log10(limit)`.
    /// When queue_size < alpha, the limit grows aggressively.
    pub alpha_factor: f64,
    /// Beta threshold factor: `beta = beta_factor * log10(limit)`.
    /// When queue_size > beta, the limit decreases.
    pub beta_factor: f64,
    /// Probe interval multiplier: rtt_noload resets every `probe_jitter * probe_multiplier * limit` samples.
    pub probe_multiplier: usize,
    /// When non-zero, rtt_noload is the minimum RTT over a sliding window of this many samples
    /// instead of the all-time minimum. This prevents transient low-latency samples (e.g. during
    /// startup) from permanently anchoring the baseline too low, which causes the limiter to
    /// underestimate capacity on low-latency backends.
    pub rtt_noload_window: usize,
}

impl Default for VegasConfig {
    fn default() -> Self {
        Self {
            initial_limit: 20,
            min_limit: 1,
            max_limit: 1000,
            smoothing: 1.0,
            alpha_factor: 3.0,
            beta_factor: 6.0,
            probe_multiplier: 30,
            rtt_noload_window: 0,
        }
    }
}

struct VegasState {
    estimated_limit: f64,
    rtt_noload: f64,
    probe_count: usize,
    probe_jitter: f64,
    /// Sliding window of recent RTT samples for computing windowed rtt_noload.
    /// Empty when `rtt_noload_window == 0` (all-time minimum mode).
    rtt_window: VecDeque<f64>,
}

/// Vegas concurrency limit algorithm (based on Netflix's `VegasLimit.java`).
///
/// Two modes for tracking `rtt_noload`:
/// - **All-time minimum** (`rtt_noload_window = 0`): matches Netflix. Probes periodically reset
///   `rtt_noload` to the current RTT; otherwise tracks the global minimum.
/// - **Windowed minimum** (`rtt_noload_window > 0`): `rtt_noload` is the minimum RTT over the
///   last N successful samples. Probing is disabled. This prevents startup transients from
///   permanently anchoring the baseline too low on low-latency backends.
///
/// On each sample:
/// 1. **Ignore** → return unchanged
/// 2. If `rtt <= 0` → return unchanged
/// 3. **Dropped** → `limit -= log10(limit)` (additive decrease), return early.
///    Skips `rtt_noload` updates to prevent fast-error RTTs from poisoning the baseline.
/// 4. (All-time mode) Increment `probe_count`; if probe triggers → reset `rtt_noload`, return
///    (Windowed mode) Push RTT into window, recompute `rtt_noload` as window minimum
/// 5. (All-time mode) If `rtt < rtt_noload` → update `rtt_noload`, return (calibration)
/// 6. Call `update_estimated_limit`:
///    a. App-limiting guard: if `inflight * 2 < limit`, return unchanged
///    b. Queue estimate: `queue_size = ceil(limit * (1 - rtt_noload / rtt))`
///    c. Thresholds: `alpha = alpha_factor * log10(limit)`, `beta = beta_factor * log10(limit)`
///    d. Adjust:
///       - `queue_size <= log10(limit)` → `+beta` (aggressive growth)
///       - `queue_size < alpha` → `+log10(limit)` (moderate growth)
///       - `queue_size > beta` → `-log10(limit)` (decrease)
///       - otherwise → return `(int) estimated_limit` (hold steady, no smoothing)
/// 7. Clamp to `[min_limit, max_limit]`, then smooth
pub struct Vegas {
    config: VegasConfig,
    gauge: Arc<AtomicUsize>,
    inner: Mutex<VegasState>,
}

/// Match Netflix's `Log10RootFunction`: `max(1, (int) log10(limit))`.
fn log10_root(limit: usize) -> i64 {
    1i64.max((limit as f64).log10() as i64)
}

impl Vegas {
    fn new(config: &VegasConfig, initial: usize) -> Self {
        let probe_jitter = rand::thread_rng().gen_range(0.5..1.0);
        Self {
            gauge: Arc::new(AtomicUsize::new(initial)),
            inner: Mutex::new(VegasState {
                estimated_limit: initial as f64,
                rtt_noload: 0.0,
                probe_count: 0,
                probe_jitter,
                rtt_window: VecDeque::new(),
            }),
            config: config.clone(),
        }
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.inner.lock().unwrap().estimated_limit as usize
    }
}

impl LimitAlgorithm for Vegas {
    fn update(&self, inflight: usize, _delivered: usize, _weight: usize, outcome: Outcome, rtt: Duration) -> usize {
        let mut state = self.inner.lock().unwrap();

        if matches!(outcome, Outcome::Ignore) {
            return state.estimated_limit as usize;
        }

        let short_rtt = rtt.as_secs_f64();
        if short_rtt <= 0.0 {
            return state.estimated_limit as usize;
        }

        // Handle drops before probing or updating rtt_noload. Fast errors
        // (connection refused, quota exceeded) produce artificially low RTTs
        // that would poison rtt_noload if allowed through. Netflix's VegasLimit
        // doesn't hit this in practice because their "drops" are timeouts (slow),
        // but ours can be fast, so we guard explicitly.
        if matches!(outcome, Outcome::Dropped) {
            let limit = state.estimated_limit as usize;
            let new_limit = (state.estimated_limit - log10_root(limit) as f64)
                .max(self.config.min_limit as f64)
                .min(self.config.max_limit as f64);
            state.estimated_limit = (1.0 - self.config.smoothing) * state.estimated_limit
                + self.config.smoothing * new_limit;
            let current = state.estimated_limit as usize;
            self.gauge.store(current, Ordering::Release);
            return current;
        }

        let windowed = self.config.rtt_noload_window > 0;

        if windowed {
            // Windowed mode: rtt_noload is the min of a sliding window of recent samples.
            state.rtt_window.push_back(short_rtt);
            while state.rtt_window.len() > self.config.rtt_noload_window {
                state.rtt_window.pop_front();
            }
            state.rtt_noload = state
                .rtt_window
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        } else {
            // All-time minimum mode (original Netflix behavior).

            // Probe: periodically reset rtt_noload to discover improved baselines.
            state.probe_count += 1;
            let probe_threshold =
                state.probe_jitter * self.config.probe_multiplier as f64 * state.estimated_limit;
            if state.probe_count as f64 >= probe_threshold {
                state.probe_jitter = rand::thread_rng().gen_range(0.5..1.0);
                state.probe_count = 0;
                state.rtt_noload = short_rtt;
                return state.estimated_limit as usize;
            }

            // When a new minimum is found, treat it as a calibration point and skip
            // adjustment (matching Netflix's VegasLimit behavior).
            if state.rtt_noload == 0.0 || short_rtt < state.rtt_noload {
                state.rtt_noload = short_rtt;
                return state.estimated_limit as usize;
            }
        }

        // From here: updateEstimatedLimit in Netflix's VegasLimit.java
        let limit = state.estimated_limit as usize;

        // App-limiting guard: don't adjust when the system isn't under pressure.
        if (inflight as f64) * 2.0 < state.estimated_limit {
            return state.estimated_limit as usize;
        }

        let queue_size =
            (state.estimated_limit * (1.0 - state.rtt_noload / short_rtt)).ceil() as i64;
        let log_limit = log10_root(limit);
        let alpha = (self.config.alpha_factor * log_limit as f64).ceil() as i64;
        let beta = (self.config.beta_factor * log_limit as f64).ceil() as i64;
        let threshold = log_limit;

        let new_limit = if queue_size <= threshold {
            state.estimated_limit + beta as f64
        } else if queue_size < alpha {
            state.estimated_limit + log_limit as f64
        } else if queue_size > beta {
            state.estimated_limit - log_limit as f64
        } else {
            // Hold steady — return directly without smoothing (matches Netflix)
            let current = state.estimated_limit as usize;
            self.gauge.store(current, Ordering::Release);
            return current;
        };

        // Clamp first, then smooth (matches Netflix's ordering)
        let clamped = new_limit
            .max(self.config.min_limit as f64)
            .min(self.config.max_limit as f64);
        state.estimated_limit =
            (1.0 - self.config.smoothing) * state.estimated_limit + self.config.smoothing * clamped;

        let current = state.estimated_limit as usize;
        self.gauge.store(current, Ordering::Release);
        current
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
    }
}

// ---------------------------------------------------------------------------
// BBR algorithm — throughput-based concurrency limiter
// ---------------------------------------------------------------------------

/// Configuration for the BBR concurrency limit algorithm.
///
/// Measures bandwidth-delay product (BDP) as `max_delivery_rate × min_rtt` and sets
/// `limit = ceil(BDP × gain)`. The gain factor (> 1.0) keeps the limit slightly above
/// BDP, enabling natural exponential growth from limit=1 until the backend saturates.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct BbrConfig {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Sliding window size for tracking minimum RTT.
    pub rtt_window: usize,
    /// Sliding window size for tracking maximum delivery rate.
    pub throughput_window: usize,
    /// Multiplier on BDP; values > 1.0 enable growth beyond current BDP.
    pub gain: f64,
    /// Multiplicative decrease factor on drops, in `[0.5, 1.0)`.
    pub backoff_ratio: f64,
}

impl Default for BbrConfig {
    fn default() -> Self {
        Self {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 1000,
            rtt_window: 5000,
            throughput_window: 5000,
            gain: 1.25,
            backoff_ratio: 0.9,
        }
    }
}

struct BbrState {
    estimated_limit: f64,
    rtt_window: VecDeque<f64>,
    delivery_rate_window: VecDeque<f64>,
}

/// BBR concurrency limit algorithm.
///
/// On each sample:
/// 1. **Ignore** → return unchanged
/// 2. If `rtt <= 0` → return unchanged
/// 3. **Dropped** → `estimated_limit *= backoff_ratio`, clamp, return (skip window updates)
/// 4. `delivery_rate = delivered / rtt`
/// 5. Push rtt and delivery_rate into windows, trim to size
/// 6. `min_rtt = min(rtt_window)`, `max_rate = max(delivery_rate_window)`
/// 7. `estimated_limit = ceil(max_rate * min_rtt * gain)`
/// 8. Clamp to `[min_limit, max_limit]`
pub struct Bbr {
    config: BbrConfig,
    gauge: Arc<AtomicUsize>,
    inner: Mutex<BbrState>,
}

impl Bbr {
    fn new(config: &BbrConfig, initial: usize) -> Self {
        Self {
            gauge: Arc::new(AtomicUsize::new(initial)),
            inner: Mutex::new(BbrState {
                estimated_limit: initial as f64,
                rtt_window: VecDeque::new(),
                delivery_rate_window: VecDeque::new(),
            }),
            config: config.clone(),
        }
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.gauge.load(Ordering::Acquire)
    }
}

impl LimitAlgorithm for Bbr {
    fn update(&self, _inflight: usize, delivered: usize, _weight: usize, outcome: Outcome, rtt: Duration) -> usize {
        let mut state = self.inner.lock().unwrap();

        if matches!(outcome, Outcome::Ignore) {
            return state.estimated_limit as usize;
        }

        let rtt_secs = rtt.as_secs_f64();
        if rtt_secs <= 0.0 {
            return state.estimated_limit as usize;
        }

        // Drops apply multiplicative backoff and skip window updates. Fast errors
        // produce artificially low RTT that would corrupt the min_rtt window.
        if matches!(outcome, Outcome::Dropped) {
            state.estimated_limit = (state.estimated_limit * self.config.backoff_ratio)
                .clamp(self.config.min_limit as f64, self.config.max_limit as f64);
            let current = state.estimated_limit as usize;
            self.gauge.store(current, Ordering::Release);
            return current;
        }

        let delivery_rate = delivered as f64 / rtt_secs;

        state.rtt_window.push_back(rtt_secs);
        while state.rtt_window.len() > self.config.rtt_window {
            state.rtt_window.pop_front();
        }

        state.delivery_rate_window.push_back(delivery_rate);
        while state.delivery_rate_window.len() > self.config.throughput_window {
            state.delivery_rate_window.pop_front();
        }

        let min_rtt = state
            .rtt_window
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max_rate = state
            .delivery_rate_window
            .iter()
            .copied()
            .fold(0.0_f64, f64::max);

        let bdp = max_rate * min_rtt;
        state.estimated_limit = (bdp * self.config.gain)
            .ceil()
            .clamp(self.config.min_limit as f64, self.config.max_limit as f64);

        let current = state.estimated_limit as usize;
        self.gauge.store(current, Ordering::Release);
        current
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
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
    fn update(&self, inflight: usize, _delivered: usize, _weight: usize, outcome: Outcome, rtt: Duration) -> usize {
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
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 11);
    }

    #[test]
    fn drop_decreases_limit_by_backoff_ratio() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);
        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(10));
        // floor(10 * 0.9) = 9
        assert_eq!(alg.current(), 9);
    }

    #[test]
    fn ignore_has_no_effect() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);
        alg.update(0, 0, 1, Outcome::Ignore, Duration::from_millis(10));
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
        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(10));
        assert_eq!(alg.current(), 2); // max(2, floor(2*0.5)=1) = 2

        // Increase should not go above max_limit
        alg.update(2, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 3);
        alg.update(3, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 3); // clamped at max
    }

    #[test]
    fn multiple_drops_reduce_progressively() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);

        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(10)); // floor(10 * 0.9) = 9
        assert_eq!(alg.current(), 9);

        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(10)); // floor(9 * 0.9) = 8
        assert_eq!(alg.current(), 8);

        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(10)); // floor(8 * 0.9) = 7
        assert_eq!(alg.current(), 7);
    }

    #[test]
    fn recovery_after_drop() {
        let alg = aimd(default_config());

        alg.update(10, 0, 1, Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(alg.current(), 9);

        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10)); // 9 -> 10
        assert_eq!(alg.current(), 10);

        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10)); // 10 -> 11
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
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // Drop resets the counter
        alg.update(10, 0, 1, Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(alg.current(), 9);

        // Need 3 consecutive successes again
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 9); // still 9, only 2 successes

        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10); // now 3 consecutive -> increase
    }

    #[test]
    fn ignore_does_not_affect_consecutive_successes() {
        let config = AimdConfig {
            successes_per_increase: 2,
            ..default_config()
        };
        let alg = aimd(config);

        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        alg.update(10, 0, 1, Outcome::Ignore, Duration::from_millis(10)); // should not reset counter
        alg.update(10, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 11); // 2 successes reached
    }

    #[test]
    fn no_increase_when_underutilized() {
        let alg = aimd(default_config());
        assert_eq!(alg.current(), 10);

        // With 0 inflight (0*2=0 < limit=10), success should not increase
        alg.update(0, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 4 inflight (4*2=8 < limit=10), should not increase
        alg.update(4, 0, 1, Outcome::Success, Duration::from_millis(10));
        assert_eq!(alg.current(), 10);

        // With 5 inflight (5*2=10 >= limit=10), should increase
        alg.update(5, 0, 1, Outcome::Success, Duration::from_millis(10));
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
            alg.update(20, 0, 1, Outcome::Success, rtt);
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
            alg.update(100, 0, 1, Outcome::Success, baseline_rtt);
        }
        let before = alg.current();

        // Now spike the RTT — gradient < 1.0 should reduce the limit
        let high_rtt = Duration::from_millis(500);
        for _ in 0..50 {
            alg.update(100, 0, 1, Outcome::Success, high_rtt);
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
            alg.update(0, 0, 1, Outcome::Success, Duration::from_millis(50));
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
            alg.update(15, 0, 1, Outcome::Success, rtt);
        }
        assert!(alg.current() <= 15, "Should not exceed max_limit");
    }

    #[test]
    fn gradient_ignore_has_no_effect() {
        let alg = gradient(default_gradient_config());
        let initial = alg.current();

        alg.update(20, 0, 1, Outcome::Ignore, Duration::from_millis(50));
        assert_eq!(alg.current(), initial);
    }

    #[test]
    fn gradient_drop_backoff_reduces_limit() {
        let alg = gradient(default_gradient_config());
        assert_eq!(alg.current(), 20);

        // A drop should reduce limit by backoff_ratio (0.9)
        alg.update(20, 0, 1, Outcome::Dropped, Duration::from_millis(1));
        // floor(20 * 0.9) = 18
        assert_eq!(alg.current(), 18);

        alg.update(20, 0, 1, Outcome::Dropped, Duration::from_millis(1));
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
        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(1));
        assert_eq!(alg.current(), 5);

        // Should not go below min
        alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(1));
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
        alg.update(20, 0, 1, Outcome::Dropped, Duration::from_millis(1));
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
            alg.update(20, 0, 1, Outcome::Success, Duration::from_millis(200));
        }
        let long_rtt_before = {
            let state = alg.inner.lock().unwrap();
            state.long_rtt.get()
        };

        // Now send a much lower RTT — should trigger drift decay
        alg.update(20, 0, 1, Outcome::Success, Duration::from_millis(50));
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
            alg.update(200, 0, 1, Outcome::Success, Duration::from_millis(30));
        }
        let limit_before_spike = alg.current();

        // Spike to 90ms (3x baseline). The gradient drops to ~0.5, pulling
        // the limit down. 100 samples is enough to see a clear decrease but
        // not enough for the EMA (half-life ~208 samples) to absorb the new
        // latency.
        for _ in 0..100 {
            alg.update(200, 0, 1, Outcome::Success, Duration::from_millis(90));
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
            alg.update(200, 0, 1, Outcome::Success, Duration::from_millis(90));
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
            alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(1));
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
            successes_per_increase: 1,
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

    // ======================== Vegas algorithm tests ========================

    fn default_vegas_config() -> VegasConfig {
        VegasConfig {
            initial_limit: 20,
            min_limit: 1,
            max_limit: 1000,
            smoothing: 1.0,
            alpha_factor: 3.0,
            beta_factor: 6.0,
            probe_multiplier: 30,
            rtt_noload_window: 0,
        }
    }

    fn vegas(config: VegasConfig) -> Vegas {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Vegas::new(&config, initial)
    }

    #[test]
    fn vegas_steady_state_grows() {
        let alg = vegas(default_vegas_config());

        // Feed many samples at the same RTT; queue_size ≈ 0, so limit should grow.
        let rtt = Duration::from_millis(50);
        for _ in 0..100 {
            alg.update(20, 0, 1, Outcome::Success, rtt);
        }
        assert!(alg.current() > 20, "Limit should grow under steady RTT");
    }

    #[test]
    fn vegas_high_latency_decreases_limit() {
        let config = VegasConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 200,
            ..default_vegas_config()
        };
        let alg = vegas(config);

        // Establish a low rtt_noload (inflight=0 so app-limiting guard keeps limit at 100)
        let low_rtt = Duration::from_millis(10);
        for _ in 0..20 {
            alg.update(0, 0, 1, Outcome::Success, low_rtt);
        }
        let before = alg.current();

        // Now spike RTT — queue_size grows, limit should decrease
        let high_rtt = Duration::from_millis(200);
        for _ in 0..50 {
            alg.update(100, 0, 1, Outcome::Success, high_rtt);
        }
        assert!(
            alg.current() < before,
            "Limit should decrease when latency spikes (before={before}, after={})",
            alg.current()
        );
    }

    #[test]
    fn vegas_sustained_high_latency_stays_low() {
        // This is the key behavioral difference from Gradient: Vegas should NOT recover
        // when latency stays high, because rtt_noload anchors the baseline permanently
        // (until a probe fires). With a large enough probe_multiplier, the probe won't
        // fire during this test window.
        let config = VegasConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 200,
            // Large probe_multiplier so the probe doesn't reset during this test.
            // Min threshold = 0.5 * 10000 * 1 = 5000, exceeding total samples.
            probe_multiplier: 10000,
            ..default_vegas_config()
        };
        let alg = vegas(config);

        // Establish a low rtt_noload (inflight=0 so the app-limiting guard
        // prevents the limit from growing during the establish phase)
        let low_rtt = Duration::from_millis(10);
        for _ in 0..20 {
            alg.update(0, 0, 1, Outcome::Success, low_rtt);
        }
        assert_eq!(alg.current(), 100);

        // Spike to 200ms (20x baseline) — limit should decrease
        let high_rtt = Duration::from_millis(200);
        for _ in 0..100 {
            alg.update(200, 0, 1, Outcome::Success, high_rtt);
        }
        let limit_after_spike = alg.current();
        assert!(
            limit_after_spike < 50,
            "Limit should be well below initial after spike"
        );

        // Continue at 200ms for a long time — unlike Gradient, the limit should NOT recover.
        // At very low limits, the queue estimate is small so the limit oscillates around a
        // low equilibrium (~6) rather than staying at 1, but it never climbs back to 100.
        for _ in 0..2000 {
            alg.update(200, 0, 1, Outcome::Success, high_rtt);
        }
        let limit_sustained = alg.current();
        assert!(
            limit_sustained < 20,
            "Vegas should NOT recover under sustained high latency \
             (after_spike={limit_after_spike}, sustained={limit_sustained})"
        );
    }

    #[test]
    fn vegas_grows_from_one() {
        let config = VegasConfig {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 200,
            ..default_vegas_config()
        };
        let alg = vegas(config);
        assert_eq!(alg.current(), 1);

        let rtt = Duration::from_millis(50);
        for _ in 0..20 {
            alg.update(1, 0, 1, Outcome::Success, rtt);
        }
        assert!(
            alg.current() > 1,
            "Limit should grow from 1 (got {})",
            alg.current()
        );
    }

    #[test]
    fn vegas_app_limiting_guard() {
        let alg = vegas(default_vegas_config());
        let initial = alg.current();

        // Establish rtt_noload, then test with 0 inflight — guard should prevent changes
        alg.update(0, 0, 1, Outcome::Success, Duration::from_millis(50));
        for _ in 0..50 {
            alg.update(0, 0, 1, Outcome::Success, Duration::from_millis(100));
        }
        assert_eq!(alg.current(), initial);
    }

    #[test]
    fn vegas_drop_backoff() {
        let alg = vegas(default_vegas_config());
        assert_eq!(alg.current(), 20);

        // Establish rtt_noload first (required so the drop path is reached after probing)
        alg.update(20, 0, 1, Outcome::Success, Duration::from_millis(50));

        // Drops use additive decrease: limit -= log10(limit)
        // log10_root(20) = max(1, (int) log10(20)) = max(1, 1) = 1
        alg.update(20, 0, 1, Outcome::Dropped, Duration::from_millis(50));
        assert_eq!(alg.current(), 19); // 20 - 1 = 19

        alg.update(20, 0, 1, Outcome::Dropped, Duration::from_millis(50));
        assert_eq!(alg.current(), 18); // 19 - 1 = 18
    }

    #[test]
    fn vegas_min_max_bounds() {
        let config = VegasConfig {
            initial_limit: 10,
            min_limit: 5,
            max_limit: 15,
            ..default_vegas_config()
        };
        let alg = vegas(config);

        // Steady RTT should grow but cap at max_limit
        let rtt = Duration::from_millis(50);
        for _ in 0..200 {
            alg.update(15, 0, 1, Outcome::Success, rtt);
        }
        assert!(alg.current() <= 15, "Should not exceed max_limit");

        // Additive decrease: log10_root(limit) = 1 for small limits, so each drop
        // subtracts 1. Need enough drops to reach min_limit.
        for _ in 0..100 {
            alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(50));
        }
        assert!(
            alg.current() >= 5,
            "Should not go below min_limit (got {})",
            alg.current()
        );
    }

    #[test]
    fn vegas_probe_resets_baseline() {
        let config = VegasConfig {
            initial_limit: 20,
            min_limit: 1,
            max_limit: 200,
            // Small probe_multiplier so probes trigger quickly even with jitter.
            // Threshold = jitter * 1 * 20 = [10, 20) samples.
            probe_multiplier: 1,
            ..default_vegas_config()
        };
        let alg = vegas(config);

        // Establish a low rtt_noload
        let low_rtt = Duration::from_millis(10);
        alg.update(20, 0, 1, Outcome::Success, low_rtt);

        let rtt_noload_before = {
            let state = alg.inner.lock().unwrap();
            state.rtt_noload
        };
        assert!((rtt_noload_before - 0.010).abs() < 0.001);

        // Feed enough samples at a higher RTT to trigger the probe reset.
        // With probe_multiplier=1 and jitter in [0.5, 1.0), threshold is at most 20 samples.
        // 100 samples guarantees at least one probe fires.
        let higher_rtt = Duration::from_millis(50);
        for _ in 0..100 {
            alg.update(20, 0, 1, Outcome::Success, higher_rtt);
        }

        let rtt_noload_after = {
            let state = alg.inner.lock().unwrap();
            state.rtt_noload
        };

        // After probe reset, rtt_noload should have been reset to the current RTT
        assert!(
            rtt_noload_after > rtt_noload_before,
            "Probe should reset rtt_noload (before={rtt_noload_before}, after={rtt_noload_after})"
        );
    }

    #[test]
    fn vegas_ignore_has_no_effect() {
        let alg = vegas(default_vegas_config());
        let initial = alg.current();

        alg.update(20, 0, 1, Outcome::Ignore, Duration::from_millis(50));
        assert_eq!(alg.current(), initial);
    }

    #[test]
    fn vegas_toml_deserialization() {
        let toml_str = r#"
            [concurrency.vegas]
            initial-limit = 10
            min-limit = 1
            max-limit = 1000
            smoothing = 1.0
            alpha-factor = 3.0
            beta-factor = 6.0
            probe-multiplier = 30
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Vegas(VegasConfig {
                max_limit: 1000,
                ..
            })
        ));
    }

    #[test]
    fn concurrency_limit_vegas_build() {
        let config = ConcurrencyLimit::Vegas(VegasConfig {
            initial_limit: 20,
            max_limit: 500,
            ..VegasConfig::default()
        });
        let limiter = config.build();
        assert_eq!(limiter.current(), 20);
    }

    // ======================== BBR algorithm tests ========================

    fn default_bbr_config() -> BbrConfig {
        BbrConfig {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 1000,
            rtt_window: 5000,
            throughput_window: 5000,
            gain: 1.25,
            backoff_ratio: 0.9,
        }
    }

    fn bbr(config: BbrConfig) -> Bbr {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Bbr::new(&config, initial)
    }

    #[test]
    fn bbr_grows_from_one() {
        let alg = bbr(default_bbr_config());
        assert_eq!(alg.current(), 1);

        // At limit=1, inflight=1, constant 5ms RTT:
        // delivery_rate = 1/0.005 = 200, min_rtt = 0.005
        // BDP = 200 * 0.005 = 1.0, limit = ceil(1.0 * 1.25) = 2
        // Next: inflight=2, delivery_rate = 2/0.005 = 400
        // BDP = 400 * 0.005 = 2.0, limit = ceil(2.0 * 1.25) = 3 ... exponential growth
        let rtt = Duration::from_millis(5);
        let mut prev = 1;
        for i in 1..=10 {
            let limit = alg.update(i, i, 1, Outcome::Success, rtt);
            assert!(
                limit >= prev,
                "Limit should grow (iteration {i}, prev={prev}, now={limit})"
            );
            prev = limit;
        }
        assert!(
            alg.current() > 1,
            "Limit should grow from 1 (got {})",
            alg.current()
        );
    }

    #[test]
    fn bbr_saturated_backend_stabilizes() {
        let config = BbrConfig {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 200,
            rtt_window: 50,
            throughput_window: 50,
            ..default_bbr_config()
        };
        let alg = bbr(config);

        // Phase 1: unsaturated backend — delivery_rate grows with concurrency.
        // inflight increases linearly at constant RTT, driving limit up.
        let rtt = Duration::from_millis(5);
        for i in 1..=30 {
            alg.update(i, i, 1, Outcome::Success, rtt);
        }
        let limit_after_growth = alg.current();
        assert!(
            limit_after_growth > 10,
            "Should have grown (got {limit_after_growth})"
        );

        // Phase 2: saturated backend — throughput plateaus at a lower concurrency.
        // Only 20 operations complete concurrently (the backend's true capacity),
        // even though the limit is higher. As old high-delivery-rate samples from
        // phase 1 age out, max_rate drops, and BDP converges to the actual capacity.
        let saturated_inflight = 20;
        for _ in 0..100 {
            alg.update(saturated_inflight, 10, 1, Outcome::Success, rtt);
        }
        let limit_stabilized = alg.current();
        assert!(
            limit_stabilized < limit_after_growth,
            "Limit should decrease when throughput plateaus \
             (growth={limit_after_growth}, stabilized={limit_stabilized})"
        );
    }

    #[test]
    fn bbr_drop_backoff() {
        let config = BbrConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 1000,
            ..default_bbr_config()
        };
        let alg = bbr(config);
        assert_eq!(alg.current(), 100);

        // floor(100 * 0.9) = 90
        alg.update(100, 100, 1, Outcome::Dropped, Duration::from_millis(5));
        assert_eq!(alg.current(), 90);

        // floor(90 * 0.9) = 81
        alg.update(90, 90, 1, Outcome::Dropped, Duration::from_millis(5));
        assert_eq!(alg.current(), 81);
    }

    #[test]
    fn bbr_ignore_no_effect() {
        let config = BbrConfig {
            initial_limit: 50,
            ..default_bbr_config()
        };
        let alg = bbr(config);
        assert_eq!(alg.current(), 50);

        alg.update(50, 50, 1, Outcome::Ignore, Duration::from_millis(5));
        assert_eq!(alg.current(), 50);
    }

    #[test]
    fn bbr_min_max_bounds() {
        let config = BbrConfig {
            initial_limit: 10,
            min_limit: 5,
            max_limit: 15,
            ..default_bbr_config()
        };
        let alg = bbr(config);

        // Growth should cap at max_limit
        let rtt = Duration::from_millis(5);
        for i in 1..=50 {
            alg.update(i.min(15), i.min(15), 1, Outcome::Success, rtt);
        }
        assert!(alg.current() <= 15, "Should not exceed max_limit");

        // Drops should not go below min_limit
        for _ in 0..100 {
            alg.update(0, 0, 1, Outcome::Dropped, Duration::from_millis(5));
        }
        assert!(
            alg.current() >= 5,
            "Should not go below min_limit (got {})",
            alg.current()
        );
    }

    #[test]
    fn bbr_toml_deserialization() {
        let toml_str = r#"
            [concurrency.bbr]
            initial-limit = 1
            min-limit = 1
            max-limit = 500
            rtt-window = 3000
            throughput-window = 3000
            gain = 1.5
            backoff-ratio = 0.85
        "#;
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            parsed.concurrency,
            ConcurrencyLimit::Bbr(BbrConfig { max_limit: 500, .. })
        ));
    }

    #[test]
    fn concurrency_limit_bbr_build() {
        let config = ConcurrencyLimit::Bbr(BbrConfig {
            initial_limit: 10,
            max_limit: 500,
            ..BbrConfig::default()
        });
        let limiter = config.build();
        assert_eq!(limiter.current(), 10);
    }

    // ======================== record_sample_weighted tests ========================

    #[test]
    fn record_sample_weighted_weight_one_equals_unweighted() {
        let limiter = Limiter::fixed(10);
        let t1 = limiter.acquire();
        let t2 = limiter.acquire();
        assert_eq!(limiter.inflight(), 2);

        t1.record_sample(Outcome::Success);
        assert_eq!(limiter.inflight(), 1);

        t2.record_sample_weighted(Outcome::Success, 1);
        assert_eq!(limiter.inflight(), 0);
    }

    #[test]
    fn record_sample_weighted_zero_treated_as_one() {
        let limiter = Limiter::fixed(10);
        let token = limiter.acquire();
        assert_eq!(limiter.inflight(), 1);

        token.record_sample_weighted(Outcome::Success, 0);
        assert_eq!(limiter.inflight(), 0);
    }

    #[test]
    fn record_sample_weighted_decrements_inflight() {
        let limiter = Limiter::fixed(10);
        let token = limiter.acquire();
        assert_eq!(limiter.inflight(), 1);

        token.record_sample_weighted(Outcome::Dropped, 100);
        assert_eq!(limiter.inflight(), 0);
    }
}

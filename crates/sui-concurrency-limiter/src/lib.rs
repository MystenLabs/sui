// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

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

/// A dynamic concurrency limit algorithm.
///
/// Receives operation outcomes and publishes limit changes via a watch channel.
pub trait Limit: Send + Sync + 'static {
    fn current(&self) -> usize;
    fn on_sample(&self, outcome: Outcome, rtt: Duration);
    fn acquire(&self);
    fn release(&self);
    fn subscribe(&self) -> watch::Receiver<usize>;
}

/// Configuration for the AIMD (Additive Increase / Multiplicative Decrease) algorithm.
///
/// Uses Netflix-style gentle backoff (default `backoff_ratio = 0.9`, i.e. 10% cut) rather
/// than TCP-style halving. Combined with additive +1 increase per `successes_per_increase`
/// consecutive successes, this recovers quickly from transient errors.
#[derive(Serialize, Deserialize, Debug, Clone)]
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
            max_limit: 1000,
            backoff_ratio: 0.9,
            successes_per_increase: 1,
        }
    }
}

struct AimdState {
    current: usize,
    consecutive_successes: usize,
}

/// AIMD concurrency limit algorithm.
///
/// On each sample:
/// - **Dropped**: `limit = max(min_limit, floor(limit * backoff_ratio))`
/// - **Success**: after `successes_per_increase` consecutive successes, `limit = min(max_limit, limit + 1)`
/// - **Ignore**: no change
pub struct AimdLimit {
    config: AimdConfig,
    inner: Mutex<AimdState>,
    tx: watch::Sender<usize>,
    inflight: AtomicUsize,
}

impl AimdLimit {
    pub fn new(config: AimdConfig) -> (Self, watch::Receiver<usize>) {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        let (tx, rx) = watch::channel(initial);
        let limiter = Self {
            inner: Mutex::new(AimdState {
                current: initial,
                consecutive_successes: 0,
            }),
            config,
            tx,
            inflight: AtomicUsize::new(0),
        };
        (limiter, rx)
    }
}

impl Limit for AimdLimit {
    fn current(&self) -> usize {
        let state = self.inner.lock().unwrap();
        state.current
    }

    fn on_sample(&self, outcome: Outcome, _rtt: Duration) {
        let mut state = self.inner.lock().unwrap();
        match outcome {
            Outcome::Dropped => {
                state.consecutive_successes = 0;
                let new = ((state.current as f64) * self.config.backoff_ratio).floor() as usize;
                state.current = new.max(self.config.min_limit);
                let _ = self.tx.send(state.current);
            }
            Outcome::Success => {
                state.consecutive_successes += 1;
                // Only increase when the system is actually under pressure. Without
                // this guard a lightly-loaded pipeline would ratchet the limit up to
                // max_limit without ever testing the boundary.
                let inflight = self.inflight.load(Ordering::Relaxed);
                if state.consecutive_successes >= self.config.successes_per_increase
                    && inflight >= state.current / 2
                {
                    state.consecutive_successes = 0;
                    state.current = (state.current + 1).min(self.config.max_limit);
                    let _ = self.tx.send(state.current);
                }
            }
            Outcome::Ignore => {}
        }
    }

    fn acquire(&self) {
        self.inflight.fetch_add(1, Ordering::Relaxed);
    }

    fn release(&self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
    }

    fn subscribe(&self) -> watch::Receiver<usize> {
        self.tx.subscribe()
    }
}

// ---------------------------------------------------------------------------
// Gradient2 algorithm — Netflix's latency-predictive concurrency limiter
// ---------------------------------------------------------------------------

/// Configuration for the Gradient2 concurrency limit algorithm.
///
/// Gradient2 adjusts the limit based on the ratio of long-term to short-term RTT, making it
/// sensitive to latency changes rather than errors alone. When latency increases (gradient < 1),
/// the limit decreases; when latency is stable or improving (gradient ~1), the limit grows
/// additively by `queue_size`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Gradient2Config {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Exponential smoothing factor in `(0.0, 1.0]` for blending old and new limit estimates.
    pub smoothing: f64,
    /// Additive growth per update, controlling how fast the limit climbs.
    pub queue_size: usize,
    /// Multiplier on the long/short RTT ratio; values > 1.0 tolerate some latency increase.
    pub tolerance: f64,
    /// Window size for the long-term RTT exponential moving average.
    pub long_window: usize,
}

impl Default for Gradient2Config {
    fn default() -> Self {
        Self {
            initial_limit: 20,
            min_limit: 20,
            max_limit: 200,
            smoothing: 0.2,
            queue_size: 4,
            tolerance: 1.5,
            long_window: 600,
        }
    }
}

/// Exponential moving average with a warmup phase.
///
/// During the warmup period (first `warmup` samples), a simple arithmetic mean is used.
/// After warmup, the value transitions to an EMA with `factor = 2.0 / (window + 1)`.
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

    fn add(&mut self, sample: f64) -> f64 {
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

struct Gradient2State {
    estimated_limit: f64,
    long_rtt: ExpAvgMeasurement,
}

/// Gradient2 concurrency limit algorithm (Netflix).
///
/// Adjusts the limit by computing a gradient from long-term vs short-term RTT:
/// 1. Record `short_rtt = rtt`, update `long_rtt` EMA
/// 2. Drift recovery: if `long_rtt / short_rtt > 2.0`, decay long_rtt by 5%
/// 3. App-limiting guard: if `inflight < estimated_limit / 2`, return unchanged
/// 4. Gradient: `max(0.5, min(1.0, tolerance * long_rtt / short_rtt))`
/// 5. New limit: `estimated_limit * gradient + queue_size`
/// 6. Smooth: `estimated_limit * (1 - smoothing) + new_limit * smoothing`
/// 7. Clamp to `[min_limit, max_limit]`
pub struct Gradient2Limit {
    config: Gradient2Config,
    inner: Mutex<Gradient2State>,
    tx: watch::Sender<usize>,
    inflight: AtomicUsize,
}

impl Gradient2Limit {
    pub fn new(config: Gradient2Config) -> (Self, watch::Receiver<usize>) {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        let (tx, rx) = watch::channel(initial);
        let limiter = Self {
            inner: Mutex::new(Gradient2State {
                estimated_limit: initial as f64,
                long_rtt: ExpAvgMeasurement::new(config.long_window, 10),
            }),
            config,
            tx,
            inflight: AtomicUsize::new(0),
        };
        (limiter, rx)
    }
}

impl Limit for Gradient2Limit {
    fn current(&self) -> usize {
        let state = self.inner.lock().unwrap();
        (state.estimated_limit as usize).clamp(self.config.min_limit, self.config.max_limit)
    }

    fn on_sample(&self, outcome: Outcome, rtt: Duration) {
        if matches!(outcome, Outcome::Ignore) {
            return;
        }

        let short_rtt = rtt.as_secs_f64();
        if short_rtt <= 0.0 {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        let long_rtt = state.long_rtt.add(short_rtt);

        // Drift recovery: if the long-term RTT has drifted much higher than current
        // observations, decay it to prevent the limit from being permanently inflated.
        if long_rtt / short_rtt > 2.0 {
            state.long_rtt.update(|v| v * 0.95);
        }

        // App-limiting guard: don't adjust when the system isn't under pressure.
        let inflight = self.inflight.load(Ordering::Relaxed);
        if (inflight as f64) < state.estimated_limit / 2.0 {
            return;
        }

        let gradient = (self.config.tolerance * long_rtt / short_rtt).clamp(0.5, 1.0);
        let new_limit = state.estimated_limit * gradient + self.config.queue_size as f64;
        state.estimated_limit = state.estimated_limit * (1.0 - self.config.smoothing)
            + new_limit * self.config.smoothing;

        let clamped =
            (state.estimated_limit as usize).clamp(self.config.min_limit, self.config.max_limit);
        let _ = self.tx.send(clamped);
    }

    fn acquire(&self) {
        self.inflight.fetch_add(1, Ordering::Relaxed);
    }

    fn release(&self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
    }

    fn subscribe(&self) -> watch::Receiver<usize> {
        self.tx.subscribe()
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

    /// Simulate `n` inflight tasks for testing the inflight guard.
    fn simulate_inflight(limiter: &dyn Limit, n: usize) {
        for _ in 0..n {
            limiter.acquire();
        }
    }

    // ======================== AIMD tests ========================

    #[test]
    fn success_increases_limit_by_one() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        simulate_inflight(&limiter, 10);
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 11);
    }

    #[test]
    fn drop_decreases_limit_by_backoff_ratio() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10));
        // floor(10 * 0.9) = 9
        assert_eq!(limiter.current(), 9);
    }

    #[test]
    fn ignore_has_no_effect() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        limiter.on_sample(Outcome::Ignore, Duration::from_millis(10));
        assert_eq!(limiter.current(), 10);
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
        let (limiter, _rx) = AimdLimit::new(config);

        // Decrease should not go below min_limit
        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10));
        assert_eq!(limiter.current(), 2); // max(2, floor(2*0.5)=1) = 2

        // Increase should not go above max_limit
        simulate_inflight(&limiter, 2);
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 3);
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 3); // clamped at max
    }

    #[test]
    fn multiple_drops_reduce_progressively() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);

        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10)); // floor(10 * 0.9) = 9
        assert_eq!(limiter.current(), 9);

        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10)); // floor(9 * 0.9) = 8
        assert_eq!(limiter.current(), 8);

        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10)); // floor(8 * 0.9) = 7
        assert_eq!(limiter.current(), 7);
    }

    #[test]
    fn recovery_after_drop() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        simulate_inflight(&limiter, 10);

        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(limiter.current(), 9);

        limiter.on_sample(Outcome::Success, Duration::from_millis(10)); // 9 -> 10
        assert_eq!(limiter.current(), 10);

        limiter.on_sample(Outcome::Success, Duration::from_millis(10)); // 10 -> 11
        assert_eq!(limiter.current(), 11);
    }

    #[test]
    fn consecutive_success_counter_resets_on_drop() {
        let config = AimdConfig {
            successes_per_increase: 3,
            ..default_config()
        };
        let (limiter, _rx) = AimdLimit::new(config);
        assert_eq!(limiter.current(), 10);
        simulate_inflight(&limiter, 10);

        // Two successes, not enough to increase
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 10);

        // Drop resets the counter
        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10)); // 10 -> 9
        assert_eq!(limiter.current(), 9);

        // Need 3 consecutive successes again
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 9); // still 9, only 2 successes

        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 10); // now 3 consecutive -> increase
    }

    #[test]
    fn watch_channel_receives_updates() {
        let (limiter, mut rx) = AimdLimit::new(default_config());
        assert_eq!(*rx.borrow(), 10);
        simulate_inflight(&limiter, 10);

        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(*rx.borrow_and_update(), 11);

        limiter.on_sample(Outcome::Dropped, Duration::from_millis(10));
        assert_eq!(*rx.borrow_and_update(), 9); // floor(11 * 0.9) = 9
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
        let (limiter, rx) = AimdLimit::new(config);
        assert_eq!(limiter.current(), 5);
        assert_eq!(*rx.borrow(), 5);

        let config = AimdConfig {
            initial_limit: 100,
            min_limit: 5,
            max_limit: 10,
            backoff_ratio: 0.9,
            successes_per_increase: 1,
        };
        let (limiter, rx) = AimdLimit::new(config);
        assert_eq!(limiter.current(), 10);
        assert_eq!(*rx.borrow(), 10);
    }

    #[test]
    fn ignore_does_not_affect_consecutive_successes() {
        let config = AimdConfig {
            successes_per_increase: 2,
            ..default_config()
        };
        let (limiter, _rx) = AimdLimit::new(config);
        simulate_inflight(&limiter, 10);

        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        limiter.on_sample(Outcome::Ignore, Duration::from_millis(10)); // should not reset counter
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 11); // 2 successes reached
    }

    #[test]
    fn no_increase_when_underutilized() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);

        // With 0 inflight (well under limit/2 = 5), success should not increase
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 10);

        // With 4 inflight (still under limit/2 = 5), should not increase
        simulate_inflight(&limiter, 4);
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 10);

        // With 5 inflight (= limit/2), should increase
        limiter.acquire();
        limiter.on_sample(Outcome::Success, Duration::from_millis(10));
        assert_eq!(limiter.current(), 11);
    }

    // ======================== ExpAvgMeasurement tests ========================

    #[test]
    fn exp_avg_warmup_phase_uses_simple_average() {
        let mut ema = ExpAvgMeasurement::new(100, 3);
        assert_eq!(ema.add(10.0), 10.0); // 10/1
        assert_eq!(ema.add(20.0), 15.0); // 30/2
        assert_eq!(ema.add(30.0), 20.0); // 60/3
    }

    #[test]
    fn exp_avg_transitions_to_ema_after_warmup() {
        let mut ema = ExpAvgMeasurement::new(100, 2);
        ema.add(10.0); // warmup 1
        ema.add(20.0); // warmup 2, value = 15.0

        // After warmup, EMA with factor = 2/101 ~= 0.0198
        let factor = 2.0 / 101.0;
        let expected = 15.0 * (1.0 - factor) + 30.0 * factor;
        let result = ema.add(30.0);
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn exp_avg_update_modifies_value() {
        let mut ema = ExpAvgMeasurement::new(100, 1);
        ema.add(100.0);
        ema.update(|v| v * 0.95);
        assert!((ema.get() - 95.0).abs() < 1e-10);
    }

    // ======================== Gradient2 tests ========================

    fn default_g2_config() -> Gradient2Config {
        Gradient2Config {
            initial_limit: 20,
            min_limit: 5,
            max_limit: 200,
            smoothing: 0.2,
            queue_size: 4,
            tolerance: 1.5,
            long_window: 600,
        }
    }

    #[test]
    fn gradient2_steady_state_grows() {
        let (limiter, _rx) = Gradient2Limit::new(default_g2_config());
        simulate_inflight(&limiter, 20);

        // Feed many samples at the same RTT; gradient should be ~1.0, limit should grow.
        let rtt = Duration::from_millis(50);
        for _ in 0..100 {
            limiter.on_sample(Outcome::Success, rtt);
        }
        assert!(limiter.current() > 20, "Limit should grow under steady RTT");
    }

    #[test]
    fn gradient2_increasing_latency_reduces_limit() {
        let config = Gradient2Config {
            initial_limit: 100,
            min_limit: 5,
            max_limit: 200,
            smoothing: 0.2,
            queue_size: 4,
            tolerance: 1.5,
            long_window: 600,
        };
        let (limiter, _rx) = Gradient2Limit::new(config);
        simulate_inflight(&limiter, 100);

        // Establish a baseline long RTT
        let baseline_rtt = Duration::from_millis(50);
        for _ in 0..20 {
            limiter.on_sample(Outcome::Success, baseline_rtt);
        }
        let before = limiter.current();

        // Now spike the RTT — gradient < 1.0 should reduce the limit
        let high_rtt = Duration::from_millis(500);
        for _ in 0..50 {
            limiter.on_sample(Outcome::Success, high_rtt);
        }
        assert!(
            limiter.current() < before,
            "Limit should decrease when latency spikes (before={before}, after={})",
            limiter.current()
        );
    }

    #[test]
    fn gradient2_app_limiting_guard() {
        let (limiter, _rx) = Gradient2Limit::new(default_g2_config());
        // Don't simulate any inflight — guard should prevent changes
        let initial = limiter.current();
        for _ in 0..50 {
            limiter.on_sample(Outcome::Success, Duration::from_millis(50));
        }
        assert_eq!(limiter.current(), initial);
    }

    #[test]
    fn gradient2_min_max_bounds() {
        let config = Gradient2Config {
            initial_limit: 10,
            min_limit: 10,
            max_limit: 15,
            smoothing: 1.0, // aggressive smoothing to hit bounds fast
            queue_size: 10,
            tolerance: 1.5,
            long_window: 10,
        };
        let (limiter, _rx) = Gradient2Limit::new(config);
        simulate_inflight(&limiter, 15);

        let rtt = Duration::from_millis(50);
        for _ in 0..100 {
            limiter.on_sample(Outcome::Success, rtt);
        }
        assert!(limiter.current() <= 15, "Should not exceed max_limit");
    }

    #[test]
    fn gradient2_watch_channel_receives_updates() {
        let (limiter, rx) = Gradient2Limit::new(default_g2_config());
        assert_eq!(*rx.borrow(), 20);
        simulate_inflight(&limiter, 20);

        for _ in 0..50 {
            limiter.on_sample(Outcome::Success, Duration::from_millis(50));
        }
        // The watch channel should have been updated
        assert!(*rx.borrow() > 20);
    }

    #[test]
    fn gradient2_ignore_has_no_effect() {
        let (limiter, _rx) = Gradient2Limit::new(default_g2_config());
        simulate_inflight(&limiter, 20);
        let initial = limiter.current();

        limiter.on_sample(Outcome::Ignore, Duration::from_millis(50));
        assert_eq!(limiter.current(), initial);
    }

    #[test]
    fn gradient2_drift_recovery() {
        let config = Gradient2Config {
            long_window: 10,
            ..default_g2_config()
        };
        let (limiter, _rx) = Gradient2Limit::new(config);
        simulate_inflight(&limiter, 20);

        // Establish a high long RTT
        for _ in 0..20 {
            limiter.on_sample(Outcome::Success, Duration::from_millis(200));
        }
        let long_rtt_before = {
            let state = limiter.inner.lock().unwrap();
            state.long_rtt.get()
        };

        // Now send a much lower RTT — should trigger drift decay
        limiter.on_sample(Outcome::Success, Duration::from_millis(50));
        let long_rtt_after = {
            let state = limiter.inner.lock().unwrap();
            state.long_rtt.get()
        };

        // The long_rtt should have been decayed because long/short > 2.0
        // Note: the EMA update itself moves it, but the decay should make it
        // noticeably lower than if only the EMA updated it
        assert!(long_rtt_after < long_rtt_before);
    }
}

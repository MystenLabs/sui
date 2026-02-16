// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adaptive concurrency limiter using throughput probing.
//!
//! Unlike the per-sample algorithms (AIMD, Gradient, Vegas, BBR), Adaptive collects statistics
//! over **probe intervals** (~1s) and makes limit decisions at interval boundaries:
//!
//! 1. **ProbeBW**: Steady-state with periodic probing (Cruise → ProbeUp → ProbeDown cycle).
//!    Cruise uses additive increase (sqrt(limit) per interval) with decay when underutilized.
//! 2. **Emergency brake**: Per-sample error check + interval-based tail-latency check (always active).
//!
//! The key insight: measure completions-per-second as an independent throughput signal, then ask
//! whether it grows when concurrency increases. If yes, grow. If no, you've found the knee.
//! ProbeDown verifies: if throughput is maintained at reduced concurrency, the extra concurrency
//! wasn't helping — keep the lower limit.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{LimitAlgorithm, Outcome};

/// Configuration for the Adaptive concurrency limit algorithm.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct AdaptiveConfig {
    /// Starting concurrency limit.
    pub initial_limit: usize,
    /// Floor: the limit will never go below this value.
    pub min_limit: usize,
    /// Ceiling: the limit will never exceed this value.
    pub max_limit: usize,
    /// Duration of each probe interval in milliseconds.
    pub probe_interval_ms: u64,

    // Throughput tracking
    /// EMA smoothing weight for current interval (0.0–1.0). Lower = smoother, less noise.
    pub throughput_ema_alpha: f64,
    /// Minimum EMA growth ratio to allow Cruise to increase the limit.
    pub throughput_growth_threshold: f64,

    // ProbeBW
    /// Multiplicative gain when probing upward.
    pub probe_up_gain: f64,
    /// Number of cruise intervals between probe cycles.
    pub probe_bw_intervals: usize,
    /// Minimum throughput fraction to accept when probing down (0.0–1.0).
    /// ProbeDown keeps the lower concurrency limit if throughput stays above this
    /// fraction of pre-probe throughput. Lower values (e.g. 0.80) trade throughput
    /// for lower latency; higher values (e.g. 0.95) keep concurrency closer to peak.
    pub probe_down_min_throughput: f64,

    // Braking
    /// Multiplicative backoff on error brake.
    pub error_backoff_ratio: f64,
    /// Multiplicative backoff on latency brake.
    pub latency_backoff_ratio: f64,
    /// Error rate threshold (fraction) that triggers the error brake.
    pub error_rate_threshold: f64,
    /// Queue signal threshold for the latency brake.
    pub queue_signal_beta: f64,
    /// Percentile used for tail-latency braking (0.0–1.0).
    pub brake_percentile: f64,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 100_000,
            probe_interval_ms: 1000,
            throughput_ema_alpha: 0.3,
            throughput_growth_threshold: 1.10,
            probe_up_gain: 1.25,
            probe_bw_intervals: 10,
            probe_down_min_throughput: 0.90,
            error_backoff_ratio: 0.5,
            latency_backoff_ratio: 0.9,
            error_rate_threshold: 0.05,
            queue_signal_beta: 6.0,
            brake_percentile: 0.95,
        }
    }
}

// ---------------------------------------------------------------------------
// State machine phases
// ---------------------------------------------------------------------------

enum Phase {
    ProbeBW(ProbeBWState),
}

enum ProbeBWState {
    Cruise {
        intervals_since_probe: usize,
    },
    ProbeUp {
        pre_probe_limit: usize,
        start_throughput_ema: f64,
    },
    ProbeDown {
        pre_down_limit: usize,
        pre_probe_throughput_ema: f64,
    },
}

// ---------------------------------------------------------------------------
// Per-interval statistics
// ---------------------------------------------------------------------------

struct IntervalStats {
    successes: usize,
    errors: usize,
    rtt_samples: Vec<f64>,
    peak_inflight: usize,
    start: Instant,
    next_interval: Duration,
}

impl IntervalStats {
    fn new(interval: Duration) -> Self {
        Self {
            successes: 0,
            errors: 0,
            rtt_samples: Vec::new(),
            peak_inflight: 0,
            start: Instant::now(),
            next_interval: interval,
        }
    }

    fn reset(&mut self, interval: Duration) {
        self.successes = 0;
        self.errors = 0;
        self.rtt_samples.clear();
        self.peak_inflight = 0;
        self.start = Instant::now();
        // Jitter: +/- 10%
        let jitter = rand::thread_rng().gen_range(0.9..1.1);
        self.next_interval = Duration::from_secs_f64(interval.as_secs_f64() * jitter);
    }

    fn elapsed(&self) -> bool {
        self.start.elapsed() >= self.next_interval
    }

    fn percentile(&mut self, p: f64) -> Option<f64> {
        if self.rtt_samples.len() < 10 {
            return None;
        }
        let idx = ((self.rtt_samples.len() as f64 * p).ceil() as usize)
            .min(self.rtt_samples.len())
            .saturating_sub(1);
        self.rtt_samples.select_nth_unstable_by(idx, |a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(self.rtt_samples[idx])
    }
}

// ---------------------------------------------------------------------------
// Rolling error window (for per-sample brake)
// ---------------------------------------------------------------------------

struct RollingErrors {
    /// Ring buffer of booleans: true = error, false = success/ignore.
    window: Vec<bool>,
    head: usize,
    count: usize,
    error_count: usize,
}

const MIN_ERROR_WINDOW: usize = 100;
const MAX_ERROR_WINDOW: usize = 10_000;

impl RollingErrors {
    fn new(size: usize) -> Self {
        Self {
            window: vec![false; size],
            head: 0,
            count: 0,
            error_count: 0,
        }
    }

    /// Scale window to ~2x the current limit, clamped to [100, 10_000].
    /// At low concurrency this gives reasonable history; at high concurrency
    /// it prevents a tiny burst from tripping the brake.
    fn resize_for_limit(&mut self, limit: usize) {
        let target = (limit * 2).clamp(MIN_ERROR_WINDOW, MAX_ERROR_WINDOW);
        if target != self.window.len() {
            // Reset rather than try to remap the ring buffer — the window
            // refills quickly at high throughput.
            self.window = vec![false; target];
            self.head = 0;
            self.count = 0;
            self.error_count = 0;
        }
    }

    fn push(&mut self, is_error: bool) {
        if self.count == self.window.len() {
            if self.window[self.head] {
                self.error_count -= 1;
            }
        } else {
            self.count += 1;
        }
        self.window[self.head] = is_error;
        if is_error {
            self.error_count += 1;
        }
        self.head = (self.head + 1) % self.window.len();
    }

    fn error_rate(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.error_count as f64 / self.count as f64
    }

    fn reset(&mut self) {
        self.window.fill(false);
        self.head = 0;
        self.count = 0;
        self.error_count = 0;
    }
}

// ---------------------------------------------------------------------------
// Inner mutable state
// ---------------------------------------------------------------------------

struct AdaptiveState {
    phase: Phase,
    stats: IntervalStats,
    rolling_errors: RollingErrors,
    throughput_ema: Option<f64>,
    baseline_p95: Option<f64>,
    baseline_recorded_at: Option<Instant>,
    suppress_latency_brake: bool,
    limit: usize,
}

impl AdaptiveState {
    fn record_sample(&mut self, inflight: usize, outcome: &Outcome, rtt: Duration) {
        self.stats.peak_inflight = self.stats.peak_inflight.max(inflight);
        self.rolling_errors.resize_for_limit(self.limit);
        match outcome {
            Outcome::Success => {
                self.stats.successes += 1;
                let rtt_secs = rtt.as_secs_f64();
                if rtt_secs > 0.0 {
                    self.stats.rtt_samples.push(rtt_secs);
                }
                self.rolling_errors.push(false);
            }
            Outcome::Dropped => {
                self.stats.errors += 1;
                self.rolling_errors.push(true);
            }
            Outcome::Ignore => {
                self.rolling_errors.push(false);
            }
        }
    }

    fn check_error_brake(&mut self, config: &AdaptiveConfig, gauge: &AtomicUsize) {
        if self.rolling_errors.error_rate() > config.error_rate_threshold {
            self.limit = ((self.limit as f64) * config.error_backoff_ratio)
                .ceil()
                .max(config.min_limit as f64) as usize;
            self.limit = self.limit.clamp(config.min_limit, config.max_limit);
            gauge.store(self.limit, Ordering::Release);
            // Reset to ProbeBW Cruise and clear rolling errors
            self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
            self.rolling_errors.reset();
        }
    }

    fn process_interval(&mut self, config: &AdaptiveConfig, gauge: &AtomicUsize) {
        // Skip if zero successful samples — extend interval, don't treat as stall
        if self.stats.successes == 0 {
            return;
        }

        let elapsed_secs = self.stats.start.elapsed().as_secs_f64();
        if elapsed_secs <= 0.0 {
            return;
        }

        let throughput = self.stats.successes as f64 / elapsed_secs;
        self.apply_interval(config, gauge, throughput);
    }

    /// Interval processing with an explicit throughput value.
    /// Separated from `process_interval` so tests can inject deterministic throughput.
    fn apply_interval(&mut self, config: &AdaptiveConfig, gauge: &AtomicUsize, throughput: f64) {
        let prev_ema = self.throughput_ema;

        // Update throughput EMA: first interval seeds directly
        self.throughput_ema = Some(match prev_ema {
            None => throughput,
            Some(prev) => {
                config.throughput_ema_alpha * throughput
                    + (1.0 - config.throughput_ema_alpha) * prev
            }
        });
        let throughput_ema = self.throughput_ema.unwrap();

        // Compute tail latency
        let current_p95 = self.stats.percentile(config.brake_percentile);

        // Inflight guard: skip growth when the system isn't utilizing the current limit.
        // Without this, the limit rockets to max when ingestion is slow.
        let underutilized = self.stats.peak_inflight * 2 < self.limit;

        // Throughput guard: only grow when throughput is actively responding.
        // If adding concurrency doesn't increase throughput, we're past the knee.
        let throughput_not_growing = match prev_ema {
            Some(prev) => throughput_ema < prev * config.throughput_growth_threshold,
            None => false,
        };

        // Phase-specific logic
        match &mut self.phase {
            Phase::ProbeBW(sub) => {
                match sub {
                    ProbeBWState::Cruise {
                        intervals_since_probe,
                    } => {
                        if underutilized {
                            // Limit is above what the system actually uses — decay toward
                            // real usage so the limit stays meaningful as a ceiling.
                            self.limit = ((self.limit as f64) * 0.95).ceil() as usize;
                        } else if !throughput_not_growing {
                            let inc = ((self.limit as f64).sqrt().floor() as usize).max(1);
                            self.limit += inc;
                        }
                        *intervals_since_probe += 1;

                        if !underutilized && *intervals_since_probe >= config.probe_bw_intervals {
                            let pre_probe_limit = self.limit;
                            let start_ema = throughput_ema;
                            self.limit =
                                ((self.limit as f64) * config.probe_up_gain).ceil() as usize;
                            self.phase = Phase::ProbeBW(ProbeBWState::ProbeUp {
                                pre_probe_limit,
                                start_throughput_ema: start_ema,
                            });
                        }
                    }
                    ProbeBWState::ProbeUp {
                        pre_probe_limit,
                        start_throughput_ema,
                    } => {
                        if throughput >= *start_throughput_ema * 1.10 {
                            // Throughput responded — keep the higher limit
                            self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                                intervals_since_probe: 0,
                            });
                        } else {
                            // Throughput didn't respond — revert and probe down to
                            // check if we can use less concurrency.
                            let pre_down = *pre_probe_limit;
                            let pre_probe_ema = *start_throughput_ema;
                            self.limit = ((pre_down as f64) * 0.75).ceil() as usize;
                            self.phase = Phase::ProbeBW(ProbeBWState::ProbeDown {
                                pre_down_limit: pre_down,
                                pre_probe_throughput_ema: pre_probe_ema,
                            });
                        }
                    }
                    ProbeBWState::ProbeDown {
                        pre_down_limit,
                        pre_probe_throughput_ema,
                    } => {
                        // Record latency baseline at reduced concurrency
                        if let Some(p95) = current_p95 {
                            self.baseline_p95 = Some(p95);
                            self.baseline_recorded_at = Some(Instant::now());
                            self.suppress_latency_brake = true;
                        }
                        // If throughput at reduced concurrency is within threshold of what we
                        // had before probing, the extra concurrency wasn't helping —
                        // keep the lower limit.
                        if throughput
                            >= *pre_probe_throughput_ema * config.probe_down_min_throughput
                        {
                            // Throughput maintained — keep reduced limit
                        } else {
                            // Throughput dropped — restore original limit
                            self.limit = *pre_down_limit;
                        }
                        self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                            intervals_since_probe: 0,
                        });
                    }
                }
            }
        }

        // Latency brake (always active when baseline exists and we have enough samples).
        // Suppressed for one interval after ProbeDown recalibrates to avoid immediately
        // undoing the fresh baseline measurement.
        if self.suppress_latency_brake {
            self.suppress_latency_brake = false;
        } else if let (Some(baseline), Some(current)) = (self.baseline_p95, current_p95)
            && baseline > 0.0
            && current > baseline
        {
            let queue_signal = self.limit as f64 * (1.0 - baseline / current);
            if queue_signal > config.queue_signal_beta {
                self.limit = ((self.limit as f64) * config.latency_backoff_ratio).ceil() as usize;
            }
        }

        // Clamp
        self.limit = self.limit.clamp(config.min_limit, config.max_limit);
        gauge.store(self.limit, Ordering::Release);

        // Reset interval stats
        self.stats
            .reset(Duration::from_millis(config.probe_interval_ms));
    }
}

// ---------------------------------------------------------------------------
// Public algorithm struct
// ---------------------------------------------------------------------------

/// Adaptive concurrency limit algorithm.
///
/// Combines BBR-style throughput probing with tail-latency braking using a three-phase
/// state machine. See module docs for details.
pub struct Adaptive {
    config: AdaptiveConfig,
    gauge: Arc<AtomicUsize>,
    inner: Mutex<AdaptiveState>,
}

impl Adaptive {
    pub(crate) fn new(config: &AdaptiveConfig, initial: usize) -> Self {
        let interval = Duration::from_millis(config.probe_interval_ms);
        Self {
            gauge: Arc::new(AtomicUsize::new(initial)),
            inner: Mutex::new(AdaptiveState {
                phase: Phase::ProbeBW(ProbeBWState::Cruise {
                    intervals_since_probe: 0,
                }),
                stats: IntervalStats::new(interval),
                rolling_errors: RollingErrors::new(100),
                throughput_ema: None,
                baseline_p95: None,
                baseline_recorded_at: None,
                suppress_latency_brake: false,
                limit: initial,
            }),
            config: config.clone(),
        }
    }

    #[cfg(test)]
    fn current(&self) -> usize {
        self.gauge.load(Ordering::Acquire)
    }

    /// Force-process the current interval regardless of elapsed time.
    #[cfg(test)]
    fn force_interval(&self) {
        let mut state = self.inner.lock().unwrap();
        state.process_interval(&self.config, &self.gauge);
    }

    /// Force-process with a deterministic throughput value (for timing-independent tests).
    #[cfg(test)]
    fn force_interval_with_throughput(&self, throughput: f64) {
        let mut state = self.inner.lock().unwrap();
        if state.stats.successes == 0 {
            return;
        }
        state.apply_interval(&self.config, &self.gauge, throughput);
    }
}

impl LimitAlgorithm for Adaptive {
    fn update(
        &self,
        inflight: usize,
        _delivered: usize,
        outcome: Outcome,
        rtt: Duration,
    ) -> usize {
        let mut state = self.inner.lock().unwrap();

        // 1. Record sample
        state.record_sample(inflight, &outcome, rtt);

        // 2. Per-sample emergency brake (error rate)
        state.check_error_brake(&self.config, &self.gauge);

        // 3. Check if interval elapsed
        if state.stats.elapsed() {
            state.process_interval(&self.config, &self.gauge);
        }

        self.gauge.load(Ordering::Relaxed)
    }

    fn gauge(&self) -> Arc<AtomicUsize> {
        self.gauge.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AdaptiveConfig {
        AdaptiveConfig {
            // Use 0ms interval so every update triggers interval processing
            probe_interval_ms: 0,
            ..AdaptiveConfig::default()
        }
    }

    fn adaptive(config: AdaptiveConfig) -> Adaptive {
        let initial = config
            .initial_limit
            .clamp(config.min_limit, config.max_limit);
        Adaptive::new(&config, initial)
    }

    /// Feed successes with high inflight (passes the inflight guard).
    fn feed_successes(alg: &Adaptive, count: usize, rtt: Duration) {
        let limit = alg.current();
        for _ in 0..count {
            alg.update(limit, 0, Outcome::Success, rtt);
        }
    }

    /// Feed successes with zero inflight (triggers the inflight guard).
    fn feed_successes_idle(alg: &Adaptive, count: usize, rtt: Duration) {
        for _ in 0..count {
            alg.update(0, 0, Outcome::Success, rtt);
        }
    }

    // ======================== Cruise tests ========================

    #[test]
    fn cruise_additive_increase() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 100, // high so we stay in cruise
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
        }

        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // sqrt(100) = 10, so limit should be 110
        assert_eq!(alg.current(), 110);
    }

    #[test]
    fn cruise_minimum_additive_increase_is_one() {
        let config = AdaptiveConfig {
            initial_limit: 1,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 100,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
        }

        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // sqrt(1) = 1, so limit should be 2
        assert_eq!(alg.current(), 2);
    }

    // ======================== ProbeUp tests ========================

    #[test]
    fn probe_up_keeps_higher_limit_when_throughput_responds() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_up_gain: 1.25,
            ..default_config()
        };
        let alg = adaptive(config);

        // Force into ProbeUp with known parameters
        {
            let mut state = alg.inner.lock().unwrap();
            state.limit = 125;
            state.throughput_ema = Some(100.0);
            state.phase = Phase::ProbeBW(ProbeBWState::ProbeUp {
                pre_probe_limit: 100,
                start_throughput_ema: 100.0,
            });
            alg.gauge.store(125, Ordering::Release);
        }

        // Feed samples, then force with high throughput (200 → EMA = 0.3*200 + 0.7*100 = 130 >= 110)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(200.0);

        // Should transition to Cruise, keeping the higher limit
        let limit = alg.current();
        assert!(limit >= 125, "Should keep higher limit, got {limit}");
        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::Cruise { .. })
        ));
    }

    #[test]
    fn probe_up_enters_probe_down_when_throughput_flat() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_up_gain: 1.25,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.limit = 125;
            state.throughput_ema = Some(100.0);
            state.phase = Phase::ProbeBW(ProbeBWState::ProbeUp {
                pre_probe_limit: 100,
                start_throughput_ema: 100.0,
            });
            alg.gauge.store(125, Ordering::Release);
        }

        // Feed samples, then force with flat throughput (100 → EMA stays ~100, below 110)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(100.0);

        // Should enter ProbeDown at ceil(100 * 0.75) = 75
        let limit = alg.current();
        assert_eq!(limit, 75);
        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::ProbeDown { .. })
        ));
    }

    // ======================== ProbeDown tests ========================

    #[test]
    fn probe_down_keeps_lower_limit_when_throughput_maintained() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.limit = 75;
            state.throughput_ema = Some(100.0);
            state.phase = Phase::ProbeBW(ProbeBWState::ProbeDown {
                pre_down_limit: 100,
                pre_probe_throughput_ema: 100.0,
            });
            alg.gauge.store(75, Ordering::Release);
        }

        // Throughput at 75% concurrency is ~same as before (95 → EMA = 0.3*95 + 0.7*100 = 98.5, >= 90)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(95.0);

        // Should keep the lower limit (75) since throughput was maintained
        assert_eq!(alg.current(), 75);
        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::Cruise { .. })
        ));
    }

    #[test]
    fn probe_down_restores_limit_when_throughput_drops() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.limit = 75;
            state.throughput_ema = Some(100.0);
            state.phase = Phase::ProbeBW(ProbeBWState::ProbeDown {
                pre_down_limit: 100,
                pre_probe_throughput_ema: 100.0,
            });
            alg.gauge.store(75, Ordering::Release);
        }

        // Throughput dropped significantly (50 → EMA = 0.3*50 + 0.7*100 = 85, < 90)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(50.0);

        // Should restore original limit (100) since throughput dropped
        assert_eq!(alg.current(), 100);
        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::Cruise { .. })
        ));
    }

    // ======================== Error brake tests ========================

    #[test]
    fn error_brake_fires_per_sample() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            error_rate_threshold: 0.05,
            error_backoff_ratio: 0.5,
            // Use a long interval so the brake fires before interval processing
            probe_interval_ms: 60_000,
            ..default_config()
        };
        let alg = adaptive(config);
        assert_eq!(alg.current(), 100);

        // Window is 200 (limit=100 * 2, clamped to min 100).
        // 190 successes + 11 errors → 11/200 = 5.5% > 5%.
        for _ in 0..190 {
            alg.update(0, 0, Outcome::Success, Duration::from_millis(10));
        }
        assert_eq!(alg.current(), 100);

        for _ in 0..10 {
            alg.update(0, 0, Outcome::Dropped, Duration::from_millis(10));
        }
        assert_eq!(alg.current(), 100); // 10/200 = 5.0%, not > 5%

        alg.update(0, 0, Outcome::Dropped, Duration::from_millis(10));
        // 11/200 = 5.5% > 5%, brake fires: ceil(100 * 0.5) = 50
        assert_eq!(alg.current(), 50);
    }

    #[test]
    fn error_brake_resets_to_cruise() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            error_rate_threshold: 0.05,
            error_backoff_ratio: 0.5,
            probe_interval_ms: 60_000,
            ..default_config()
        };
        let alg = adaptive(config);

        // Trigger error brake (window=200, need >5% error rate)
        for _ in 0..190 {
            alg.update(0, 0, Outcome::Success, Duration::from_millis(10));
        }
        for _ in 0..11 {
            alg.update(0, 0, Outcome::Dropped, Duration::from_millis(10));
        }

        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::Cruise { .. })
        ));
    }

    // ======================== Latency brake tests ========================

    #[test]
    fn latency_brake_fires_at_interval() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            queue_signal_beta: 6.0,
            latency_backoff_ratio: 0.9,
            probe_bw_intervals: 100, // high so we stay in cruise
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
            state.baseline_p95 = Some(0.010); // 10ms baseline
            state.throughput_ema = Some(100.0);
        }

        // Feed samples with much higher latency (100ms) to trigger latency brake
        // queue_signal = limit * (1 - baseline/current) = 110 * (1 - 0.01/0.1) = 110 * 0.9 = 99 > 6
        feed_successes(&alg, 50, Duration::from_millis(100));
        alg.force_interval_with_throughput(1000.0);

        // After cruise additive increase: 100 + sqrt(100) = 110
        // After latency brake: ceil(110 * 0.9) = 99
        let limit = alg.current();
        assert!(
            limit < 110,
            "Latency brake should have reduced limit, got {limit}"
        );
    }

    // ======================== Edge case tests ========================

    #[test]
    fn zero_sample_interval_is_skipped() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);
        assert_eq!(alg.current(), 10);

        // Force interval with no samples
        alg.force_interval();

        // Should remain unchanged
        assert_eq!(alg.current(), 10);
    }

    #[test]
    fn min_max_bounds_respected() {
        let config = AdaptiveConfig {
            initial_limit: 5,
            min_limit: 5,
            max_limit: 15,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
            state.limit = 14;
            alg.gauge.store(14, Ordering::Release);
        }

        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // sqrt(14) ≈ 3, so 14 + 3 = 17, but max is 15
        assert!(
            alg.current() <= 15,
            "Should not exceed max_limit, got {}",
            alg.current()
        );
    }

    #[test]
    fn min_limit_respected_on_error_brake() {
        let config = AdaptiveConfig {
            initial_limit: 5,
            min_limit: 3,
            max_limit: 10000,
            error_rate_threshold: 0.05,
            error_backoff_ratio: 0.5,
            probe_interval_ms: 60_000,
            ..default_config()
        };
        let alg = adaptive(config);

        // Trigger error brake: ceil(5 * 0.5) = 3 (= min_limit)
        for _ in 0..94 {
            alg.update(0, 0, Outcome::Success, Duration::from_millis(10));
        }
        for _ in 0..6 {
            alg.update(0, 0, Outcome::Dropped, Duration::from_millis(10));
        }
        assert_eq!(alg.current(), 3);
    }

    // ======================== Inflight guard tests ========================

    #[test]
    fn no_growth_when_underutilized() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 100,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
        }

        // Feed with zero inflight — should decay, not grow
        feed_successes_idle(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // ceil(100 * 0.95) = 95
        assert_eq!(alg.current(), 95, "Limit should decay when underutilized");
    }

    #[test]
    fn growth_resumes_when_utilized() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 100,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
        }

        // Feed with high inflight — should grow
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        assert_eq!(
            alg.current(),
            110,
            "Limit should grow when utilized (100 + sqrt(100))"
        );
    }

    #[test]
    fn cruise_stops_growing_when_throughput_flat() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 100,
            ..default_config()
        };
        let alg = adaptive(config);

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
            state.throughput_ema = Some(1000.0);
        }

        // Flat throughput: 0.3*1000 + 0.7*1000 = 1000, need > 1100 to grow
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // 1000 < 1000*1.10=1100, so not growing — should NOT increase
        assert_eq!(alg.current(), 100);

        // Now with strong throughput growth
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(2000.0); // 0.3*2000 + 0.7*1000 = 1300, vs prev 1000 → 1300 >= 1100

        // 1300 >= 1000*1.10=1100, so growing — should increase
        // 100 + sqrt(100) = 110
        assert_eq!(alg.current(), 110);
    }

    // ======================== Serialization tests ========================

    #[test]
    fn config_serialization_json() {
        let config = AdaptiveConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AdaptiveConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.initial_limit, config.initial_limit);
        assert_eq!(deserialized.min_limit, config.min_limit);
        assert_eq!(deserialized.max_limit, config.max_limit);
        assert!((deserialized.probe_up_gain - config.probe_up_gain).abs() < f64::EPSILON);
    }

    #[test]
    fn config_serialization_toml() {
        let toml_str = r#"
            initial-limit = 20
            min-limit = 2
            max-limit = 500
            probe-interval-ms = 2000
        "#;
        let config: AdaptiveConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.initial_limit, 20);
        assert_eq!(config.min_limit, 2);
        assert_eq!(config.max_limit, 500);
        assert_eq!(config.probe_interval_ms, 2000);
        // Defaults should fill in
        assert!((config.probe_up_gain - 1.25).abs() < f64::EPSILON);
    }

    // ======================== Integration-style tests ========================

    #[test]
    fn full_lifecycle_cruise_to_probe_up() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_bw_intervals: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Cruise: additive increase for 3 intervals, then probe up
        let limit_before = alg.current();
        for _ in 0..3 {
            feed_successes(&alg, 50, Duration::from_millis(10));
            alg.force_interval_with_throughput(1000.0);
        }

        let limit_after = alg.current();
        assert!(
            limit_after > limit_before,
            "Limit should have grown through cruise + probe: before={limit_before}, after={limit_after}"
        );
    }

    #[test]
    fn ignore_does_not_count_as_success_or_error() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 60_000,
            ..default_config()
        };
        let alg = adaptive(config);

        // Feed only Ignore outcomes
        for _ in 0..100 {
            alg.update(0, 0, Outcome::Ignore, Duration::from_millis(10));
        }

        // No successes or errors recorded, limit should be unchanged
        assert_eq!(alg.current(), 100);
    }
}

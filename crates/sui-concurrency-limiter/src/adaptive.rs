// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adaptive concurrency limiter combining BBR-style throughput probing with tail-latency braking.
//!
//! Unlike the per-sample algorithms (AIMD, Gradient, Vegas, BBR), Adaptive collects statistics
//! over **probe intervals** (~1s) and makes limit decisions at interval boundaries. A three-phase
//! state machine drives the limit:
//!
//! 1. **Startup**: Exponential search (double each round) until throughput stalls for 3 rounds.
//! 2. **ProbeBW**: Steady-state with periodic probing (Cruise → ProbeUp → ProbeDown cycle).
//! 3. **Emergency brake**: Per-sample error check + interval-based tail-latency check (always active).
//!
//! The key insight: measure completions-per-second as an independent throughput signal, then ask
//! whether it grows when concurrency increases. If yes, grow. If no, you've found the knee.

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

    // Startup
    /// Multiplicative growth factor during startup (limit doubles by default).
    pub startup_growth_factor: f64,
    /// Minimum throughput growth ratio to consider the pipe not full.
    pub full_pipe_threshold: f64,
    /// Number of consecutive stall rounds before exiting startup.
    pub full_pipe_rounds: usize,

    // ProbeBW
    /// Multiplicative gain when probing upward.
    pub probe_up_gain: f64,
    /// Number of cruise intervals between probe cycles.
    pub probe_bw_intervals: usize,

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
    /// Headroom factor applied when transitioning out of startup.
    pub headroom_factor: f64,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 100_000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 0.25,
            full_pipe_rounds: 3,
            probe_up_gain: 1.25,
            probe_bw_intervals: 10,
            error_backoff_ratio: 0.5,
            latency_backoff_ratio: 0.9,
            error_rate_threshold: 0.05,
            queue_signal_beta: 6.0,
            brake_percentile: 0.95,
            headroom_factor: 0.85,
        }
    }
}

// ---------------------------------------------------------------------------
// State machine phases
// ---------------------------------------------------------------------------

enum Phase {
    Startup {
        best_throughput: f64,
        stall_count: usize,
    },
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
        // O(n) partial sort — only positions the idx-th element correctly.
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
    /// Skip the latency brake for one interval after ProbeDown recalibrates baseline.
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
            Some(prev) => 0.3 * throughput + 0.7 * prev,
        });
        let throughput_ema = self.throughput_ema.unwrap();

        // Compute tail latency
        let current_p95 = self.stats.percentile(config.brake_percentile);

        // Inflight guard: skip growth when the system isn't utilizing the current limit.
        // Without this, the limit rockets to max when ingestion is slow.
        let underutilized = self.stats.peak_inflight * 2 < self.limit;

        // Throughput guard: skip growth when throughput isn't responding to higher limits.
        // Without this, Cruise blindly grows the limit while the backend saturates
        // (latency climbs but no errors), overshooting the knee.
        let throughput_declining = match prev_ema {
            Some(prev) => throughput_ema < prev * 0.95,
            None => false,
        };

        // Phase-specific logic
        match &mut self.phase {
            Phase::Startup {
                best_throughput,
                stall_count,
            } => {
                if underutilized {
                    // Not enough inflight to test the backend — skip this round
                } else {
                    let growth = if *best_throughput > 0.0 {
                        (throughput_ema - *best_throughput) / *best_throughput
                    } else {
                        1.0 // first interval always counts as growth
                    };

                    if growth >= config.full_pipe_threshold {
                        *best_throughput = throughput_ema;
                        *stall_count = 0;
                        self.limit =
                            ((self.limit as f64) * config.startup_growth_factor).ceil() as usize;
                    } else {
                        *stall_count += 1;
                        if *stall_count >= config.full_pipe_rounds {
                            let drain_limit = ((self.limit as f64) * config.headroom_factor
                                / config.startup_growth_factor)
                                .ceil() as usize;
                            self.limit = drain_limit;
                            self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                                intervals_since_probe: 0,
                            });
                        }
                    }
                }
            }
            Phase::ProbeBW(sub) => {
                match sub {
                    ProbeBWState::Cruise {
                        intervals_since_probe,
                    } => {
                        if underutilized {
                            // Limit is above what the system actually uses — decay toward
                            // real usage so the limit stays meaningful as a ceiling.
                            self.limit = ((self.limit as f64) * 0.95).ceil() as usize;
                        } else if !throughput_declining {
                            let inc = ((self.limit as f64).sqrt().floor() as usize).max(1);
                            self.limit += inc;
                        }
                        *intervals_since_probe += 1;

                        if !underutilized
                            && !throughput_declining
                            && *intervals_since_probe >= config.probe_bw_intervals
                        {
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
                        // After 1 interval: did throughput respond?
                        if throughput_ema >= *start_throughput_ema * 1.10 {
                            // Keep the higher limit
                            self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                                intervals_since_probe: 0,
                            });
                        } else {
                            // Revert and probe down to measure baseline latency
                            let reverted = *pre_probe_limit;
                            let baseline_stale = self
                                .baseline_recorded_at
                                .is_none_or(|t| t.elapsed() > Duration::from_secs(30));
                            if baseline_stale {
                                let pre_down = reverted;
                                self.limit = ((reverted as f64) * 0.75).ceil() as usize;
                                self.phase = Phase::ProbeBW(ProbeBWState::ProbeDown {
                                    pre_down_limit: pre_down,
                                });
                            } else {
                                self.limit = reverted;
                                self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                                    intervals_since_probe: 0,
                                });
                            }
                        }
                    }
                    ProbeBWState::ProbeDown { pre_down_limit } => {
                        if let Some(p95) = current_p95 {
                            self.baseline_p95 = Some(p95);
                            self.baseline_recorded_at = Some(Instant::now());
                            self.suppress_latency_brake = true;
                        }
                        self.limit = *pre_down_limit;
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
                phase: Phase::Startup {
                    best_throughput: 0.0,
                    stall_count: 0,
                },
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
    fn update(&self, inflight: usize, _delivered: usize, outcome: Outcome, rtt: Duration) -> usize {
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

    // ======================== Startup tests ========================

    #[test]
    fn startup_doubles_limit_when_throughput_grows() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);
        assert_eq!(alg.current(), 10);

        // Feed successes, then force with deterministic throughput
        feed_successes(&alg, 100, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // First interval with throughput > 0 and best_throughput = 0 → always grows
        assert_eq!(alg.current(), 20); // 10 * 2.0 = 20
    }

    #[test]
    fn startup_detects_full_pipe_after_stalls() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            full_pipe_rounds: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Round 1: first throughput → doubles (growth from 0 always counts)
        feed_successes(&alg, 100, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(alg.current(), 20);

        // Rounds 2-4: same throughput (stalls since EMA barely changes)
        for _ in 0..3 {
            feed_successes(&alg, 100, Duration::from_millis(10));
            alg.force_interval_with_throughput(1000.0);
        }

        let state = alg.inner.lock().unwrap();
        assert!(
            matches!(state.phase, Phase::ProbeBW(_)),
            "Should have transitioned to ProbeBW"
        );
    }

    #[test]
    fn startup_drain_formula() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            full_pipe_rounds: 1, // exit after 1 stall
            headroom_factor: 0.85,
            startup_growth_factor: 2.0,
            ..default_config()
        };
        let alg = adaptive(config);

        // First interval: grows to 20 (first throughput always counts as growth)
        feed_successes(&alg, 100, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(alg.current(), 20);

        // Second interval: same throughput → stall → drain
        // At limit=20, drain = ceil(20 * 0.85 / 2.0) = ceil(8.5) = 9
        feed_successes(&alg, 100, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(alg.current(), 9);
    }

    // ======================== ProbeBW Cruise tests ========================

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
    fn probe_up_reverts_when_throughput_flat() {
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
            state.baseline_p95 = Some(0.01); // Existing fresh baseline so we don't probe down
            state.baseline_recorded_at = Some(Instant::now());
            state.phase = Phase::ProbeBW(ProbeBWState::ProbeUp {
                pre_probe_limit: 100,
                start_throughput_ema: 100.0,
            });
            alg.gauge.store(125, Ordering::Release);
        }

        // Feed samples, then force with flat throughput (100 → EMA stays ~100, below 110)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(100.0);

        // Should revert to pre_probe_limit (100) and go to Cruise
        let limit = alg.current();
        assert_eq!(limit, 100);
    }

    // ======================== ProbeDown tests ========================

    #[test]
    fn probe_down_records_baseline_and_restores() {
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
            });
            alg.gauge.store(75, Ordering::Release);
        }

        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        assert_eq!(alg.current(), 100);
        let state = alg.inner.lock().unwrap();
        assert!(state.baseline_p95.is_some());
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
        assert_eq!(
            alg.current(),
            95,
            "Limit should decay when underutilized"
        );
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
    fn cruise_stops_growing_when_throughput_declines() {
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

        // Feed with high inflight but declining throughput (below 95% of prev EMA)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(900.0); // 0.3*900 + 0.7*1000 = 970, vs prev 1000 → 970 < 950? No, 970 >= 950

        // 970 >= 1000*0.95=950, so NOT declining — should still grow
        assert_eq!(alg.current(), 110);

        // Now a real decline
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(500.0); // 0.3*500 + 0.7*970 = 150+679 = 829, vs prev 970 → 829 < 921.5

        // 829 < 970*0.95=921.5, so declining — should NOT grow
        // limit stays at 110 (no sqrt(110) added)
        assert_eq!(alg.current(), 110);
    }

    #[test]
    fn startup_skips_when_underutilized() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            ..default_config()
        };
        let alg = adaptive(config);

        // Feed with zero inflight — startup should not double
        feed_successes_idle(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        assert_eq!(
            alg.current(),
            10,
            "Startup should not grow when underutilized"
        );
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
        assert!((config.startup_growth_factor - 2.0).abs() < f64::EPSILON);
    }

    // ======================== Integration-style tests ========================

    #[test]
    fn full_lifecycle_startup_to_probe_bw() {
        let config = AdaptiveConfig {
            initial_limit: 10,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            full_pipe_rounds: 2,
            probe_bw_intervals: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Startup: first interval grows (throughput from 0 always counts)
        feed_successes(&alg, 100, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(alg.current(), 20);

        // Startup: 2 stalls with flat throughput → exit
        for _ in 0..2 {
            feed_successes(&alg, 100, Duration::from_millis(10));
            alg.force_interval_with_throughput(1000.0);
        }

        let state = alg.inner.lock().unwrap();
        assert!(matches!(state.phase, Phase::ProbeBW(_)));
        drop(state);

        // ProbeBW Cruise: additive increase for 3 intervals, then probe up
        let limit_before_probes = alg.current();
        for _ in 0..3 {
            feed_successes(&alg, 50, Duration::from_millis(10));
            alg.force_interval_with_throughput(1000.0);
        }

        let limit_after = alg.current();
        assert!(
            limit_after > limit_before_probes,
            "Limit should have grown through cruise + probe: before={limit_before_probes}, after={limit_after}"
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

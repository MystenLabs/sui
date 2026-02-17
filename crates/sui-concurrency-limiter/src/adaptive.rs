// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adaptive concurrency limiter using throughput probing.
//!
//! Unlike the per-sample algorithms (AIMD, Gradient, Vegas, BBR), Adaptive collects statistics
//! over **probe intervals** (~1s) and makes limit decisions at interval boundaries:
//!
//! 1. **Startup**: Exponential search (double each round) until throughput stalls for 3 rounds,
//!    then drain to ~85% of the knee and enter ProbeBW.
//! 2. **ProbeBW**: Steady-state with periodic probing (Cruise → ProbeUp → ProbeDown cycle).
//!    Cruise holds the limit steady (decaying when underutilized); ProbeUp tests for headroom.
//! 3. **Emergency brake**: Per-sample error rate check (always active).
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

    // Startup phase
    /// Multiplicative growth factor in Startup (doubles the limit each round by default).
    pub startup_growth_factor: f64,
    /// Minimum throughput EMA growth ratio to count as "pipe filling" during Startup.
    /// E.g., 1.25 means 25% growth is needed; below that is a stall.
    pub full_pipe_threshold: f64,
    /// Consecutive stall rounds in Startup before declaring the pipe full and draining.
    pub full_pipe_rounds: usize,
    /// Headroom factor applied when draining from Startup into ProbeBW.
    /// Drain formula: limit * headroom_factor / startup_growth_factor.
    pub headroom_factor: f64,

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
    /// Error rate threshold (fraction) that triggers the error brake.
    pub error_rate_threshold: f64,
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
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            headroom_factor: 0.85,
            probe_up_gain: 1.25,
            probe_bw_intervals: 10,
            probe_down_min_throughput: 0.90,
            error_backoff_ratio: 0.5,
            error_rate_threshold: 0.05,
        }
    }
}

// ---------------------------------------------------------------------------
// State machine phases
// ---------------------------------------------------------------------------

enum Phase {
    Startup {
        /// Throughput EMA at the start of the current probe round.
        round_start_throughput: Option<f64>,
        /// Consecutive rounds where throughput grew less than full_pipe_threshold.
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
        pre_probe_throughput_ema: f64,
    },
}

// ---------------------------------------------------------------------------
// Per-interval statistics
// ---------------------------------------------------------------------------

struct IntervalStats {
    successes: usize,
    errors: usize,
    peak_inflight: usize,
    start: Instant,
    next_interval: Duration,
}

impl IntervalStats {
    fn new(interval: Duration) -> Self {
        Self {
            successes: 0,
            errors: 0,
            peak_inflight: 0,
            start: Instant::now(),
            next_interval: interval,
        }
    }

    fn reset(&mut self, interval: Duration) {
        self.successes = 0;
        self.errors = 0;
        self.peak_inflight = 0;
        self.start = Instant::now();
        // Jitter: +/- 10%
        let jitter = rand::thread_rng().gen_range(0.9..1.1);
        self.next_interval = Duration::from_secs_f64(interval.as_secs_f64() * jitter);
    }

    fn elapsed(&self) -> bool {
        self.start.elapsed() >= self.next_interval
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
    limit: usize,
}

impl AdaptiveState {
    fn record_sample(&mut self, inflight: usize, outcome: &Outcome) {
        self.stats.peak_inflight = self.stats.peak_inflight.max(inflight);
        self.rolling_errors.resize_for_limit(self.limit);
        match outcome {
            Outcome::Success => {
                self.stats.successes += 1;
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

        // Inflight guard: skip growth when the system isn't utilizing the current limit.
        // Without this, the limit rockets to max when ingestion is slow.
        let underutilized = self.stats.peak_inflight * 2 < self.limit;

        // Phase-specific logic
        match &mut self.phase {
            Phase::Startup {
                round_start_throughput,
                stall_count,
            } => {
                if !underutilized {
                    // Use EMA (not raw) for growth detection: 2x jumps give huge margin
                    // over the 25% threshold, so EMA lag costs at most one extra doubling.
                    // Raw throughput noise is the bigger risk — it can cause false growth
                    // that skips stall counts.
                    let grew = match *round_start_throughput {
                        Some(prev) if prev > 0.0 => {
                            throughput_ema >= prev * config.full_pipe_threshold
                        }
                        _ => true,
                    };

                    *round_start_throughput = Some(throughput_ema);

                    if grew {
                        *stall_count = 0;
                        self.limit =
                            ((self.limit as f64) * config.startup_growth_factor).ceil() as usize;
                    } else {
                        *stall_count += 1;
                    }
                }
                // When underutilized, skip — don't grow or count stalls until
                // the pipeline catches up to the current limit.

                if *stall_count >= config.full_pipe_rounds {
                    // Full pipe detected. The last successful growth step overshot,
                    // so drain back: limit * headroom / growth_factor.
                    self.limit = ((self.limit as f64) * config.headroom_factor
                        / config.startup_growth_factor)
                        .ceil() as usize;
                    self.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                        intervals_since_probe: 0,
                    });
                }
            }
            Phase::ProbeBW(sub) => {
                match sub {
                    ProbeBWState::Cruise {
                        intervals_since_probe,
                    } => {
                        if underutilized && *intervals_since_probe > 3 {
                            // Limit is above what the system actually uses — decay toward
                            // real usage so the limit stays meaningful as a ceiling.
                            // Grace period (3 intervals) prevents decay right after
                            // entering Cruise (e.g., post-Startup drain).
                            self.limit = ((self.limit as f64) * 0.95).ceil() as usize;
                        }
                        // No additive increase — ProbeUp handles growth discovery.
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
                        if throughput >= *start_throughput_ema * config.full_pipe_threshold {
                            // Major headroom discovered — re-enter Startup for fast
                            // exploration (e.g., backend autoscaled).
                            self.phase = Phase::Startup {
                                round_start_throughput: Some(throughput_ema),
                                stall_count: 0,
                            };
                        } else if throughput
                            >= *start_throughput_ema * config.throughput_growth_threshold
                        {
                            // Modest throughput gain — keep the higher limit
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
/// Combines BBR-style throughput probing with error braking using a two-phase
/// state machine (Startup → ProbeBW). See module docs for details.
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
                    round_start_throughput: None,
                    stall_count: 0,
                },
                stats: IntervalStats::new(interval),
                rolling_errors: RollingErrors::new(100),
                throughput_ema: None,
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
        _rtt: Duration,
    ) -> usize {
        let mut state = self.inner.lock().unwrap();

        // 1. Record sample
        state.record_sample(inflight, &outcome);

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
    fn cruise_holds_steady() {
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

        // Cruise should not grow additively — ProbeUp handles growth discovery
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(alg.current(), 100);
    }

    // ======================== Startup tests ========================

    #[test]
    fn startup_doubles_on_growth() {
        let config = AdaptiveConfig {
            initial_limit: 4,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // First round: no baseline, assumed growth → double to 8
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(400.0);
        assert_eq!(alg.current(), 8);

        // Second round: throughput doubled (800 >= 400*1.25) → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);
    }

    #[test]
    fn startup_no_double_on_stall() {
        let config = AdaptiveConfig {
            initial_limit: 8,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Seed → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Throughput flat (800 < 800*1.25=1000) → stall, hold at 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);
    }

    #[test]
    fn startup_detects_full_pipe_and_drains() {
        let config = AdaptiveConfig {
            initial_limit: 4,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            headroom_factor: 0.85,
            ..default_config()
        };
        let alg = adaptive(config);

        // Round 1: seed → double to 8
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(400.0);
        assert_eq!(alg.current(), 8);

        // Round 2: throughput doubles → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Round 3: throughput flat → stall 1
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Round 4: still flat → stall 2
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Round 5: still flat → stall 3 → drain!
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        // drain: ceil(16 * 0.85 / 2.0) = ceil(6.8) = 7
        assert_eq!(alg.current(), 7);

        let state = alg.inner.lock().unwrap();
        assert!(matches!(
            state.phase,
            Phase::ProbeBW(ProbeBWState::Cruise { .. })
        ));
    }

    #[test]
    fn startup_resets_stall_count_on_growth() {
        let config = AdaptiveConfig {
            initial_limit: 4,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Seed → double to 8
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(400.0);
        // Grows → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);

        // Stall 1 (flat at 800)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        // Stall 2
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);

        // Throughput jumps to 1300. EMA from ~663 → 0.3*1300 + 0.7*663 = 854,
        // which exceeds 663*1.25=829. Grew → reset stall count, double to 32.
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1300.0);
        assert_eq!(alg.current(), 32); // doubled from 16

        // Two more stalls — not enough to trigger drain (need 3 consecutive)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1300.0);
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1300.0);

        let state = alg.inner.lock().unwrap();
        assert!(matches!(state.phase, Phase::Startup { .. }));
    }

    #[test]
    fn startup_ema_dampens_noisy_throughput() {
        // EMA prevents a single noisy spike from counting as growth.
        // With raw throughput, a 25%+ spike would falsely reset stall_count.
        let config = AdaptiveConfig {
            initial_limit: 8,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            throughput_ema_alpha: 0.3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Seed → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Flat throughput → stall 1. EMA ≈ 800. round_start = 800.
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Noisy spike at 1100 (raw is 37% above baseline, but
        // EMA = 0.3*1100 + 0.7*800 = 890, which is 890/800 = 1.11 < 1.25).
        // Stall count should continue incrementing, not reset.
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1100.0);
        assert_eq!(
            alg.current(),
            16,
            "EMA should dampen the spike — still a stall"
        );

        // One more stall → stall_count=3 → drain
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        let limit = alg.current();
        assert!(
            limit < 16,
            "Should have drained after 3 stalls, got {limit}"
        );
    }

    #[test]
    fn startup_skips_when_underutilized() {
        let config = AdaptiveConfig {
            initial_limit: 8,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            full_pipe_threshold: 1.25,
            full_pipe_rounds: 3,
            ..default_config()
        };
        let alg = adaptive(config);

        // Seed with high inflight → double to 16
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(800.0);
        assert_eq!(alg.current(), 16);

        // Now feed with zero inflight (underutilized). Should neither grow nor count as stall.
        feed_successes_idle(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(900.0);
        assert_eq!(alg.current(), 16, "Should not double when underutilized");

        // Still in Startup, stall_count should still be 0
        let state = alg.inner.lock().unwrap();
        assert!(matches!(state.phase, Phase::Startup { stall_count: 0, .. }));
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

        // Feed samples with modest throughput gain (115 >= 100*1.10 but 115 < 100*1.25)
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(115.0);

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
    fn probe_up_reenters_startup_on_major_gain() {
        let config = AdaptiveConfig {
            initial_limit: 100,
            min_limit: 1,
            max_limit: 10000,
            probe_interval_ms: 1000,
            probe_up_gain: 1.25,
            full_pipe_threshold: 1.25,
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

        // Major throughput gain: 130 >= 100*1.25=125 → re-enter Startup
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(130.0);

        let state = alg.inner.lock().unwrap();
        assert!(
            matches!(state.phase, Phase::Startup { .. }),
            "Should re-enter Startup on major throughput gain"
        );
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
            initial_limit: 10,
            min_limit: 5,
            max_limit: 15,
            probe_interval_ms: 1000,
            startup_growth_factor: 2.0,
            ..default_config()
        };
        let alg = adaptive(config);

        // Startup doubling: 10 * 2 = 20, but max is 15
        feed_successes(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

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
                intervals_since_probe: 4, // past grace period
            });
        }

        // Feed with zero inflight — should decay, not grow
        feed_successes_idle(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);

        // ceil(100 * 0.95) = 95
        assert_eq!(alg.current(), 95, "Limit should decay when underutilized");
    }

    #[test]
    fn cruise_underutilized_grace_period() {
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
                intervals_since_probe: 0, // fresh entry (e.g., post-Startup drain)
            });
        }

        // Feed with zero inflight during grace period — should NOT decay
        feed_successes_idle(&alg, 50, Duration::from_millis(10));
        alg.force_interval_with_throughput(1000.0);
        assert_eq!(
            alg.current(),
            100,
            "Should not decay during grace period (intervals_since_probe <= 3)"
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

        {
            let mut state = alg.inner.lock().unwrap();
            state.phase = Phase::ProbeBW(ProbeBWState::Cruise {
                intervals_since_probe: 0,
            });
        }

        // Cruise for 3 intervals, then transitions to ProbeUp
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

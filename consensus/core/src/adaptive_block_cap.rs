// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use consensus_config::{ConsensusProtocolConfig, Parameters};

use crate::metrics::NodeMetrics;

/// Weight of the newest sample in the round-period EWMA. Low enough that a single slow round
/// does not shrink the budget, high enough to react within a few rounds of a sustained change.
const EWMA_ALPHA: f64 = 0.2;

/// Fraction of the target below which the budget may grow again. The gap between this and the
/// target is a deadband that stops the controller oscillating around it.
const RECOVERY_THRESHOLD: f64 = 0.8;

/// Floor on the per-observation shrink factor: the budget retains at least this fraction per
/// observed round advance. Together with `RECOVERY_STEP_FRACTION` this sets how asymmetric the
/// controller is, and therefore how cheaply intermittent load can hold the budget down — the
/// budget must not fall so much faster than it recovers that a low duty cycle pins it.
const MAX_DECREASE_FACTOR: f64 = 0.7;

/// Additive growth per observation while recovering, as a fraction of the ceiling. Proportional
/// rather than absolute so that recovery takes a similar number of rounds whatever block limit
/// the protocol configures.
const RECOVERY_STEP_FRACTION: u64 = 32;

/// Largest sample admitted into the average, as a multiple of the target. A stalled or
/// partitioned node reports a round period unrelated to network health; without this it would
/// dominate the average for tens of observations afterwards, holding the budget at its floor long
/// after the network recovered. The sample still registers unambiguously as over target.
const MAX_SAMPLE_MULTIPLE: f64 = 100.0;

/// Round period the controller aims to stay under, as a multiple of `min_round_delay`.
const TARGET_ROUND_DELAY_MULTIPLE: u32 = 3;

/// Lower bound on the budget. Not tied to transaction size: a block always admits one
/// tx or soft bundle regardless (see `TransactionConsumer::next`), so block size is
/// `max(budget, bundle)`.
const FLOOR_BYTES: u64 = 32 * 1024;

/// Author-local damper on proposed block bytes.
///
/// The consensus round period grows roughly linearly with block size. This
/// controller shrinks *this* authority's own proposals when its locally
/// observed round period exceeds a target, relaxing back toward the protocol block limit as
/// rounds recover.
///
/// Control law is AIMD: multiplicative decrease proportional to overshoot (bounded per
/// observation by `MAX_DECREASE_FACTOR`), additive increase once the period is comfortably under
/// target.
pub(crate) struct AdaptiveBlockCap {
    /// Round period the controller aims to stay under, in microseconds.
    target_us: f64,
    /// Lower bound on `budget_bytes`.
    floor_bytes: u64,
    /// Upper bound on `budget_bytes`; the protocol block limit.
    ceiling_bytes: u64,
    /// Additive growth applied per observation while recovering.
    recovery_step_bytes: u64,
    /// Smoothed round period, truncated to microseconds.
    ewma_us: AtomicU64,
    /// Current self-imposed limit on proposed block bytes.
    budget_bytes: AtomicU64,
}

impl AdaptiveBlockCap {
    pub(crate) fn new(
        parameters: &Parameters,
        protocol_config: &ConsensusProtocolConfig,
    ) -> Option<Self> {
        if !parameters.adaptive_block_cap_enabled {
            return None;
        }

        // Always at or above the pacing floor, so the target cannot be unachievable.
        let target = parameters.min_round_delay * TARGET_ROUND_DELAY_MULTIPLE;

        let ceiling_bytes = protocol_config.max_transactions_in_block_bytes();
        // Clamped in case a protocol version configures a block limit below the floor.
        let floor_bytes = FLOOR_BYTES.min(ceiling_bytes);

        Some(Self {
            target_us: target.as_micros() as f64,
            floor_bytes,
            ceiling_bytes,
            recovery_step_bytes: (ceiling_bytes / RECOVERY_STEP_FRACTION).max(1),
            ewma_us: AtomicU64::new(0),
            budget_bytes: AtomicU64::new(ceiling_bytes),
        })
    }

    /// Feeds one locally observed round period into the controller. Called on each threshold
    /// clock advance, so samples arrive more slowly exactly as the network degrades. A
    /// collapsing network may report only every few seconds. The budget therefore halves per
    /// observation on the way down but grows by a small step on the way back, reacting within a
    /// few samples while giving ground back gradually.
    pub(crate) fn observe_round(&self, round_period: Duration, metrics: &NodeMetrics) {
        let sample_us =
            (round_period.as_micros() as f64).min(self.target_us * MAX_SAMPLE_MULTIPLE);

        let prev_us = self.ewma_us.load(Ordering::Relaxed) as f64;
        let ewma_us = if prev_us == 0.0 {
            sample_us
        } else {
            prev_us * (1.0 - EWMA_ALPHA) + sample_us * EWMA_ALPHA
        };
        // Saturates rather than wrapping if the average is absurdly large.
        self.ewma_us.store(ewma_us as u64, Ordering::Relaxed);

        let budget = self.budget_bytes.load(Ordering::Relaxed);
        let new_budget = if ewma_us > self.target_us {
            // Shrink in proportion to the overshoot. ewma_us is nonzero here because it
            // exceeds the target, and the float-to-int cast saturates rather than wrapping.
            let scale = (self.target_us / ewma_us).max(MAX_DECREASE_FACTOR);
            ((budget as f64 * scale) as u64).max(self.floor_bytes)
        } else if ewma_us < self.target_us * RECOVERY_THRESHOLD {
            budget
                .saturating_add(self.recovery_step_bytes)
                .min(self.ceiling_bytes)
        } else {
            budget
        };

        if new_budget != budget {
            self.budget_bytes.store(new_budget, Ordering::Relaxed);
        }
        metrics
            .adaptive_block_cap_bytes
            .set(i64::try_from(new_budget).unwrap_or(i64::MAX));
    }

    pub(crate) fn budget_bytes(&self) -> u64 {
        self.budget_bytes.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn set_budget_bytes_for_testing(&self, val: u64) {
        self.budget_bytes.store(val, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::test_metrics;
    use std::time::Duration;

    fn params(enabled: bool) -> Parameters {
        Parameters {
            min_round_delay: Duration::from_millis(50),
            adaptive_block_cap_enabled: enabled,
            ..Default::default()
        }
    }

    fn protocol_config() -> ConsensusProtocolConfig {
        let mut cfg = ConsensusProtocolConfig::for_testing();
        cfg.set_max_transactions_in_block_bytes_for_testing(512 * 1024);
        cfg.set_max_transaction_size_bytes_for_testing(128 * 1024);
        cfg
    }

    fn cap() -> AdaptiveBlockCap {
        AdaptiveBlockCap::new(&params(true), &protocol_config()).unwrap()
    }

    /// The target tracks the pacing floor, so it can never be below what the node can achieve.
    #[test]
    fn target_is_a_multiple_of_min_round_delay() {
        // Test params use a 50ms min_round_delay.
        assert_eq!(cap().target_us, 150_000.0);

        let mut p = params(true);
        p.min_round_delay = Duration::from_millis(400);
        let cap = AdaptiveBlockCap::new(&p, &protocol_config()).unwrap();
        assert_eq!(cap.target_us, 1_200_000.0);
    }

    #[test]
    fn enabled_by_default_and_disablable() {
        assert!(AdaptiveBlockCap::new(&Parameters::default(), &protocol_config()).is_some());
        assert!(AdaptiveBlockCap::new(&params(false), &protocol_config()).is_none());
    }



    #[test]
    fn starts_at_ceiling_and_shrinks_under_overshoot() {
        let metrics = test_metrics();
        let cap = cap();
        assert_eq!(cap.budget_bytes(), 512 * 1024);
        for _ in 0..20 {
            cap.observe_round(Duration::from_millis(3000), &metrics.node_metrics);
        }
        assert_eq!(cap.budget_bytes(), FLOOR_BYTES);
    }

    /// The floor bounds wind-down only; it is allowed to sit below a single transaction because
    /// `TransactionConsumer` always admits one batch.
    #[test]
    fn floor_bounds_wind_down_and_may_sit_below_one_transaction() {
        let metrics = test_metrics();
        let cap = cap();
        for _ in 0..100 {
            cap.observe_round(Duration::from_millis(60_000), &metrics.node_metrics);
        }
        assert_eq!(cap.budget_bytes(), FLOOR_BYTES);
        assert!(cap.budget_bytes() < 128 * 1024);
    }

    #[test]
    fn recovers_toward_ceiling_when_rounds_are_fast() {
        let metrics = test_metrics();
        let cap = cap();
        for _ in 0..20 {
            cap.observe_round(Duration::from_millis(3000), &metrics.node_metrics);
        }
        let shrunk = cap.budget_bytes();
        for _ in 0..500 {
            cap.observe_round(Duration::from_millis(10), &metrics.node_metrics);
        }
        assert!(cap.budget_bytes() > shrunk);
        assert_eq!(cap.budget_bytes(), 512 * 1024);
    }

    /// An absurd round period must saturate rather than wrap, and must still only move the
    /// budget by one bounded step — a single transient cannot collapse it.
    #[test]
    fn extreme_samples_saturate_without_corrupting() {
        let metrics = test_metrics();
        let cap = cap();

        cap.observe_round(Duration::MAX, &metrics.node_metrics);
        assert_eq!(
            cap.budget_bytes(),
            ((512 * 1024) as f64 * MAX_DECREASE_FACTOR) as u64,
            "one observation may shrink the budget by at most MAX_DECREASE_FACTOR"
        );

        for _ in 0..20 {
            cap.observe_round(Duration::MAX, &metrics.node_metrics);
        }
        assert_eq!(cap.budget_bytes(), cap.floor_bytes);

        for _ in 0..2_000 {
            cap.observe_round(Duration::from_millis(10), &metrics.node_metrics);
        }
        assert_eq!(cap.budget_bytes(), 512 * 1024);
    }

    /// A stalled node reports a period unrelated to network health. Clamping the sample bounds
    /// how long the average stays poisoned, and therefore how long the budget stays floored.
    #[test]
    fn oversized_sample_does_not_produce_a_long_tail() {
        let metrics = test_metrics();
        let cap = cap();

        cap.observe_round(Duration::from_secs(600), &metrics.node_metrics);

        // Count healthy observations until the budget first moves upward, i.e. until the average
        // has decayed back under the recovery threshold.
        let mut prev = cap.budget_bytes();
        let mut n = 0;
        while n < 200 {
            cap.observe_round(Duration::from_millis(10), &metrics.node_metrics);
            n += 1;
            let current = cap.budget_bytes();
            if current > prev {
                break;
            }
            prev = current;
        }
        assert!(n <= 30, "recovery began only after {n} healthy observations");
    }


    /// Inside the deadband the budget holds steady rather than oscillating.
    #[test]
    fn holds_steady_within_deadband() {
        let metrics = test_metrics();
        let cap = cap();
        for _ in 0..10 {
            cap.observe_round(Duration::from_millis(135), &metrics.node_metrics);
        }
        assert_eq!(cap.budget_bytes(), 512 * 1024);
    }
}

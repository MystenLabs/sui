// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn on_sample(&self, outcome: Outcome);
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

    /// Record that a new task has started. Call [`release`](Self::release) when it completes.
    pub fn acquire(&self) {
        self.inflight.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a task has completed (regardless of outcome).
    pub fn release(&self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Limit for AimdLimit {
    fn current(&self) -> usize {
        let state = self.inner.lock().unwrap();
        state.current
    }

    fn on_sample(&self, outcome: Outcome) {
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
    fn simulate_inflight(limiter: &AimdLimit, n: usize) {
        for _ in 0..n {
            limiter.acquire();
        }
    }

    #[test]
    fn success_increases_limit_by_one() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        simulate_inflight(&limiter, 10);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 11);
    }

    #[test]
    fn drop_decreases_limit_by_backoff_ratio() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        limiter.on_sample(Outcome::Dropped);
        // floor(10 * 0.9) = 9
        assert_eq!(limiter.current(), 9);
    }

    #[test]
    fn ignore_has_no_effect() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);
        limiter.on_sample(Outcome::Ignore);
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
        limiter.on_sample(Outcome::Dropped);
        assert_eq!(limiter.current(), 2); // max(2, floor(2*0.5)=1) = 2

        // Increase should not go above max_limit
        simulate_inflight(&limiter, 2);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 3);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 3); // clamped at max
    }

    #[test]
    fn multiple_drops_reduce_progressively() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);

        limiter.on_sample(Outcome::Dropped); // floor(10 * 0.9) = 9
        assert_eq!(limiter.current(), 9);

        limiter.on_sample(Outcome::Dropped); // floor(9 * 0.9) = 8
        assert_eq!(limiter.current(), 8);

        limiter.on_sample(Outcome::Dropped); // floor(8 * 0.9) = 7
        assert_eq!(limiter.current(), 7);
    }

    #[test]
    fn recovery_after_drop() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        simulate_inflight(&limiter, 10);

        limiter.on_sample(Outcome::Dropped); // 10 -> 9
        assert_eq!(limiter.current(), 9);

        limiter.on_sample(Outcome::Success); // 9 -> 10
        assert_eq!(limiter.current(), 10);

        limiter.on_sample(Outcome::Success); // 10 -> 11
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
        limiter.on_sample(Outcome::Success);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 10);

        // Drop resets the counter
        limiter.on_sample(Outcome::Dropped); // 10 -> 9
        assert_eq!(limiter.current(), 9);

        // Need 3 consecutive successes again
        limiter.on_sample(Outcome::Success);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 9); // still 9, only 2 successes

        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 10); // now 3 consecutive -> increase
    }

    #[test]
    fn watch_channel_receives_updates() {
        let (limiter, mut rx) = AimdLimit::new(default_config());
        assert_eq!(*rx.borrow(), 10);
        simulate_inflight(&limiter, 10);

        limiter.on_sample(Outcome::Success);
        assert_eq!(*rx.borrow_and_update(), 11);

        limiter.on_sample(Outcome::Dropped);
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

        limiter.on_sample(Outcome::Success);
        limiter.on_sample(Outcome::Ignore); // should not reset counter
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 11); // 2 successes reached
    }

    #[test]
    fn no_increase_when_underutilized() {
        let (limiter, _rx) = AimdLimit::new(default_config());
        assert_eq!(limiter.current(), 10);

        // With 0 inflight (well under limit/2 = 5), success should not increase
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 10);

        // With 4 inflight (still under limit/2 = 5), should not increase
        simulate_inflight(&limiter, 4);
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 10);

        // With 5 inflight (= limit/2), should increase
        limiter.acquire();
        limiter.on_sample(Outcome::Success);
        assert_eq!(limiter.current(), 11);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sui_protocol_config::ProtocolConfig;

/// Tracks how many gasless transactions were included in consensus commits
/// within the current 1-second window. Updated by the consensus handler on
/// each commit, and read by the rate limiter to make admission decisions.
pub struct ConsensusGaslessCounter {
    window_second: AtomicU64,
    count: AtomicU64,
}

impl Default for ConsensusGaslessCounter {
    fn default() -> Self {
        Self {
            window_second: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }
}

impl ConsensusGaslessCounter {
    pub fn record_commit(&self, commit_timestamp_ms: u64, gasless_count: u64) {
        let second = commit_timestamp_ms / 1000;
        let current = self.window_second.load(Ordering::Relaxed);
        if second > current {
            self.window_second.store(second, Ordering::Relaxed);
            self.count.store(gasless_count, Ordering::Relaxed);
        } else if second == current {
            self.count.fetch_add(gasless_count, Ordering::Relaxed);
        }
    }

    pub fn current_count(&self) -> (u64, u64) {
        let window = self.window_second.load(Ordering::Relaxed);
        let count = self.count.load(Ordering::Relaxed);
        (window, count)
    }
}

/// Per-validator rate limiter for gasless transactions. Uses the consensus-fed
/// global counter to reject new gasless transactions when the network-wide TPS
/// exceeds the configured `gasless_max_tps` threshold.
#[derive(Clone)]
pub struct GaslessRateLimiter {
    counter: Arc<ConsensusGaslessCounter>,
}

impl GaslessRateLimiter {
    pub fn new(counter: Arc<ConsensusGaslessCounter>) -> Self {
        Self { counter }
    }

    pub fn try_acquire(&self, config: &ProtocolConfig) -> bool {
        let Some(max_tps) = config.gasless_max_tps_as_option() else {
            return true;
        };
        let (_, count) = self.counter.current_count();
        count < max_tps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_protocol_config::ProtocolVersion;

    fn make_config(max_tps: u64) -> ProtocolConfig {
        let mut config = ProtocolConfig::get_for_version(
            ProtocolVersion::MAX,
            sui_protocol_config::Chain::Unknown,
        );
        config.enable_gasless_for_testing();
        config.set_gasless_max_tps_for_testing(max_tps);
        config
    }

    #[test]
    fn test_unset_is_unlimited() {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        let limiter = GaslessRateLimiter::new(counter);
        let config = ProtocolConfig::get_for_version(
            ProtocolVersion::new(117),
            sui_protocol_config::Chain::Unknown,
        );
        assert!(limiter.try_acquire(&config));
    }

    #[test]
    fn test_zero_max_tps_always_rejects() {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        let limiter = GaslessRateLimiter::new(counter);
        let config = make_config(0);
        assert!(!limiter.try_acquire(&config));
    }

    #[test]
    fn test_record_commit_new_window_resets() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(1000, 10);
        assert_eq!(counter.current_count(), (1, 10));

        counter.record_commit(2000, 5);
        assert_eq!(counter.current_count(), (2, 5));
    }

    #[test]
    fn test_record_commit_same_window_accumulates() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(1000, 10);
        counter.record_commit(1500, 7);
        assert_eq!(counter.current_count(), (1, 17));
    }

    #[test]
    fn test_record_commit_past_window_ignored() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(2000, 10);
        counter.record_commit(1000, 99);
        assert_eq!(counter.current_count(), (2, 10));
    }

    #[test]
    fn test_try_acquire_rejects_at_capacity() {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        counter.record_commit(1000, 5);
        let limiter = GaslessRateLimiter::new(counter);
        let config = make_config(5);
        assert!(!limiter.try_acquire(&config));
    }

    #[test]
    fn test_try_acquire_allows_under_capacity() {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        counter.record_commit(1000, 4);
        let limiter = GaslessRateLimiter::new(counter);
        let config = make_config(5);
        assert!(limiter.try_acquire(&config));
    }

    #[test]
    fn test_window_resets_after_non_gasless_commit() {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        counter.record_commit(1000, 5);
        let limiter = GaslessRateLimiter::new(counter.clone());
        let config = make_config(5);
        assert!(!limiter.try_acquire(&config));

        counter.record_commit(2000, 0);
        assert!(limiter.try_acquire(&config));
    }
}

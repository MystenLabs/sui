// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use sui_protocol_config::ProtocolConfig;

/// Tracks how many gasless transactions were included in consensus commits
/// within the current 1-second window. Updated by the consensus handler on
/// each commit, and read by the rate limiter to make admission decisions.
pub struct ConsensusGaslessCounter {
    inner: Mutex<ConsensusWindowInner>,
}

struct ConsensusWindowInner {
    window_second: u64,
    count: u64,
}

impl Default for ConsensusGaslessCounter {
    fn default() -> Self {
        Self {
            inner: Mutex::new(ConsensusWindowInner {
                window_second: 0,
                count: 0,
            }),
        }
    }
}

impl ConsensusGaslessCounter {
    pub fn record_commit(&self, commit_timestamp_ms: u64, gasless_count: u64) {
        let second = commit_timestamp_ms / 1000;
        let mut inner = self.inner.lock();
        if second > inner.window_second {
            inner.window_second = second;
            inner.count = gasless_count;
        } else if second == inner.window_second {
            inner.count += gasless_count;
        }
    }

    pub fn current_count(&self) -> u64 {
        self.inner.lock().count
    }
}

/// Per-validator fixed-window counter. Resets every second.
struct FixedWindowCounter {
    count: u64,
    window_start: Instant,
}

impl FixedWindowCounter {
    fn try_acquire(&mut self, max_tps: u64) -> bool {
        if self.window_start.elapsed().as_secs() >= 1 {
            self.count = 0;
            self.window_start = Instant::now();
        }
        if self.count < max_tps {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

/// Per-validator rate limiter for gasless transactions. Uses two layers:
///
/// 1. A local fixed-window counter to cap per-validator burst rate.
/// 2. A consensus-fed global counter for sustained network-wide accuracy.
///
/// Both are checked against `gasless_max_tps`. A transaction is admitted
/// only if both counters are under the limit.
#[derive(Clone)]
pub struct GaslessRateLimiter {
    consensus_counter: Arc<ConsensusGaslessCounter>,
    local: Arc<Mutex<FixedWindowCounter>>,
}

impl GaslessRateLimiter {
    pub fn new(consensus_counter: Arc<ConsensusGaslessCounter>) -> Self {
        Self {
            consensus_counter,
            local: Arc::new(Mutex::new(FixedWindowCounter {
                count: 0,
                window_start: Instant::now(),
            })),
        }
    }

    pub fn try_acquire(&self, config: &ProtocolConfig) -> bool {
        let Some(max_tps) = config.gasless_max_tps_as_option() else {
            return true;
        };
        if self.consensus_counter.current_count() >= max_tps {
            return false;
        }
        // no single validator can admit more than max_tps burst
        self.local.lock().try_acquire(max_tps)
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

    fn make_limiter() -> (Arc<ConsensusGaslessCounter>, GaslessRateLimiter) {
        let counter = Arc::new(ConsensusGaslessCounter::default());
        let limiter = GaslessRateLimiter::new(counter.clone());
        (counter, limiter)
    }

    // -- Config behavior tests --

    #[test]
    fn test_unset_is_unlimited() {
        let (_, limiter) = make_limiter();
        let config = ProtocolConfig::get_for_version(
            ProtocolVersion::new(117),
            sui_protocol_config::Chain::Unknown,
        );
        for _ in 0..100 {
            assert!(limiter.try_acquire(&config));
        }
    }

    #[test]
    fn test_zero_max_tps_always_rejects() {
        let (_, limiter) = make_limiter();
        let config = make_config(0);
        assert!(!limiter.try_acquire(&config));
    }

    // -- Consensus counter tests --

    #[test]
    fn test_record_commit_new_window_resets() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(1000, 10);
        assert_eq!(counter.current_count(), 10);

        counter.record_commit(2000, 5);
        assert_eq!(counter.current_count(), 5);
    }

    #[test]
    fn test_record_commit_same_window_accumulates() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(1000, 10);
        counter.record_commit(1500, 7);
        assert_eq!(counter.current_count(), 17);
    }

    #[test]
    fn test_record_commit_past_window_ignored() {
        let counter = ConsensusGaslessCounter::default();
        counter.record_commit(2000, 10);
        counter.record_commit(1000, 99);
        assert_eq!(counter.current_count(), 10);
    }

    // -- Local admission counter tests --

    #[test]
    fn test_local_counter_prevents_burst() {
        let (_, limiter) = make_limiter();
        let config = make_config(5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(&config));
        }
        assert!(!limiter.try_acquire(&config));
    }

    #[test]
    fn test_local_window_resets() {
        let (_, limiter) = make_limiter();
        let config = make_config(5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(&config));
        }
        assert!(!limiter.try_acquire(&config));
        std::thread::sleep(std::time::Duration::from_secs(1));
        for _ in 0..5 {
            assert!(limiter.try_acquire(&config));
        }
        assert!(!limiter.try_acquire(&config));
    }

    // -- Two-layer interaction tests --

    #[test]
    fn test_consensus_blocks_before_local_increment() {
        let (counter, limiter) = make_limiter();
        let config = make_config(5);
        counter.record_commit(1000, 5);
        assert!(!limiter.try_acquire(&config));
        counter.record_commit(2000, 0);
        for _ in 0..5 {
            assert!(limiter.try_acquire(&config));
        }
    }

    #[test]
    fn test_consensus_rejects_at_capacity() {
        let (counter, limiter) = make_limiter();
        counter.record_commit(1000, 5);
        let config = make_config(5);
        assert!(!limiter.try_acquire(&config));
    }

    #[test]
    fn test_consensus_allows_under_capacity() {
        let (counter, limiter) = make_limiter();
        counter.record_commit(1000, 4);
        let config = make_config(5);
        assert!(limiter.try_acquire(&config));
    }

    #[test]
    fn test_window_resets_after_non_gasless_commit() {
        let (counter, limiter) = make_limiter();
        counter.record_commit(1000, 5);
        let config = make_config(5);
        assert!(!limiter.try_acquire(&config));

        counter.record_commit(2000, 0);
        assert!(limiter.try_acquire(&config));
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use sui_protocol_config::ProtocolConfig;
use sui_types::committee::EpochId;

struct FixedWindowInner {
    max_tps: Option<u64>,
    count: u64,
    window_start: Instant,
    epoch: EpochId,
}

impl FixedWindowInner {
    fn try_acquire(&mut self, epoch: EpochId, config: &ProtocolConfig) -> bool {
        if epoch != self.epoch {
            self.reinit(epoch, config);
        }
        let Some(max_tps) = self.max_tps else {
            return true;
        };

        let now = Instant::now();
        if now.duration_since(self.window_start).as_secs() >= 1 {
            self.count = 0;
            self.window_start = now;
        }

        if self.count < max_tps {
            self.count += 1;
            true
        } else {
            false
        }
    }

    fn reinit(&mut self, epoch: EpochId, config: &ProtocolConfig) {
        self.max_tps = config.gasless_max_tps_per_validator_as_option();
        self.count = 0;
        self.window_start = Instant::now();
        self.epoch = epoch;
    }
}

#[derive(Clone)]
pub struct GaslessRateLimiter {
    inner: Arc<Mutex<FixedWindowInner>>,
}

impl Default for GaslessRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl GaslessRateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FixedWindowInner {
                max_tps: None,
                count: 0,
                window_start: Instant::now(),
                epoch: u64::MAX,
            })),
        }
    }

    pub fn try_acquire(&self, epoch: EpochId, config: &ProtocolConfig) -> bool {
        self.inner.lock().try_acquire(epoch, config)
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
        config.set_gasless_max_tps_per_validator_for_testing(max_tps);
        config
    }

    #[test]
    fn test_disabled_always_allows() {
        let limiter = GaslessRateLimiter::new();
        let config = ProtocolConfig::get_for_version(
            ProtocolVersion::new(117),
            sui_protocol_config::Chain::Unknown,
        );
        for _ in 0..100 {
            assert!(limiter.try_acquire(1, &config));
        }
    }

    #[test]
    fn test_cap_then_reject() {
        let limiter = GaslessRateLimiter::new();
        let config = make_config(5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(1, &config));
        }
        assert!(!limiter.try_acquire(1, &config));
    }

    #[test]
    fn test_no_burst_accumulation() {
        let limiter = GaslessRateLimiter::new();
        let config = make_config(5);
        std::thread::sleep(std::time::Duration::from_millis(100));
        for _ in 0..5 {
            assert!(limiter.try_acquire(1, &config));
        }
        assert!(
            !limiter.try_acquire(1, &config),
            "Should not allow burst beyond max_tps"
        );
    }

    #[test]
    fn test_window_reset() {
        let limiter = GaslessRateLimiter::new();
        let config = make_config(5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(1, &config));
        }
        assert!(!limiter.try_acquire(1, &config));
        std::thread::sleep(std::time::Duration::from_secs(1));
        for _ in 0..5 {
            assert!(limiter.try_acquire(1, &config));
        }
        assert!(!limiter.try_acquire(1, &config));
    }

    #[test]
    fn test_epoch_change_reinits() {
        let limiter = GaslessRateLimiter::new();
        let config = make_config(5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(1, &config));
        }
        assert!(!limiter.try_acquire(1, &config));
        for _ in 0..5 {
            assert!(limiter.try_acquire(2, &config));
        }
        assert!(!limiter.try_acquire(2, &config));
    }
}

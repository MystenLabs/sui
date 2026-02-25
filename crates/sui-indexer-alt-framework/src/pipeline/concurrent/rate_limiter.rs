// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU32;
use std::sync::Arc;

use governor::Quota;
use governor::RateLimiter as GovRateLimiter;

/// A single token-bucket rate limiter that can be shared across pipelines via `Arc`.
pub(crate) struct RateLimiter {
    limiter: governor::DefaultDirectRateLimiter,
    burst: NonZeroU32,
}

/// A composed rate limiter that acquires tokens from one or more underlying limiters.
/// Used to enforce both per-pipeline and indexer-wide rate limits with a single `acquire` call.
pub(crate) struct CompositeRateLimiter {
    limiters: Vec<Arc<RateLimiter>>,
}

impl RateLimiter {
    pub(crate) fn new(max_rows_per_second: u64) -> Arc<Self> {
        let burst =
            NonZeroU32::new(max_rows_per_second as u32).expect("max_rows_per_second must be > 0");
        let quota = Quota::per_second(burst);
        Arc::new(Self {
            limiter: GovRateLimiter::direct(quota),
            burst,
        })
    }

    async fn acquire(&self, count: usize) {
        let burst = self.burst.get();
        let mut remaining = count as u32;
        while remaining > 0 {
            let take = remaining.min(burst);
            self.limiter
                .until_n_ready(NonZeroU32::new(take).unwrap())
                .await
                .expect("take <= burst, so this cannot fail");
            remaining -= take;
        }
    }
}

impl CompositeRateLimiter {
    pub(crate) fn new(limiters: Vec<Arc<RateLimiter>>) -> Self {
        Self { limiters }
    }

    pub(crate) fn noop() -> Self {
        Self {
            limiters: Vec::new(),
        }
    }

    /// Acquire `count` tokens from every underlying limiter concurrently.
    pub(crate) async fn acquire(&self, count: usize) {
        futures::future::join_all(self.limiters.iter().map(|l| l.acquire(count))).await;
    }
}

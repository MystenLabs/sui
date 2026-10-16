// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::default::Default;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::time::Duration;

pub struct LatencyObserver {
    data: Mutex<LatencyObserverInner>,
    latency_ms: AtomicU64,
}

#[derive(Default)]
struct LatencyObserverInner {
    points: VecDeque<Duration>,
    sum: Duration,
}

impl LatencyObserver {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(LatencyObserverInner::default()),
            latency_ms: AtomicU64::new(u64::MAX),
        }
    }

    pub fn report(&self, latency: Duration) {
        const EXPECTED_SAMPLES: usize = 128;
        let mut data = self.data.lock();
        data.points.push_back(latency);
        data.sum += latency;
        if data.points.len() < EXPECTED_SAMPLES {
            // Do not initialize average latency until there are enough samples.
            return;
        }
        while data.points.len() > EXPECTED_SAMPLES {
            let pop = data.points.pop_front().expect("data vector is not empty");
            data.sum -= pop; // This does not underflow because of how running sum is calculated
        }
        let latency = data.sum.as_millis() as u64 / data.points.len() as u64;
        self.latency_ms.store(latency, Ordering::Relaxed);
    }

    pub fn latency(&self) -> Option<Duration> {
        let latency = self.latency_ms.load(Ordering::Relaxed);
        if latency == u64::MAX {
            // Not initialized yet (not enough data points)
            None
        } else {
            Some(Duration::from_millis(latency))
        }
    }
}

impl Default for LatencyObserver {
    fn default() -> Self {
        Self::new()
    }
}

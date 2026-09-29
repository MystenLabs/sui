// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Execution progress published by the execution driver, read by the consensus
/// transaction pool to pace user transaction admission.
#[derive(Default)]
pub struct ExecutionProgress {
    /// Transactions this authority has executed successfully.
    pub executed_transactions: AtomicU64,

    /// Sampled time that ready transactions wait for an execution permit.
    pub queueing_latency: LatencyObserver,
}

/// Mean of the most recent latency samples, available once enough have been reported.
pub struct LatencyObserver {
    data: Mutex<LatencyObserverInner>,
    latency_ms: AtomicU64,
}

impl Default for LatencyObserver {
    fn default() -> Self {
        Self {
            data: Mutex::new(LatencyObserverInner::default()),
            latency_ms: AtomicU64::new(Self::UNINITIALIZED),
        }
    }
}

struct LatencyObserverInner {
    points: VecDeque<Duration>,
    sum: Duration,
}

impl Default for LatencyObserverInner {
    fn default() -> Self {
        Self {
            points: VecDeque::new(),
            sum: Duration::ZERO,
        }
    }
}

impl LatencyObserver {
    const EXPECTED_SAMPLES: usize = 128;
    const UNINITIALIZED: u64 = u64::MAX;

    pub fn report(&self, latency: Duration) {
        let mut data = self.data.lock();
        data.points.push_back(latency);
        data.sum += latency;
        if data.points.len() < Self::EXPECTED_SAMPLES {
            return;
        }
        while data.points.len() > Self::EXPECTED_SAMPLES {
            let pop = data.points.pop_front().expect("data vector is not empty");
            data.sum -= pop;
        }
        let latency = data.sum.as_millis() as u64 / data.points.len() as u64;
        self.latency_ms.store(latency, Ordering::Relaxed);
    }

    pub fn latency(&self) -> Option<Duration> {
        match self.latency_ms.load(Ordering::Relaxed) {
            Self::UNINITIALIZED => None,
            latency => Some(Duration::from_millis(latency)),
        }
    }
}

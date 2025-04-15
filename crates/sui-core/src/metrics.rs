// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::default::Default;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::time::Duration;
use tokio::time::Instant;

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

/// RateTracker tracks events in a rolling window, and calculates the rate of events.
/// Internally, the tracker divides the tracking window into multiple BIN_DURATION,
/// and counts events in each BIN_DURATION in a fixed sized buffer.
pub struct RateTracker {
    // Counts the number of events by bins. Each bin is BIN_DURATION long within window_duration.
    // The size of the buffer = window_duration / BIN_DURATION.
    event_buffer: Vec<u64>,
    window_duration: Duration,
    total_bins: usize,

    // We use the event time and the tracker start time to calculate the bin that an event
    // belongs to.
    // event_global_bin_index = (event_time - start_time) / BIN_DURATION.
    // event_index_in_buffer = event_global_bin_index % buffer_size.
    start_time: Instant,

    // Last updated global bin index. This tracks the end of the rolling window.
    global_bin_index: u64,
}

const BIN_DURATION: Duration = Duration::from_millis(100);

impl RateTracker {
    /// Create a new RateTracker to track event rate (events/seconds) in `window_duration`.
    pub fn new(window_duration: Duration) -> Self {
        assert!(window_duration > BIN_DURATION);
        let total_bins = (window_duration.as_millis() / BIN_DURATION.as_millis()) as usize;
        RateTracker {
            event_buffer: vec![0; total_bins],
            window_duration,
            total_bins,
            start_time: Instant::now(),
            global_bin_index: 0,
        }
    }

    /// Records an event at time `now`.
    pub fn record_at_time(&mut self, now: Instant) {
        self.update_window(now);
        let current_bin_index = self.get_bin_index(now) as usize;
        if current_bin_index + self.total_bins <= self.global_bin_index as usize {
            // The bin associated with `now` has passed the rolling window.
            return;
        }

        self.event_buffer[current_bin_index % self.total_bins] += 1;
    }

    /// Records an event at current time.
    pub fn record(&mut self) {
        self.record_at_time(Instant::now());
    }

    /// Returns the rate of events.
    pub fn rate(&mut self) -> f64 {
        let now = Instant::now();
        self.update_window(now);
        self.event_buffer.iter().sum::<u64>() as f64 / self.window_duration.as_secs_f64()
    }

    // Given a time `now`, returns the bin index since `start_time`.
    fn get_bin_index(&self, now: Instant) -> u64 {
        (now.duration_since(self.start_time).as_millis() / BIN_DURATION.as_millis()) as u64
    }

    // Updates the rolling window to accommodate the time of interests, `now`. That is, remove any
    // event counts happened prior to (`now` - `window_duration`).
    fn update_window(&mut self, now: Instant) {
        let current_bin_index = self.get_bin_index(now);
        if self.global_bin_index >= current_bin_index {
            // The rolling doesn't move.
            return;
        }

        for bin_index in (self.global_bin_index + 1)..=current_bin_index {
            // Time has elapsed from global_bin_index to current_bin_index. Clear all the buffer
            // counter associated with them.
            let index_in_buffer = bin_index as usize % self.total_bins;
            self.event_buffer[index_in_buffer] = 0;
        }
        self.global_bin_index = current_bin_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::rngs::StdRng;
    use rand::Rng;
    use rand::SeedableRng;
    use tokio::time::advance;

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_rate_tracker_basic() {
        // 1 sec rolling window.
        let mut tracker = RateTracker::new(Duration::from_secs(1));
        assert_eq!(tracker.rate(), 0.0);
        tracker.record();
        tracker.record();
        tracker.record();
        assert_eq!(tracker.rate(), 3.0);

        advance(Duration::from_millis(200)).await;
        tracker.record();
        tracker.record();
        tracker.record();
        assert_eq!(tracker.rate(), 6.0);

        advance(Duration::from_millis(800)).await;
        assert_eq!(tracker.rate(), 3.0);

        advance(Duration::from_millis(200)).await;
        assert_eq!(tracker.rate(), 0.0);
    }

    // Tests rate calculation using different window duration.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_rate_tracker_window() {
        let seed = [0; 32];
        let mut rng = StdRng::from_seed(seed);
        let random_windows: Vec<u64> = (0..10).map(|_| rng.gen_range(1..=60)).collect();
        for window in random_windows {
            let mut tracker = RateTracker::new(Duration::from_secs(window));
            for _ in 0..23 {
                tracker.record();
            }
            assert_eq!(tracker.rate(), 23.0 / window as f64);
            advance(Duration::from_secs(window)).await;
            assert_eq!(tracker.rate(), 0.0);
        }
    }

    // Tests rate calculation when window moves continuously.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_rate_tracker_rolling_window() {
        let mut tracker = RateTracker::new(Duration::from_secs(1));
        // Generate event every 100ms.
        for i in 0..10 {
            tracker.record();
            assert_eq!(tracker.rate(), (i + 1) as f64);
            advance(Duration::from_millis(100)).await;
        }

        // Generate event every 50ms.
        for i in 0..10 {
            tracker.record();
            advance(Duration::from_millis(50)).await;
            tracker.record();
            assert_eq!(tracker.rate(), 11.0 + i as f64);
            advance(Duration::from_millis(50)).await;
        }

        // Rate gradually returns to 0.
        for i in 0..10 {
            assert_eq!(tracker.rate(), 20.0 - (i as f64 + 1.0) * 2.0);
            advance(Duration::from_millis(100)).await;
        }
        assert_eq!(tracker.rate(), 0.0);
    }

    // Tests that events happened prior to tracking window shouldn't affect the rate.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_rate_tracker_outside_of_window() {
        let mut tracker = RateTracker::new(Duration::from_secs(1));
        advance(Duration::from_secs(60)).await;
        tracker.record();
        tracker.record();
        tracker.record();
        assert_eq!(tracker.rate(), 3.0);
        tracker.record_at_time(Instant::now() - Duration::from_millis(1100));
        tracker.record_at_time(Instant::now() - Duration::from_millis(1100));
        tracker.record_at_time(Instant::now() - Duration::from_millis(1100));
        assert_eq!(tracker.rate(), 3.0);
    }
}

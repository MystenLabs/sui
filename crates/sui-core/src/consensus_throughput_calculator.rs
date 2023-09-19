// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::AuthorityMetrics;
use arc_swap::ArcSwapOption;
use narwhal_types::TimestampMs;
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use tracing::{info, warn};

const DEFAULT_OBSERVATIONS_WINDOW: u64 = 120; // number of observations to use to calculate the past traffic
const DEFAULT_TRAFFIC_PROFILE_UPDATE_WINDOW_SECS: u64 = 60; // seconds that need to pass between two consenqutive traffic profile updates

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrafficProfile {
    Low,
    High,
}

impl TrafficProfile {
    fn as_int(&self) -> usize {
        match self {
            TrafficProfile::Low => 0,
            TrafficProfile::High => 1,
        }
    }
}

#[derive(Default)]
pub struct TrafficProfileRangesBuilder {
    profiles: BTreeMap<u64, TrafficProfile>,
}

impl TrafficProfileRangesBuilder {
    /// Adds a new profile with its upper range threshold. Ex if the values (2000, TrafficProfile::MIN) are provided,
    /// then for provided throughput <= 2000 the traffic profile MIN will be returned.
    pub fn add_profile(
        mut self,
        upper_threshold: u64,
        profile: TrafficProfile,
    ) -> TrafficProfileRangesBuilder {
        assert!(
            self.profiles.insert(upper_threshold, profile).is_none(),
            "{}",
            format!(
                "Attempted to add overriding profile for same upper threshold {} {:?}",
                upper_threshold, profile
            )
        );
        self
    }

    /// Adds a traffic profile that should be returned after the penultimate profile.
    pub fn add_max_threshold_profile(self, profile: TrafficProfile) -> TrafficProfileRangesBuilder {
        self.add_profile(u64::MAX, profile)
    }

    pub fn build(self) -> Result<TrafficProfileRanges, String> {
        // ensure that we have added a profile to cover the throughput up to max, otherwise might end up
        // not able to figure out the profile during runtime.
        if !self.profiles.contains_key(&u64::MAX) {
            return Err("Builder should always include the profile for max value".to_string());
        }

        Ok(TrafficProfileRanges {
            profiles: self.profiles,
        })
    }
}

pub struct TrafficProfileRanges {
    profiles: BTreeMap<u64, TrafficProfile>,
}

impl TrafficProfileRanges {
    /// Resolves the traffic profile that corresponds to the provided throughput. The method guarantees
    /// to always return a profile if the TrafficProfileRangesBuilder has been used to create the
    /// TrafficProfileRanges. In any other case a panic will be raised.
    pub fn resolve(&self, throughput: u64) -> TrafficProfile {
        for (threshold, profile) in &self.profiles {
            if *threshold >= throughput {
                return *profile;
            }
        }
        panic!("Method should always be able to detect a traffic profile to cover the provided throughput");
    }
}

impl Default for TrafficProfileRanges {
    fn default() -> Self {
        TrafficProfileRangesBuilder::default()
            .add_profile(2_000, TrafficProfile::Low)
            .add_max_threshold_profile(TrafficProfile::High)
            .build()
            .unwrap()
    }
}

pub type TimestampSecs = u64;

#[derive(Debug)]
pub struct TrafficProfileEntry {
    /// The traffic profile
    profile: TrafficProfile,
    /// The time when this traffic profile was created
    timestamp: TimestampSecs,
    /// The calculated throughput when this profile created
    #[allow(unused)]
    throughput: u64,
}

#[derive(Default)]
struct ConsensusThroughputCalculatorInner {
    observations: VecDeque<(TimestampSecs, u64)>,
    total_transactions: u64,
}

pub struct ConsensusThroughputCalculator {
    /// The number of transaction traffic observations that should be stored within the observations
    /// vector in the ConsensusThroughputCalculatorInner. Those observations will be used to calculate
    /// the current transactions throughput. We want to select a number that give us enough observations
    /// so we better calculate the throughput and protected against spikes. A large enough value though
    /// will make us less reactive to traffic changes.
    observations_window: u64,
    /// The time that should be passed between two consecutive traffic profile updates. For example, if
    /// we switch at point T to profile "Low", there will need to be passed at least `traffic_profile_update_window`
    /// seconds until the traffic profile gets updated to a different value.
    traffic_profile_update_window: TimestampSecs,
    inner: Mutex<ConsensusThroughputCalculatorInner>,
    last_traffic_profile: ArcSwapOption<TrafficProfileEntry>,
    current_throughput: AtomicU64,
    metrics: Arc<AuthorityMetrics>,
    profile_ranges: TrafficProfileRanges,
}

impl ConsensusThroughputCalculator {
    pub fn new(
        observations_window: Option<NonZeroU64>,
        traffic_profile_update_window: Option<TimestampSecs>,
        metrics: Arc<AuthorityMetrics>,
        profile_ranges: TrafficProfileRanges,
    ) -> Self {
        let traffic_profile_update_window =
            traffic_profile_update_window.unwrap_or(DEFAULT_TRAFFIC_PROFILE_UPDATE_WINDOW_SECS);
        let observations_window = observations_window
            .unwrap_or(NonZeroU64::new(DEFAULT_OBSERVATIONS_WINDOW).unwrap())
            .get();

        assert!(
            traffic_profile_update_window > 0,
            "traffic_profile_update_window should be >= 0"
        );

        Self {
            observations_window,
            traffic_profile_update_window,
            inner: Mutex::new(ConsensusThroughputCalculatorInner::default()),
            last_traffic_profile: ArcSwapOption::empty(), // assume high traffic so the node is more conservative on bootstrap
            current_throughput: AtomicU64::new(0),
            metrics,
            profile_ranges,
        }
    }

    // Adds an observation of the number of transactions that have been sequenced after deduplication
    // and the corresponding leader timestamp. The observation timestamps should be monotonically
    // incremented otherwise observation will be ignored.
    pub fn add_transactions(&self, timestamp_ms: TimestampMs, num_of_transactions: u64) {
        let mut inner = self.inner.lock();
        let timestamp_secs: TimestampSecs = timestamp_ms / 1_000; // lowest bucket we care is seconds

        // If it's the very first observation we just use it as timestamp and don't count any transactions.
        let num_of_transactions = if !inner.observations.is_empty() {
            num_of_transactions
        } else {
            0
        };

        if let Some((front_ts, transactions)) = inner.observations.pop_front() {
            // First check that the timestamp is monotonically incremented - ignore any observation that is not
            // later from previous one (it shouldn't really happen).
            if timestamp_secs < front_ts {
                warn!("Ignoring observation of transactions:{} as has earlier timestamp than last observation {}s < {}s", num_of_transactions, timestamp_secs, front_ts);
                return;
            }

            // Not very likely, but if transactions refer to same second we add to the last element.
            if timestamp_secs == front_ts {
                inner
                    .observations
                    .push_front((front_ts, transactions + num_of_transactions));
            } else {
                inner.observations.push_front((front_ts, transactions));
                inner
                    .observations
                    .push_front((timestamp_secs, num_of_transactions));
            }
        } else {
            inner
                .observations
                .push_front((timestamp_secs, num_of_transactions));
        }

        // update total number of transactions in the observations list
        inner.total_transactions = inner.total_transactions.saturating_add(num_of_transactions);

        // If we have more values on our window of max values, remove the last one, and update the num of transactions
        // We also update the traffic profile when we have at least observations_window values in our observations.
        if inner.observations.len() as u64 > self.observations_window {
            let (last_element_ts, last_element_transactions) =
                inner.observations.pop_back().unwrap();
            inner.total_transactions = inner
                .total_transactions
                .saturating_sub(last_element_transactions);

            // get the first element's timestamp to calculate the transaction rate
            let (first_element_ts, _first_element_transactions) = inner
                .observations
                .front()
                .expect("There should be at least on element in the list");

            let period = first_element_ts.saturating_sub(last_element_ts);

            if period > 0 {
                let current_throughput = inner.total_transactions / period;

                self.update_traffic_profile(current_throughput, timestamp_secs);
            } else {
                warn!("Skip calculating throughput as time period is {}. This is very unlikely to happen, should investigate.", period);
            }
        }
    }

    // Calculate and update the traffic profile based on the provided throughput. The traffic profile
    // will only get updated when a different value has been calculated. For example, if the
    // `last_traffic_profile` is `Low` , and again we calculate it as `Low` based on input, then we'll
    // not update the profile or the timestamp. We do care to perform updates only when profiles differ.
    // To ensure that we are protected against traffic profile change fluctuations, we only change a
    // traffic profile when `traffic_profile_update_window` seconds have passed since last update.
    fn update_traffic_profile(&self, throughput: u64, timestamp: TimestampSecs) {
        let profile = self.profile_ranges.resolve(throughput);

        let should_update_profile = self.last_traffic_profile.load().as_ref().map_or_else(
            || true,
            |entry| {
                // update only when we have a new profile
                profile != entry.profile
                    && timestamp - entry.timestamp >= self.traffic_profile_update_window
            },
        );

        if should_update_profile {
            let p = TrafficProfileEntry {
                profile,
                timestamp,
                throughput,
            };
            info!("Updating traffic profile to {:?}", p);
            println!("Updating traffic profile to {:?}", p);
            self.last_traffic_profile.store(Some(Arc::new(p)));
        }

        // Also update the current throughput
        self.current_throughput.store(throughput, Relaxed);
        self.metrics
            .consensus_calculated_throughput
            .set(throughput as i64);

        self.metrics
            .consensus_calculated_traffic_profile
            .set(self.traffic_profile().0.as_int() as i64);
    }

    // Return the current traffic profile and the corresponding throughput when this was last updated.
    // If that is not set yet then as default the High profile is returned and the throughput will be None.
    pub fn traffic_profile(&self) -> (TrafficProfile, u64) {
        let profile = self.last_traffic_profile.load();
        profile.as_ref().map_or_else(
            || (TrafficProfile::Low, 0),
            |entry| (entry.profile, entry.throughput),
        )
    }

    // Returns the current (live calculated) throughput. If want to get the current throughput use
    // this method. If want to figure out what was the throughput when the traffic profile was last
    // calculated then use the traffic_profile() method.
    #[allow(unused)]
    pub fn current_throughput(&self) -> u64 {
        self.current_throughput.load(Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_throughput_calculator::TrafficProfile::{High, Low};
    use prometheus::Registry;

    #[test]
    pub fn test_traffic_profile_ranges_builder() {
        let ranges = TrafficProfileRangesBuilder::default()
            .add_profile(2_000, Low)
            .add_max_threshold_profile(High)
            .build()
            .unwrap();

        assert_eq!(ranges.resolve(0), Low);
        assert_eq!(ranges.resolve(1_000), Low);
        assert_eq!(ranges.resolve(2_000), Low);
        assert_eq!(ranges.resolve(2_001), High);
        assert_eq!(ranges.resolve(u64::MAX), High);

        // When omitting to add the max threshold profile, the build method should return an error
        let builder = TrafficProfileRangesBuilder::default().add_profile(2_000, Low);

        assert!(builder.build().is_err());
    }

    #[test]
    pub fn test_consensus_throughput_calculator() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let traffic_profile_update_window: TimestampSecs = 5;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(3).unwrap();

        let ranges = TrafficProfileRangesBuilder::default()
            .add_profile(2_000, Low)
            .add_max_threshold_profile(High)
            .build()
            .unwrap();

        let calculator = ConsensusThroughputCalculator::new(
            Some(max_observation_points),
            Some(traffic_profile_update_window),
            metrics,
            ranges,
        );

        // When no transactions exists, the calculator will return by default "High" to err on the
        // assumption that there is lots of load.
        assert_eq!(calculator.traffic_profile(), (Low, 0));

        calculator.add_transactions(1000 as TimestampMs, 1_000);
        calculator.add_transactions(2000 as TimestampMs, 1_000);
        calculator.add_transactions(3000 as TimestampMs, 1_000);
        calculator.add_transactions(4000 as TimestampMs, 1_000);

        // We expect to have a rate of 1K tx/sec, that's < 2K limit , so traffic profile is set to "low"
        assert_eq!(calculator.traffic_profile(), (Low, 1_000));

        // We add more transactions to go over 2K tx/sec, but time window threshold not satisfied yet,
        // and the profile is not updated yet
        calculator.add_transactions(5_000 as TimestampMs, 2_500);
        calculator.add_transactions(6_000 as TimestampMs, 2_800);
        calculator.add_transactions(7_000 as TimestampMs, 2_500);

        assert_eq!(calculator.traffic_profile(), (Low, 1000));

        // We are adding more transactions to get over 2K tx/sec, so traffic profile should now be categorised
        // as "high"
        calculator.add_transactions(8_000 as TimestampMs, 2_500);
        calculator.add_transactions(9_000 as TimestampMs, 3_000);

        assert_eq!(calculator.traffic_profile(), (High, 2666));
        assert_eq!(calculator.current_throughput(), 2666);

        // Let's now add 0 transactions after 5 seconds. Since 5 seconds have passed since the last
        // update and now the transactions are 0 we expect the traffic to be calculate as:
        // 3000 + 2500 + 0 = 5500 / 15 - 7sec = 5500 / 8sec = 687 tx/sec
        calculator.add_transactions(15_000 as TimestampMs, 0);

        assert_eq!(calculator.traffic_profile(), (Low, 687));
        assert_eq!(calculator.current_throughput(), 687);

        // Adding zero transactions for the next 5 seconds will make throughput zero
        // Traffic profile will remain as Low as it won't get updated.
        calculator.add_transactions(17_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), 333);

        calculator.add_transactions(19_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), 0);

        calculator.add_transactions(20_000 as TimestampMs, 0);

        assert_eq!(calculator.traffic_profile(), (Low, 687));
        assert_eq!(calculator.current_throughput(), 0);

        // By adding now a few entries with lots of transactions will trigger a traffic profile update
        // since the last one happened on timestamp 15_000ms.
        calculator.add_transactions(21_000 as TimestampMs, 1_000);
        calculator.add_transactions(22_000 as TimestampMs, 2_000);
        calculator.add_transactions(23_000 as TimestampMs, 3_100);
        assert_eq!(calculator.current_throughput(), 2033);
        assert_eq!(calculator.traffic_profile(), (High, 2033));
    }

    #[test]
    pub fn test_same_timestamp_observations() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let traffic_profile_update_window: TimestampSecs = 5;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(2).unwrap();

        let ranges = TrafficProfileRangesBuilder::default()
            .add_profile(100, Low)
            .add_max_threshold_profile(High)
            .build()
            .unwrap();

        let calculator = ConsensusThroughputCalculator::new(
            Some(max_observation_points),
            Some(traffic_profile_update_window),
            metrics,
            ranges,
        );

        // adding one observation
        calculator.add_transactions(1_000, 0);

        // Adding observations with same timestamp should fall under the same bucket
        for _ in 0..10 {
            calculator.add_transactions(2_340, 100);
        }

        // should not produce more than one record to trigger a throughput change
        assert_eq!(calculator.traffic_profile().0, Low);

        // Adding now one observation will trigger a profile change
        calculator.add_transactions(4_000, 0);

        assert_eq!(calculator.traffic_profile().0, High);
        assert_eq!(calculator.current_throughput(), 333);
    }
}

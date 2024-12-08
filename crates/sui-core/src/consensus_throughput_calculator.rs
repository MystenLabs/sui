// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::Arc;
use sui_protocol_config::Chain;
use sui_types::digests::ChainIdentifier;
use sui_types::messages_consensus::TimestampMs;
use tracing::{debug, warn};

use crate::authority::AuthorityMetrics;

const DEFAULT_OBSERVATIONS_WINDOW: u64 = 120; // number of observations to use to calculate the past throughput
const DEFAULT_THROUGHPUT_PROFILE_UPDATE_INTERVAL_SECS: u64 = 60; // seconds that need to pass between two consecutive throughput profile updates
const DEFAULT_THROUGHPUT_PROFILE_COOL_DOWN_THRESHOLD: u64 = 10; // 10% of throughput

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub struct ThroughputProfile {
    pub level: Level,
    /// The lower range of the throughput that this profile is referring to. For example, if
    /// `throughput = 1_000`, then for values >= 1_000 this throughput profile applies.
    pub throughput: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub enum Level {
    Low,
    Medium,
    High,
}

impl From<usize> for Level {
    fn from(value: usize) -> Self {
        if value == 0 {
            Level::Low
        } else if value == 1 {
            Level::Medium
        } else {
            Level::High
        }
    }
}

impl From<Level> for usize {
    fn from(value: Level) -> Self {
        match value {
            Level::Low => 0,
            Level::Medium => 1,
            Level::High => 2,
        }
    }
}

#[derive(Debug)]
pub struct ThroughputProfileRanges {
    /// Holds the throughput profiles by the throughput range (upper_throughput, cool_down_threshold)
    profiles: BTreeMap<u64, ThroughputProfile>,
}

impl ThroughputProfileRanges {
    pub fn from_chain(chain_id: ChainIdentifier) -> ThroughputProfileRanges {
        let to_profiles = |medium: u64, high: u64| -> Vec<ThroughputProfile> {
            vec![
                ThroughputProfile {
                    level: Level::Low,
                    throughput: 0,
                },
                ThroughputProfile {
                    level: Level::Medium,
                    throughput: medium,
                },
                ThroughputProfile {
                    level: Level::High,
                    throughput: high,
                },
            ]
        };

        match chain_id.chain() {
            Chain::Mainnet => ThroughputProfileRanges::new(&to_profiles(500, 2_000)),
            Chain::Testnet => ThroughputProfileRanges::new(&to_profiles(500, 2_000)),
            Chain::Unknown => ThroughputProfileRanges::new(&to_profiles(1_000, 2_000)),
        }
    }

    pub fn new(profiles: &[ThroughputProfile]) -> Self {
        let mut p: BTreeMap<u64, ThroughputProfile> = BTreeMap::new();

        for profile in profiles {
            assert!(
                !p.iter().any(|(_, pr)| pr.level == profile.level),
                "Attempted to insert profile with same level"
            );
            assert!(
                p.insert(profile.throughput, *profile).is_none(),
                "Attempted to insert profile with same throughput"
            );
        }

        // By default the Low profile should exist with throughput 0
        assert_eq!(
            *p.get(&0).unwrap(),
            ThroughputProfile {
                level: Level::Low,
                throughput: 0
            }
        );

        Self { profiles: p }
    }

    pub fn lowest_profile(&self) -> ThroughputProfile {
        *self
            .profiles
            .first_key_value()
            .expect("Should contain at least one throughput profile")
            .1
    }

    pub fn highest_profile(&self) -> ThroughputProfile {
        *self
            .profiles
            .last_key_value()
            .expect("Should contain at least one throughput profile")
            .1
    }
    /// Resolves the throughput profile that corresponds to the provided throughput.
    pub fn resolve(&self, current_throughput: u64) -> ThroughputProfile {
        let mut iter = self.profiles.iter();
        while let Some((threshold, profile)) = iter.next_back() {
            if current_throughput >= *threshold {
                return *profile;
            }
        }

        warn!("Could not resolve throughput profile for throughput {} - we shouldn't end up here. Fallback to lowest profile as default.", current_throughput);

        // If not found, then we should return the lowest possible profile as default to stay on safe side.
        self.highest_profile()
    }
}

impl Default for ThroughputProfileRanges {
    fn default() -> Self {
        let profiles = vec![
            ThroughputProfile {
                level: Level::Low,
                throughput: 0,
            },
            ThroughputProfile {
                level: Level::High,
                throughput: 2_000,
            },
        ];
        ThroughputProfileRanges::new(&profiles)
    }
}

pub type TimestampSecs = u64;

#[derive(Debug, Copy, Clone)]
pub struct ThroughputProfileEntry {
    /// The throughput profile
    profile: ThroughputProfile,
    /// The time when this throughput profile was created
    timestamp: TimestampSecs,
    /// The calculated throughput when this profile created
    throughput: u64,
}

#[derive(Default)]
struct ConsensusThroughputCalculatorInner {
    observations: VecDeque<(TimestampSecs, u64)>,
    total_transactions: u64,
    /// The last timestamp that we considered as oldest to calculate the throughput over the observations window.
    last_oldest_timestamp: Option<TimestampSecs>,
}

/// The ConsensusThroughputProfiler is responsible for assigning the right throughput profile by polling
/// the measured consensus throughput. It is important to rely on the ConsensusThroughputCalculator to measure
/// throughput as we need to make sure that validators will see an as possible consistent view to assign
/// the right profile.
pub struct ConsensusThroughputProfiler {
    /// The throughput profile will be eligible for update every `throughput_profile_update_interval` seconds.
    /// A bucketing approach is followed where the throughput timestamp is used in order to calculate on which
    /// seconds bucket is assigned to. When we detect a change on that bucket then an update is triggered (if a different
    /// profile is calculated). That allows validators to align on the update timing and ensure they will eventually
    /// converge as the consensus timestamps are used.
    throughput_profile_update_interval: TimestampSecs,
    /// When current calculated throughput (A) is lower than previous, and the assessed profile is now a lower than previous,
    /// we'll change to the lower profile only when (A) <= (previous_profile.throughput) * (100 - throughput_profile_cool_down_threshold) / 100.
    /// Otherwise we'll stick to the previous profile. We want to do that to avoid any jittery behaviour that alternates between two profiles.
    throughput_profile_cool_down_threshold: u64,
    /// The profile ranges to use to profile the throughput
    profile_ranges: ThroughputProfileRanges,
    /// The most recently calculated throughput profile
    last_throughput_profile: ArcSwap<ThroughputProfileEntry>,
    metrics: Arc<AuthorityMetrics>,
    /// The throughput calculator to use to derive the current throughput.
    calculator: Arc<ConsensusThroughputCalculator>,
}

impl ConsensusThroughputProfiler {
    pub fn new(
        calculator: Arc<ConsensusThroughputCalculator>,
        throughput_profile_update_interval: Option<TimestampSecs>,
        throughput_profile_cool_down_threshold: Option<u64>,
        metrics: Arc<AuthorityMetrics>,
        profile_ranges: ThroughputProfileRanges,
    ) -> Self {
        let throughput_profile_update_interval = throughput_profile_update_interval
            .unwrap_or(DEFAULT_THROUGHPUT_PROFILE_UPDATE_INTERVAL_SECS);
        let throughput_profile_cool_down_threshold = throughput_profile_cool_down_threshold
            .unwrap_or(DEFAULT_THROUGHPUT_PROFILE_COOL_DOWN_THRESHOLD);

        assert!(
            throughput_profile_update_interval > 0,
            "throughput_profile_update_interval should be >= 0"
        );

        assert!(
            (0..=30).contains(&throughput_profile_cool_down_threshold),
            "Out of bounds provided cool down threshold offset"
        );

        debug!("Profile ranges used: {:?}", profile_ranges);

        Self {
            throughput_profile_update_interval,
            throughput_profile_cool_down_threshold,
            last_throughput_profile: ArcSwap::from_pointee(ThroughputProfileEntry {
                profile: profile_ranges.highest_profile(),
                timestamp: 0,
                throughput: 0,
            }), // assume high throughput so the node is more conservative on bootstrap
            profile_ranges,
            metrics,
            calculator,
        }
    }

    // Return the current throughput level and the corresponding throughput when this was last updated.
    // If that is not set yet then as default the High profile is returned and the throughput will be None.
    pub fn throughput_level(&self) -> (Level, u64) {
        // Update throughput profile if necessary time has passed
        let (throughput, timestamp) = self.calculator.current_throughput();
        let profile = self.update_and_fetch_throughput_profile(throughput, timestamp);

        (profile.profile.level, profile.throughput)
    }

    // Calculate and update the throughput profile based on the provided throughput. The throughput profile
    // will only get updated when a different value has been calculated. For example, if the
    // `last_throughput_profile` is `Low` , and again we calculate it as `Low` based on input, then we'll
    // not update the profile or the timestamp. We do care to perform updates only when profiles differ.
    // To ensure that we are protected against throughput profile change fluctuations, we update a
    // throughput profile every `throughput_profile_update_interval` seconds based on the provided unix timestamps.
    // The last throughput profile entry is returned.
    fn update_and_fetch_throughput_profile(
        &self,
        throughput: u64,
        timestamp: TimestampSecs,
    ) -> ThroughputProfileEntry {
        let last_profile = self.last_throughput_profile.load();

        // Skip any processing if provided timestamp is older than the last used one. Also return existing
        // profile when provided timestamp is 0 - this avoids triggering an immediate update eventually overriding
        // the default value.
        if timestamp == 0 || timestamp < last_profile.timestamp {
            return **last_profile;
        }

        let profile = self.profile_ranges.resolve(throughput);

        let current_seconds_bucket = timestamp / self.throughput_profile_update_interval;
        let last_profile_seconds_bucket =
            last_profile.timestamp / self.throughput_profile_update_interval;

        // Update only when we minimum time has been passed since last update.
        // We allow the edge case to update on the same bucket when a different profile has been
        // computed for the exact same timestamp.
        let should_update_profile = if current_seconds_bucket > last_profile_seconds_bucket
            || (profile != last_profile.profile && last_profile.timestamp == timestamp)
        {
            if profile < last_profile.profile {
                // If new profile is smaller than previous one, then make sure the cool down threshold is respected.
                let min_throughput = last_profile
                    .profile
                    .throughput
                    .saturating_mul(100 - self.throughput_profile_cool_down_threshold)
                    / 100;
                throughput <= min_throughput
            } else {
                true
            }
        } else {
            false
        };

        if should_update_profile {
            let p = ThroughputProfileEntry {
                profile,
                timestamp,
                throughput,
            };
            debug!("Updating throughput profile to {:?}", p);
            self.last_throughput_profile.store(Arc::new(p));

            self.metrics
                .consensus_calculated_throughput_profile
                .set(usize::from(profile.level) as i64);

            p
        } else {
            **last_profile
        }
    }
}

/// ConsensusThroughputCalculator is calculating the transaction throughput as this is coming out from
/// consensus. The throughput is calculated using a sliding window approach and leveraging the timestamps
/// provided by consensus.
pub struct ConsensusThroughputCalculator {
    /// The number of transaction throughput observations that should be stored within the observations
    /// vector in the ConsensusThroughputCalculatorInner. Those observations will be used to calculate
    /// the current transactions throughput. We want to select a number that give us enough observations
    /// so we better calculate the throughput and protected against spikes. A large enough value though
    /// will make us less reactive to throughput changes.
    observations_window: u64,
    inner: Mutex<ConsensusThroughputCalculatorInner>,
    current_throughput: ArcSwap<(u64, TimestampSecs)>,
    metrics: Arc<AuthorityMetrics>,
}

impl ConsensusThroughputCalculator {
    pub fn new(observations_window: Option<NonZeroU64>, metrics: Arc<AuthorityMetrics>) -> Self {
        let observations_window = observations_window
            .unwrap_or(NonZeroU64::new(DEFAULT_OBSERVATIONS_WINDOW).unwrap())
            .get();

        Self {
            observations_window,
            inner: Mutex::new(ConsensusThroughputCalculatorInner::default()),
            current_throughput: ArcSwap::from_pointee((0, 0)),
            metrics,
        }
    }

    // Adds an observation of the number of transactions that have been sequenced after deduplication
    // and the corresponding leader timestamp. The observation timestamps should be monotonically
    // incremented otherwise observation will be ignored.
    pub fn add_transactions(&self, timestamp_ms: TimestampMs, num_of_transactions: u64) {
        let mut inner = self.inner.lock();
        let timestamp_secs: TimestampSecs = timestamp_ms / 1_000; // lowest bucket we care is seconds

        if let Some((front_ts, transactions)) = inner.observations.front_mut() {
            // First check that the timestamp is monotonically incremented - ignore any observation that is not
            // later from previous one (it shouldn't really happen).
            if timestamp_secs < *front_ts {
                warn!("Ignoring observation of transactions:{} as has earlier timestamp than last observation {}s < {}s", num_of_transactions, timestamp_secs, front_ts);
                return;
            }

            // Not very likely, but if transactions refer to same second we add to the last element.
            if timestamp_secs == *front_ts {
                *transactions = transactions.saturating_add(num_of_transactions);
            } else {
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

        // If we have more values on our window of max values, remove the last one, and calculate throughput.
        // If we have the exact same values on our window of max values, then still calculate the throughput to ensure
        // that we are taking into account the case where the last bucket gets updated because it falls into the same second.
        if inner.observations.len() as u64 >= self.observations_window {
            let last_element_ts = if inner.observations.len() as u64 == self.observations_window {
                if let Some(ts) = inner.last_oldest_timestamp {
                    ts
                } else {
                    warn!("Skip calculation - we still don't have enough elements to pop the last observation");
                    return;
                }
            } else {
                let (ts, txes) = inner.observations.pop_back().unwrap();
                inner.total_transactions = inner.total_transactions.saturating_sub(txes);
                ts
            };

            // update the last oldest timestamp
            inner.last_oldest_timestamp = Some(last_element_ts);

            // get the first element's timestamp to calculate the transaction rate
            let (first_element_ts, _first_element_transactions) = inner
                .observations
                .front()
                .expect("There should be at least on element in the list");

            let period = first_element_ts.saturating_sub(last_element_ts);

            if period > 0 {
                let current_throughput = inner.total_transactions / period;

                self.metrics
                    .consensus_calculated_throughput
                    .set(current_throughput as i64);

                self.current_throughput
                    .store(Arc::new((current_throughput, timestamp_secs)));
            } else {
                warn!("Skip calculating throughput as time period is {}. This is very unlikely to happen, should investigate.", period);
            }
        }
    }

    // Returns the current (live calculated) throughput and the corresponding timestamp of when this got updated.
    pub fn current_throughput(&self) -> (u64, TimestampSecs) {
        *self.current_throughput.load().as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus_throughput_calculator::Level::{High, Low};
    use prometheus::Registry;

    #[test]
    pub fn test_throughput_profile_ranges() {
        let ranges = ThroughputProfileRanges::default();

        assert_eq!(
            ranges.resolve(0),
            ThroughputProfile {
                level: Low,
                throughput: 0
            }
        );
        assert_eq!(
            ranges.resolve(1_000),
            ThroughputProfile {
                level: Low,
                throughput: 0
            }
        );
        assert_eq!(
            ranges.resolve(2_000),
            ThroughputProfile {
                level: High,
                throughput: 2_000
            }
        );
        assert_eq!(
            ranges.resolve(u64::MAX),
            ThroughputProfile {
                level: High,
                throughput: 2_000
            }
        );
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_consensus_throughput_calculator() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let max_observation_points: NonZeroU64 = NonZeroU64::new(3).unwrap();

        let calculator = ConsensusThroughputCalculator::new(Some(max_observation_points), metrics);

        assert_eq!(calculator.current_throughput(), (0, 0));

        calculator.add_transactions(1000 as TimestampMs, 1_000);
        calculator.add_transactions(2000 as TimestampMs, 1_000);
        calculator.add_transactions(3000 as TimestampMs, 1_000);
        calculator.add_transactions(4000 as TimestampMs, 1_000);

        // We expect to have a rate of 1K tx/sec with last update timestamp the 4th second
        assert_eq!(calculator.current_throughput(), (1000, 4));

        // We are adding more transactions to get over 2K tx/sec
        calculator.add_transactions(5_000 as TimestampMs, 2_500);
        calculator.add_transactions(6_000 as TimestampMs, 2_800);
        assert_eq!(calculator.current_throughput(), (2100, 6));

        // Let's now add 0 transactions after 5 seconds. Since 5 seconds have passed since the last
        // update and now the transactions are 0 we expect the throughput to be calculate as:
        // 2800 + 2500 + 0 = 5300 / (15sec - 4sec) = 5300 / 11sec = 481 tx/sec
        calculator.add_transactions(15_000 as TimestampMs, 0);

        assert_eq!(calculator.current_throughput(), (481, 15));

        // Adding zero transactions for the next 5 seconds will make throughput zero
        calculator.add_transactions(17_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), (233, 17));

        calculator.add_transactions(19_000 as TimestampMs, 0);
        calculator.add_transactions(20_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), (0, 20));

        // By adding now a few entries with lots of transactions increase again the throughput
        calculator.add_transactions(21_000 as TimestampMs, 1_000);
        calculator.add_transactions(22_000 as TimestampMs, 2_000);
        calculator.add_transactions(23_000 as TimestampMs, 3_100);
        assert_eq!(calculator.current_throughput(), (2033, 23));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_throughput_calculator_same_timestamp_observations() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let max_observation_points: NonZeroU64 = NonZeroU64::new(2).unwrap();

        let calculator = ConsensusThroughputCalculator::new(Some(max_observation_points), metrics);

        // adding one observation
        calculator.add_transactions(1_000, 0);

        // Adding observations with same timestamp should fall under the same bucket and won't lead
        // to throughput update.
        for _ in 0..10 {
            calculator.add_transactions(2_340, 100);
        }
        assert_eq!(calculator.current_throughput(), (0, 0));

        // Adding now one observation on a different second bucket will change throughput
        calculator.add_transactions(5_000, 0);

        assert_eq!(calculator.current_throughput(), (250, 5));

        // Updating further the last bucket with more transactions it keeps updating the throughput
        calculator.add_transactions(5_000, 400);
        assert_eq!(calculator.current_throughput(), (350, 5));

        calculator.add_transactions(5_000, 300);
        assert_eq!(calculator.current_throughput(), (425, 5));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_consensus_throughput_profiler() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let throughput_profile_update_interval: TimestampSecs = 5;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(3).unwrap();
        let throughput_profile_cool_down_threshold: u64 = 10;

        let ranges = ThroughputProfileRanges::default();

        let calculator = Arc::new(ConsensusThroughputCalculator::new(
            Some(max_observation_points),
            metrics.clone(),
        ));
        let profiler = ConsensusThroughputProfiler::new(
            calculator.clone(),
            Some(throughput_profile_update_interval),
            Some(throughput_profile_cool_down_threshold),
            metrics,
            ranges,
        );

        // When no transactions exists, the calculator will return by default "High" to err on the
        // assumption that there is lots of load.
        assert_eq!(profiler.throughput_level(), (High, 0));

        calculator.add_transactions(1000 as TimestampMs, 1_000);
        calculator.add_transactions(2000 as TimestampMs, 1_000);
        calculator.add_transactions(3000 as TimestampMs, 1_000);

        // We expect to have a rate of 1K tx/sec, that's < 2K limit , so throughput profile remains to "High" - nothing gets updated
        assert_eq!(profiler.throughput_level(), (High, 0));

        // We are adding more transactions to get over 2K tx/sec, so throughput profile should now be categorised
        // as "high"
        calculator.add_transactions(4000 as TimestampMs, 2_500);
        calculator.add_transactions(5000 as TimestampMs, 2_800);
        assert_eq!(profiler.throughput_level(), (High, 2100));

        // Let's now add 0 transactions after at least 5 seconds. Since the update should happen every 5 seconds
        // now the transactions are 0 we expect the throughput to be calculate as:
        // 2800 + 2800 + 0 = 5300 / 15 - 4sec = 5600 / 11sec = 509 tx/sec
        calculator.add_transactions(7_000 as TimestampMs, 2_800);
        calculator.add_transactions(15_000 as TimestampMs, 0);

        assert_eq!(profiler.throughput_level(), (Low, 509));

        // Adding zero transactions for the next 5 seconds will make throughput zero.
        // Profile will remain Low and throughput will get updated
        calculator.add_transactions(17_000 as TimestampMs, 0);
        calculator.add_transactions(19_000 as TimestampMs, 0);
        calculator.add_transactions(20_000 as TimestampMs, 0);

        assert_eq!(profiler.throughput_level(), (Low, 0));

        // By adding a few entries with lots of transactions for the exact same last timestamp it will
        // trigger a throughput profile update.
        calculator.add_transactions(20_000 as TimestampMs, 4_000);
        calculator.add_transactions(20_000 as TimestampMs, 4_000);
        calculator.add_transactions(20_000 as TimestampMs, 4_000);
        assert_eq!(profiler.throughput_level(), (High, 2400));

        // no further updates will happen until the next 5sec bucket update.
        calculator.add_transactions(22_000 as TimestampMs, 0);
        calculator.add_transactions(23_000 as TimestampMs, 0);
        assert_eq!(profiler.throughput_level(), (High, 2400));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_consensus_throughput_profiler_update_interval() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let throughput_profile_update_interval: TimestampSecs = 5;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(2).unwrap();

        let ranges = ThroughputProfileRanges::default();

        let calculator = Arc::new(ConsensusThroughputCalculator::new(
            Some(max_observation_points),
            metrics.clone(),
        ));
        let profiler = ConsensusThroughputProfiler::new(
            calculator.clone(),
            Some(throughput_profile_update_interval),
            None,
            metrics,
            ranges,
        );

        // Current setup is `throughput_profile_update_interval` = 5sec, which means that throughput profile
        // should get updated every 5 seconds (based on the provided unix timestamp).

        calculator.add_transactions(3_000 as TimestampMs, 2_200);
        calculator.add_transactions(4_000 as TimestampMs, 4_200);
        calculator.add_transactions(7_000 as TimestampMs, 4_200);

        assert_eq!(profiler.throughput_level(), (High, 2_100));

        // When adding transactions at timestamp 10s the bucket changes and the profile should get updated
        calculator.add_transactions(10_000 as TimestampMs, 1_000);

        assert_eq!(profiler.throughput_level(), (Low, 866));

        // Now adding transactions at timestamp 16s the bucket changes and profile should get updated
        calculator.add_transactions(16_000 as TimestampMs, 20_000);

        assert_eq!(profiler.throughput_level(), (High, 2333));

        // Keep adding transactions that fall under the same timestamp as the previous one, even though
        // traffic should be marked as low it doesn't until the bucket of 20s is updated.
        calculator.add_transactions(17_000 as TimestampMs, 0);
        calculator.add_transactions(18_000 as TimestampMs, 0);
        calculator.add_transactions(19_000 as TimestampMs, 0);

        assert_eq!(profiler.throughput_level(), (High, 2333));

        calculator.add_transactions(20_000 as TimestampMs, 0);

        assert_eq!(profiler.throughput_level(), (Low, 0));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_consensus_throughput_profiler_cool_down() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let throughput_profile_update_window: TimestampSecs = 3;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(3).unwrap();
        let throughput_profile_cool_down_threshold: u64 = 10;

        let ranges = ThroughputProfileRanges::default();

        let calculator = Arc::new(ConsensusThroughputCalculator::new(
            Some(max_observation_points),
            metrics.clone(),
        ));
        let profiler = ConsensusThroughputProfiler::new(
            calculator.clone(),
            Some(throughput_profile_update_window),
            Some(throughput_profile_cool_down_threshold),
            metrics,
            ranges,
        );

        // Adding 4 observations of 3_000 tx/sec, so in the end throughput profile should be flagged as high
        for i in 1..=4 {
            calculator.add_transactions(i * 1_000, 3_000);
        }
        assert_eq!(profiler.throughput_level(), (High, 3_000));

        // Now let's add some transactions to bring throughput little bit bellow the upper Low threshold (2000 tx/sec)
        // but still above the 10% offset which is 1800 tx/sec.
        calculator.add_transactions(5_000, 1_900);
        calculator.add_transactions(6_000, 1_900);
        calculator.add_transactions(7_000, 1_900);

        assert_eq!(calculator.current_throughput(), (1_900, 7));
        assert_eq!(profiler.throughput_level(), (High, 3_000));

        // Let's bring down more throughput - now the throughput profile should get updated
        calculator.add_transactions(8_000, 1_500);
        calculator.add_transactions(9_000, 1_500);
        calculator.add_transactions(10_000, 1_500);

        assert_eq!(profiler.throughput_level(), (Low, 1500));
    }
}

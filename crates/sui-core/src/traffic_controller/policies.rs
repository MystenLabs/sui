// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, net::IpAddr, sync::Arc};

use count_min_sketch::CountMinSketch32;
use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;
use std::time::{Instant, SystemTime};
use sui_types::traffic_control::{FreqThresholdConfig, PolicyConfig, PolicyType, Weight};
use tracing::{info, trace};

const HIGHEST_RATES_CAPACITY: usize = 20;

/// The type of request client.
#[derive(Hash, Eq, PartialEq, Debug)]
enum ClientType {
    Direct,
    ThroughFullnode,
}

#[derive(Hash, Eq, PartialEq, Debug)]
struct SketchKey {
    salt: u64,
    ip_addr: IpAddr,
    client_type: ClientType,
}

struct HighestRates {
    direct: BinaryHeap<Reverse<(u64, IpAddr)>>,
    proxied: BinaryHeap<Reverse<(u64, IpAddr)>>,
    capacity: usize,
}

pub struct TrafficSketch {
    /// Circular buffer Count Min Sketches representing a sliding window
    /// of traffic data. Note that the 32 in CountMinSketch32 represents
    /// the number of bits used to represent the count in the sketch. Since
    /// we only count on a sketch for a window of `update_interval`, we only
    /// need enough precision to represent the max expected unique IP addresses
    /// we may see in that window. For a 10 second period, we might conservatively
    /// expect 100,000, which can be represented in 17 bits, but not 16. We can
    /// potentially lower the memory consumption by using CountMinSketch16, which
    /// will reliably support up to ~65,000 unique IP addresses in the window.
    sketches: VecDeque<CountMinSketch32<SketchKey>>,
    window_size: Duration,
    update_interval: Duration,
    last_reset_time: Instant,
    current_sketch_index: usize,
    /// Used for metrics collection and logging purposes,
    /// as CountMinSketch does not provide this directly.
    /// Note that this is an imperfect metric, since we preserve
    /// the highest N rates (by unique IP) that we have seen,
    /// but update rates (down or up) as they change so that
    /// the metric is not monotonic and reflects recent traffic.
    /// However, this should only lead to inaccuracy edge cases
    /// with very low traffic.
    highest_rates: HighestRates,
}

impl TrafficSketch {
    pub fn new(
        window_size: Duration,
        update_interval: Duration,
        sketch_capacity: usize,
        sketch_probability: f64,
        sketch_tolerance: f64,
        highest_rates_capacity: usize,
    ) -> Self {
        // intentionally round down via integer division. We can't have a partial sketch
        let num_sketches = window_size.as_secs() / update_interval.as_secs();
        let new_window_size = Duration::from_secs(num_sketches * update_interval.as_secs());
        if new_window_size != window_size {
            info!(
                "Rounding traffic sketch window size down to {} seconds to make it an integer multiple of update interval {} seconds.",
                new_window_size.as_secs(),
                update_interval.as_secs(),
            );
        }
        let window_size = new_window_size;

        assert!(
            window_size < Duration::from_secs(600),
            "window_size too large. Max 600 seconds"
        );
        assert!(
            update_interval < window_size,
            "Update interval may not be larger than window size"
        );
        assert!(
            update_interval >= Duration::from_secs(1),
            "Update interval too short, must be at least 1 second"
        );
        assert!(num_sketches <= 10, "Given parameters require too many sketches to be stored. Reduce window size or increase update interval.");
        let mem_estimate = (num_sketches as usize)
            * CountMinSketch32::<IpAddr>::estimate_memory(
                sketch_capacity,
                sketch_probability,
                sketch_tolerance,
            )
            .expect("Failed to estimate memory for CountMinSketch32");
        assert!(mem_estimate < 128_000_000, "Memory estimate for traffic sketch exceeds 128MB. Reduce window size or increase update interval.");

        let mut sketches = VecDeque::with_capacity(num_sketches as usize);
        for _ in 0..num_sketches {
            sketches.push_back(
                CountMinSketch32::<SketchKey>::new(
                    sketch_capacity,
                    sketch_probability,
                    sketch_tolerance,
                )
                .expect("Failed to create CountMinSketch32"),
            );
        }
        Self {
            sketches,
            window_size,
            update_interval,
            last_reset_time: Instant::now(),
            current_sketch_index: 0,
            highest_rates: HighestRates {
                direct: BinaryHeap::with_capacity(highest_rates_capacity),
                proxied: BinaryHeap::with_capacity(highest_rates_capacity),
                capacity: highest_rates_capacity,
            },
        }
    }

    fn increment_count(&mut self, key: &SketchKey) {
        // reset all expired intervals
        let current_time = Instant::now();
        let mut elapsed = current_time.duration_since(self.last_reset_time);
        while elapsed >= self.update_interval {
            self.rotate_window();
            elapsed -= self.update_interval;
        }
        // Increment in the current active sketch
        self.sketches[self.current_sketch_index].increment(key);
    }

    fn get_request_rate(&mut self, key: &SketchKey) -> f64 {
        let count: u32 = self
            .sketches
            .iter()
            .map(|sketch| sketch.estimate(key))
            .sum();
        let rate = count as f64 / self.window_size.as_secs() as f64;
        self.update_highest_rates(key, rate);
        rate
    }

    fn update_highest_rates(&mut self, key: &SketchKey, rate: f64) {
        match key.client_type {
            ClientType::Direct => {
                Self::update_highest_rate(
                    &mut self.highest_rates.direct,
                    key.ip_addr,
                    rate,
                    self.highest_rates.capacity,
                );
            }
            ClientType::ThroughFullnode => {
                Self::update_highest_rate(
                    &mut self.highest_rates.proxied,
                    key.ip_addr,
                    rate,
                    self.highest_rates.capacity,
                );
            }
        }
    }

    fn update_highest_rate(
        rate_heap: &mut BinaryHeap<Reverse<(u64, IpAddr)>>,
        ip_addr: IpAddr,
        rate: f64,
        capacity: usize,
    ) {
        // Remove previous instance of this IPAddr so that we
        // can update with new rate
        rate_heap.retain(|&Reverse((_, key))| key != ip_addr);

        let rate = rate as u64;
        if rate_heap.len() < capacity {
            rate_heap.push(Reverse((rate, ip_addr)));
        } else if let Some(&Reverse((smallest_score, _))) = rate_heap.peek() {
            if rate > smallest_score {
                rate_heap.pop();
                rate_heap.push(Reverse((rate, ip_addr)));
            }
        }
    }

    pub fn highest_direct_rate(&self) -> Option<(u64, IpAddr)> {
        self.highest_rates
            .direct
            .iter()
            .map(|Reverse(v)| v)
            .max_by(|a, b| a.0.partial_cmp(&b.0).expect("Failed to compare rates"))
            .copied()
    }

    pub fn highest_proxied_rate(&self) -> Option<(u64, IpAddr)> {
        self.highest_rates
            .proxied
            .iter()
            .map(|Reverse(v)| v)
            .max_by(|a, b| a.0.partial_cmp(&b.0).expect("Failed to compare rates"))
            .copied()
    }

    fn rotate_window(&mut self) {
        self.current_sketch_index = (self.current_sketch_index + 1) % self.sketches.len();
        self.sketches[self.current_sketch_index].clear();
        self.last_reset_time = Instant::now();
    }
}

#[derive(Clone, Debug)]
pub struct TrafficTally {
    pub direct: Option<IpAddr>,
    pub through_fullnode: Option<IpAddr>,
    pub error_info: Option<(Weight, String)>,
    pub spam_weight: Weight,
    pub timestamp: SystemTime,
}

impl TrafficTally {
    pub fn new(
        direct: Option<IpAddr>,
        through_fullnode: Option<IpAddr>,
        error_info: Option<(Weight, String)>,
        spam_weight: Weight,
    ) -> Self {
        Self {
            direct,
            through_fullnode,
            error_info,
            spam_weight,
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PolicyResponse {
    pub block_client: Option<IpAddr>,
    pub block_proxied_client: Option<IpAddr>,
}

pub trait Policy {
    // returns, e.g. (true, false) if connection_ip should be added to blocklist
    // and proxy_ip should not
    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse;
    fn policy_config(&self) -> &PolicyConfig;
}

// Nonserializable representation, also note that inner types are
// not object safe, so we can't use a trait object instead
pub enum TrafficControlPolicy {
    FreqThreshold(FreqThresholdPolicy),
    NoOp(NoOpPolicy),
    // Test policies below this point
    TestNConnIP(TestNConnIPPolicy),
    TestPanicOnInvocation(TestPanicOnInvocationPolicy),
}

impl Policy for TrafficControlPolicy {
    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::FreqThreshold(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestNConnIP(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestPanicOnInvocation(policy) => policy.handle_tally(tally),
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.policy_config(),
            TrafficControlPolicy::FreqThreshold(policy) => policy.policy_config(),
            TrafficControlPolicy::TestNConnIP(policy) => policy.policy_config(),
            TrafficControlPolicy::TestPanicOnInvocation(policy) => policy.policy_config(),
        }
    }
}

impl TrafficControlPolicy {
    pub async fn from_spam_config(policy_config: PolicyConfig) -> Self {
        Self::from_config(policy_config.clone().spam_policy_type, policy_config).await
    }
    pub async fn from_error_config(policy_config: PolicyConfig) -> Self {
        Self::from_config(policy_config.clone().error_policy_type, policy_config).await
    }
    pub async fn from_config(policy_type: PolicyType, policy_config: PolicyConfig) -> Self {
        match policy_type {
            PolicyType::NoOp => Self::NoOp(NoOpPolicy::new(policy_config)),
            PolicyType::FreqThreshold(freq_threshold_config) => Self::FreqThreshold(
                FreqThresholdPolicy::new(policy_config, freq_threshold_config),
            ),
            PolicyType::TestNConnIP(n) => {
                Self::TestNConnIP(TestNConnIPPolicy::new(policy_config, n).await)
            }
            PolicyType::TestPanicOnInvocation => {
                Self::TestPanicOnInvocation(TestPanicOnInvocationPolicy::new(policy_config))
            }
        }
    }
}

////////////// *** Policy definitions *** //////////////

pub struct FreqThresholdPolicy {
    config: PolicyConfig,
    sketch: TrafficSketch,
    client_threshold: u64,
    proxied_client_threshold: u64,
    /// Unique salt to be added to all keys in the sketch. This
    /// ensures that false positives are not correlated across
    /// all nodes at the same time. For Sui validators for example,
    /// this means that false positives should not prevent the network
    /// from achieving quorum.
    salt: u64,
}

impl FreqThresholdPolicy {
    pub fn new(
        config: PolicyConfig,
        FreqThresholdConfig {
            client_threshold,
            proxied_client_threshold,
            window_size_secs,
            update_interval_secs,
            sketch_capacity,
            sketch_probability,
            sketch_tolerance,
        }: FreqThresholdConfig,
    ) -> Self {
        let sketch = TrafficSketch::new(
            Duration::from_secs(window_size_secs),
            Duration::from_secs(update_interval_secs),
            sketch_capacity,
            sketch_probability,
            sketch_tolerance,
            HIGHEST_RATES_CAPACITY,
        );
        Self {
            config,
            sketch,
            client_threshold,
            proxied_client_threshold,
            salt: rand::random(),
        }
    }

    pub fn highest_direct_rate(&self) -> Option<(u64, IpAddr)> {
        self.sketch.highest_direct_rate()
    }

    pub fn highest_proxied_rate(&self) -> Option<(u64, IpAddr)> {
        self.sketch.highest_proxied_rate()
    }

    pub fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        let block_client = if let Some(source) = tally.direct {
            let key = SketchKey {
                salt: self.salt,
                ip_addr: source,
                client_type: ClientType::Direct,
            };
            self.sketch.increment_count(&key);
            let req_rate = self.sketch.get_request_rate(&key);
            trace!(
                "FreqThresholdPolicy handling tally -- req_rate: {:?}, client_threshold: {:?}, client: {:?}",
                req_rate,
                self.client_threshold,
                source,
            );
            if req_rate >= self.client_threshold as f64 {
                Some(source)
            } else {
                None
            }
        } else {
            None
        };
        let block_proxied_client = if let Some(source) = tally.through_fullnode {
            let key = SketchKey {
                salt: self.salt,
                ip_addr: source,
                client_type: ClientType::ThroughFullnode,
            };
            self.sketch.increment_count(&key);
            if self.sketch.get_request_rate(&key) >= self.proxied_client_threshold as f64 {
                Some(source)
            } else {
                None
            }
        } else {
            None
        };
        PolicyResponse {
            block_client,
            block_proxied_client,
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
    }
}

////////////// *** Test policies below this point *** //////////////

#[derive(Clone)]
pub struct NoOpPolicy {
    config: PolicyConfig,
}

impl NoOpPolicy {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    fn handle_tally(&mut self, _tally: TrafficTally) -> PolicyResponse {
        PolicyResponse::default()
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
    }
}

#[derive(Clone)]
pub struct TestNConnIPPolicy {
    config: PolicyConfig,
    frequencies: Arc<RwLock<HashMap<IpAddr, u64>>>,
    threshold: u64,
}

impl TestNConnIPPolicy {
    pub async fn new(config: PolicyConfig, threshold: u64) -> Self {
        let frequencies = Arc::new(RwLock::new(HashMap::new()));
        let frequencies_clone = frequencies.clone();
        spawn_monitored_task!(run_clear_frequencies(
            frequencies_clone,
            config.connection_blocklist_ttl_sec * 2,
        ));
        Self {
            config,
            frequencies,
            threshold,
        }
    }

    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        let client = if let Some(client) = tally.direct {
            client
        } else {
            return PolicyResponse::default();
        };

        // increment the count for the IP
        let mut frequencies = self.frequencies.write();
        let count = frequencies.entry(client).or_insert(0);
        *count += 1;
        PolicyResponse {
            block_client: if *count >= self.threshold {
                Some(client)
            } else {
                None
            },
            block_proxied_client: None,
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
    }
}

async fn run_clear_frequencies(frequencies: Arc<RwLock<HashMap<IpAddr, u64>>>, window_secs: u64) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(window_secs)).await;
        frequencies.write().clear();
    }
}

#[derive(Clone)]
pub struct TestPanicOnInvocationPolicy {
    config: PolicyConfig,
}

impl TestPanicOnInvocationPolicy {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    fn handle_tally(&mut self, _: TrafficTally) -> PolicyResponse {
        panic!("Tally for this policy should never be invoked")
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use sui_macros::sim_test;
    use sui_types::traffic_control::{
        DEFAULT_SKETCH_CAPACITY, DEFAULT_SKETCH_PROBABILITY, DEFAULT_SKETCH_TOLERANCE,
    };

    #[sim_test]
    async fn test_freq_threshold_policy() {
        // Create freq policy that will block on average frequency 2 requests per second
        // for proxied connections and 4 requests per second for direct connections
        // as observed over a 5 second window.
        let mut policy = FreqThresholdPolicy::new(
            PolicyConfig::default(),
            FreqThresholdConfig {
                client_threshold: 5,
                proxied_client_threshold: 2,
                window_size_secs: 5,
                update_interval_secs: 1,
                ..Default::default()
            },
        );
        // alice and bob connection from different IPs through the
        // same fullnode, thus have the same connection IP on
        // validator, but different proxy IPs
        let alice = TrafficTally {
            direct: Some(IpAddr::V4(Ipv4Addr::new(8, 7, 6, 5))),
            through_fullnode: Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))),
            error_info: None,
            spam_weight: Weight::one(),
            timestamp: SystemTime::now(),
        };
        let bob = TrafficTally {
            direct: Some(IpAddr::V4(Ipv4Addr::new(8, 7, 6, 5))),
            through_fullnode: Some(IpAddr::V4(Ipv4Addr::new(4, 3, 2, 1))),
            error_info: None,
            spam_weight: Weight::one(),
            timestamp: SystemTime::now(),
        };
        let charlie = TrafficTally {
            direct: Some(IpAddr::V4(Ipv4Addr::new(8, 7, 6, 5))),
            through_fullnode: Some(IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8))),
            error_info: None,
            spam_weight: Weight::one(),
            timestamp: SystemTime::now(),
        };

        // initial 2 tallies for alice, should not block
        for _ in 0..2 {
            let response = policy.handle_tally(alice.clone());
            assert_eq!(response.block_proxied_client, None);
            assert_eq!(response.block_client, None);
        }

        let (direct_rate, direct_ip_addr) = policy.highest_direct_rate().unwrap();
        let (proxied_rate, proxied_ip_addr) = policy.highest_proxied_rate().unwrap();
        assert_eq!(direct_ip_addr, alice.direct.unwrap());
        assert!(direct_rate < 1);
        assert_eq!(proxied_ip_addr, alice.through_fullnode.unwrap());
        assert!(proxied_rate < 1);

        // meanwhile bob spams 10 requests at once and is blocked
        for _ in 0..9 {
            let response = policy.handle_tally(bob.clone());
            assert_eq!(response.block_client, None);
            assert_eq!(response.block_proxied_client, None);
        }
        let response = policy.handle_tally(bob.clone());
        assert_eq!(response.block_client, None);
        assert_eq!(response.block_proxied_client, bob.through_fullnode);

        // highest rates should now show bob
        let (direct_rate, direct_ip_addr) = policy.highest_direct_rate().unwrap();
        let (proxied_rate, proxied_ip_addr) = policy.highest_proxied_rate().unwrap();
        assert_eq!(direct_ip_addr, bob.direct.unwrap());
        assert_eq!(direct_rate, 2);
        assert_eq!(proxied_ip_addr, bob.through_fullnode.unwrap());
        assert_eq!(proxied_rate, 2);

        // 2 more tallies, so far we are above 2 tallies
        // per second, but over the average window of 5 seconds
        // we are still below the threshold. Should not block
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        for _ in 0..2 {
            let response = policy.handle_tally(alice.clone());
            assert_eq!(response.block_client, None);
            assert_eq!(response.block_proxied_client, None);
        }
        // bob is no longer blocked, as we moved past the bursty traffic
        // in the sliding window
        let _ = policy.handle_tally(bob.clone());
        let response = policy.handle_tally(bob.clone());
        assert_eq!(response.block_client, None);
        assert_eq!(response.block_proxied_client, bob.through_fullnode);

        let (direct_rate, direct_ip_addr) = policy.highest_direct_rate().unwrap();
        let (proxied_rate, proxied_ip_addr) = policy.highest_proxied_rate().unwrap();
        // direct rate increased due to alice going through same fullnode
        assert_eq!(direct_ip_addr, alice.direct.unwrap());
        assert_eq!(direct_rate, 3);
        // highest rate should now have been updated given that Bob's rate
        // recently decreased
        assert_eq!(proxied_ip_addr, bob.through_fullnode.unwrap());
        assert_eq!(proxied_rate, 2);

        // close to threshold for alice, but still below
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        for i in 0..5 {
            let response = policy.handle_tally(alice.clone());
            assert_eq!(response.block_client, None, "Blocked at i = {}", i);
            assert_eq!(response.block_proxied_client, None);
        }

        // should block alice now
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let response = policy.handle_tally(alice.clone());
        assert_eq!(response.block_client, None);
        assert_eq!(response.block_proxied_client, alice.through_fullnode);

        let (direct_rate, direct_ip_addr) = policy.highest_direct_rate().unwrap();
        let (proxied_rate, proxied_ip_addr) = policy.highest_proxied_rate().unwrap();
        assert_eq!(direct_ip_addr, alice.direct.unwrap());
        assert_eq!(direct_rate, 4);
        assert_eq!(proxied_ip_addr, bob.through_fullnode.unwrap());
        assert_eq!(proxied_rate, 2);

        // spam through charlie to block connection
        for i in 0..2 {
            let response = policy.handle_tally(charlie.clone());
            assert_eq!(response.block_client, None, "Blocked at i = {}", i);
            assert_eq!(response.block_proxied_client, None);
        }
        // Now we block connection IP
        let response = policy.handle_tally(charlie.clone());
        assert_eq!(response.block_proxied_client, None);
        assert_eq!(response.block_client, charlie.direct);

        // Ensure that if we wait another second, we are no longer blocked
        // as the bursty first second has finally rotated out of the sliding
        // window
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        for i in 0..3 {
            let response = policy.handle_tally(charlie.clone());
            assert_eq!(response.block_client, None, "Blocked at i = {}", i);
            assert_eq!(response.block_proxied_client, None);
        }

        // check that we revert back to previous highest rates when rates decrease
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        // alice and bob rates are now decreased after 5 seconds of break, but charlie's
        // has not since they have not yet sent a new request
        let _ = policy.handle_tally(alice.clone());
        let _ = policy.handle_tally(bob.clone());
        let (direct_rate, direct_ip_addr) = policy.highest_direct_rate().unwrap();
        let (proxied_rate, proxied_ip_addr) = policy.highest_proxied_rate().unwrap();
        assert_eq!(direct_ip_addr, alice.direct.unwrap());
        assert_eq!(direct_rate, 0);
        assert_eq!(proxied_ip_addr, charlie.through_fullnode.unwrap());
        assert_eq!(proxied_rate, 1);
    }

    #[sim_test]
    async fn test_traffic_sketch_mem_estimate() {
        // Test for getting a rough estimate of memory usage for the traffic sketch
        // given certain parameters. Parameters below are the default.
        // With default parameters, memory estimate is 113 MB.
        let window_size = Duration::from_secs(30);
        let update_interval = Duration::from_secs(5);
        let mem_estimate = CountMinSketch32::<IpAddr>::estimate_memory(
            DEFAULT_SKETCH_CAPACITY,
            DEFAULT_SKETCH_PROBABILITY,
            DEFAULT_SKETCH_TOLERANCE,
        )
        .unwrap()
            * (window_size.as_secs() / update_interval.as_secs()) as usize;
        assert!(
            mem_estimate < 128_000_000,
            "Memory estimate {mem_estimate} for traffic sketch exceeds 128MB."
        );
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::path::PathBuf;

// These values set to loosely attempt to limit
// memory usage for a single sketch to ~20MB
// For reference, see
// https://github.com/jedisct1/rust-count-min-sketch/blob/master/src/lib.rs
pub const DEFAULT_SKETCH_CAPACITY: usize = 50_000;
pub const DEFAULT_SKETCH_PROBABILITY: f64 = 0.999;
pub const DEFAULT_SKETCH_TOLERANCE: f64 = 0.2;
use rand::distributions::Distribution;

const TRAFFIC_SINK_TIMEOUT_SEC: u64 = 300;

/// The source that should be used to identify the client's
/// IP address. To be used to configure cases where a node has
/// infra running in front of the node that is separate from the
/// protocol, such as a load balancer. Note that this is not the
/// same as the client type (e.g a direct client vs a proxy client,
/// as in the case of a fullnode driving requests from many clients).
///
/// For x-forwarded-for, the usize parameter is the number of forwarding
/// hops between the client and the node for requests going your infra
/// or infra provider. Example:
///
/// ```ignore
///     (client) -> { (global proxy) -> (regional proxy) -> (node) }
/// ```
///
/// where
///
/// ```ignore
///     { <server>, ... }
/// ```
///
/// are controlled by the Node operator / their cloud provider.
/// In this case, we set:
///
/// ```ignore
/// policy-config:
///    client-id-source:
///      x-forwarded-for: 2
///    ...
/// ```
///
/// NOTE: x-forwarded-for: 0 is a special case value that can be used by Node
/// operators to discover the number of hops that should be configured. To use:
///
/// 1. Set `x-forwarded-for: 0` for the `client-id-source` in the config.
/// 2. Run the node and query any endpoint (AuthorityServer for validator, or json rpc for rpc node)
///     from a known IP address.
/// 3. Search for lines containing `x-forwarded-for` in the logs. The log lines should contain
///    the contents of the `x-forwarded-for` header, if present, or a corresponding error if not.
/// 4. The value for number of hops is derived from any such log line that contains your known IP
///     address, and is defined as 1 + the number of IP addresses in the `x-forwarded-for` that occur
///     **after** the known client IP address. Example:
///
/// ```ignore
///     [<known client IP>] <--- number of hops is 1
///     ["1.2.3.4", <known client IP>, "5.6.7.8", "9.10.11.12"] <--- number of hops is 3
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ClientIdSource {
    #[default]
    SocketAddr,
    XForwardedFor(usize),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Weight(f32);

impl Weight {
    pub fn new(value: f32) -> Result<Self, &'static str> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err("Weight must be between 0.0 and 1.0")
        }
    }

    pub fn one() -> Self {
        Self(1.0)
    }

    pub fn zero() -> Self {
        Self(0.0)
    }

    pub fn value(&self) -> f32 {
        self.0
    }

    pub fn is_sampled(&self) -> bool {
        let mut rng = rand::thread_rng();
        let sample = rand::distributions::Uniform::new(0.0, 1.0).sample(&mut rng);
        sample <= self.value()
    }
}

impl PartialEq for Weight {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteFirewallConfig {
    pub remote_fw_url: String,
    pub destination_port: u16,
    #[serde(default)]
    pub delegate_spam_blocking: bool,
    #[serde(default)]
    pub delegate_error_blocking: bool,
    #[serde(default = "default_drain_path")]
    pub drain_path: PathBuf,
    /// Time in secs, after which no registered ingress traffic
    /// will trigger dead mans switch to drain any firewalls
    #[serde(default = "default_drain_timeout")]
    pub drain_timeout_secs: u64,
}

fn default_drain_path() -> PathBuf {
    PathBuf::from("/tmp/drain")
}

fn default_drain_timeout() -> u64 {
    TRAFFIC_SINK_TIMEOUT_SEC
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FreqThresholdConfig {
    #[serde(default = "default_client_threshold")]
    pub client_threshold: u64,
    #[serde(default = "default_proxied_client_threshold")]
    pub proxied_client_threshold: u64,
    #[serde(default = "default_window_size_secs")]
    pub window_size_secs: u64,
    #[serde(default = "default_update_interval_secs")]
    pub update_interval_secs: u64,
    #[serde(default = "default_sketch_capacity")]
    pub sketch_capacity: usize,
    #[serde(default = "default_sketch_probability")]
    pub sketch_probability: f64,
    #[serde(default = "default_sketch_tolerance")]
    pub sketch_tolerance: f64,
}

impl Default for FreqThresholdConfig {
    fn default() -> Self {
        Self {
            client_threshold: default_client_threshold(),
            proxied_client_threshold: default_proxied_client_threshold(),
            window_size_secs: default_window_size_secs(),
            update_interval_secs: default_update_interval_secs(),
            sketch_capacity: default_sketch_capacity(),
            sketch_probability: default_sketch_probability(),
            sketch_tolerance: default_sketch_tolerance(),
        }
    }
}

fn default_client_threshold() -> u64 {
    // by default only block client with unreasonably
    // high qps, as a client could be a single fullnode proxying
    // the majority of traffic from many behaving clients in normal
    // operations. If used as a spam policy, all requests would
    // count against this threshold within the window time. In
    // practice this should always be set
    1_000_000
}

fn default_proxied_client_threshold() -> u64 {
    10
}

fn default_window_size_secs() -> u64 {
    30
}

fn default_update_interval_secs() -> u64 {
    5
}

fn default_sketch_capacity() -> usize {
    DEFAULT_SKETCH_CAPACITY
}

fn default_sketch_probability() -> f64 {
    DEFAULT_SKETCH_PROBABILITY
}

fn default_sketch_tolerance() -> f64 {
    DEFAULT_SKETCH_TOLERANCE
}

// Serializable representation of policy types, used in config
// in order to easily change in tests or to killswitch
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub enum PolicyType {
    /// Does nothing
    #[default]
    NoOp,

    /// Blocks connection_ip after reaching a tally frequency (tallies per second)
    /// of `threshold`, as calculated over an average window of `window_size_secs`
    /// with granularity of `update_interval_secs`
    FreqThreshold(FreqThresholdConfig),

    /* Below this point are test policies, and thus should not be used in production */
    ///
    /// Simple policy that adds connection_ip to blocklist when the same connection_ip
    /// is encountered in tally N times. If used in an error policy, this would trigger
    /// after N errors
    TestNConnIP(u64),
    /// Test policy that panics when invoked. To be used as an error policy in tests that do
    /// not expect request errors in order to verify that the error policy is not invoked
    TestPanicOnInvocation,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyConfig {
    #[serde(default = "default_client_id_source")]
    pub client_id_source: ClientIdSource,
    #[serde(default = "default_connection_blocklist_ttl_sec")]
    pub connection_blocklist_ttl_sec: u64,
    #[serde(default)]
    pub proxy_blocklist_ttl_sec: u64,
    #[serde(default)]
    pub spam_policy_type: PolicyType,
    #[serde(default)]
    pub error_policy_type: PolicyType,
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,
    #[serde(default = "default_spam_sample_rate")]
    /// Note that this sample policy is applied on top of the
    /// endpoint-specific sample policy (not configurable) which
    /// weighs endpoints by the relative effort required to serve
    /// them. Therefore a sample rate of N will yield an actual
    /// sample rate <= N.
    pub spam_sample_rate: Weight,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    /// List of String which should all parse to type IPAddr.
    /// If set, only requests from provided IPs will be allowed,
    /// and any blocklist related configuration will be ignored.
    #[serde(default)]
    pub allow_list: Option<Vec<String>>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            client_id_source: default_client_id_source(),
            connection_blocklist_ttl_sec: 0,
            proxy_blocklist_ttl_sec: 0,
            spam_policy_type: PolicyType::NoOp,
            error_policy_type: PolicyType::NoOp,
            channel_capacity: 100,
            spam_sample_rate: default_spam_sample_rate(),
            dry_run: default_dry_run(),
            allow_list: None,
        }
    }
}

pub fn default_client_id_source() -> ClientIdSource {
    ClientIdSource::SocketAddr
}

pub fn default_connection_blocklist_ttl_sec() -> u64 {
    60
}
pub fn default_channel_capacity() -> usize {
    100
}

pub fn default_dry_run() -> bool {
    true
}

pub fn default_spam_sample_rate() -> Weight {
    Weight::new(0.2).unwrap()
}

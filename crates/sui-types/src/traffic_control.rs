// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::SuiResult;
use core::hash::Hash;
use jsonrpsee::core::server::helpers::MethodResponse;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{fmt::Debug, path::PathBuf};

// These values set to loosely attempt to limit
// memory usage for a single sketch to ~20MB
// For reference, see
// https://github.com/jedisct1/rust-count-min-sketch/blob/master/src/lib.rs
pub const DEFAULT_SKETCH_CAPACITY: usize = 50_000;
pub const DEFAULT_SKETCH_PROBABILITY: f64 = 0.999;
pub const DEFAULT_SKETCH_TOLERANCE: f64 = 0.2;

const TRAFFIC_SINK_TIMEOUT_SEC: u64 = 300;

#[derive(Clone, Debug)]
pub enum ServiceResponse {
    Validator(SuiResult),
    Fullnode(MethodResponse),
}

impl PartialEq for ServiceResponse {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ServiceResponse::Validator(a), ServiceResponse::Validator(b)) => a == b,
            (ServiceResponse::Fullnode(a), ServiceResponse::Fullnode(b)) => {
                a.error_code == b.error_code && a.success == b.success
            }
            _ => false,
        }
    }
}

impl Eq for ServiceResponse {}

impl Hash for ServiceResponse {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ServiceResponse::Validator(result) => result.hash(state),
            ServiceResponse::Fullnode(response) => {
                response.error_code.hash(state);
                response.success.hash(state);
            }
        }
    }
}

impl ServiceResponse {
    pub fn is_ok(&self) -> bool {
        match self {
            ServiceResponse::Validator(result) => result.is_ok(),
            ServiceResponse::Fullnode(response) => response.success,
        }
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
    #[serde(default = "default_threshold")]
    pub threshold: u64,
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
            threshold: default_threshold(),
            window_size_secs: default_window_size_secs(),
            update_interval_secs: default_update_interval_secs(),
            sketch_capacity: default_sketch_capacity(),
            sketch_probability: default_sketch_probability(),
            sketch_tolerance: default_sketch_tolerance(),
        }
    }
}

fn default_threshold() -> u64 {
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
    /// Test policy that inspects the proxy_ip and connection_ip to ensure they are present
    /// in the tally. Tests IP forwarding. To be used only in tests that submit transactions
    /// through a client
    TestInspectIp,
    /// Test policy that panics when invoked. To be used as an error policy in tests that do
    /// not expect request errors in order to verify that the error policy is not invoked
    TestPanicOnInvocation,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyConfig {
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
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            connection_blocklist_ttl_sec: 0,
            proxy_blocklist_ttl_sec: 0,
            spam_policy_type: PolicyType::NoOp,
            error_policy_type: PolicyType::NoOp,
            channel_capacity: 100,
            dry_run: default_dry_run(),
        }
    }
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

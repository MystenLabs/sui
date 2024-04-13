// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, net::SocketAddr};

use crate::error::{SuiError, SuiResult};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::fmt::Debug;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct TrafficTally {
    pub connection_ip: Option<SocketAddr>,
    pub proxy_ip: Option<SocketAddr>,
    pub result: SuiResult,
    pub timestamp: SystemTime,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct RemoteFirewallConfig {
    pub remote_fw_url: String,
    pub delegate_spam_blocking: bool,
    pub delegate_error_blocking: bool,
    pub destination_port: u16,
}

// Serializable representation of policy types, used in config
// in order to easily change in tests or to killswitch
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub enum PolicyType {
    /// Does nothing
    #[default]
    NoOp,

    /* Below this point are test policies, and thus should not be used in production */
    ///
    /// Simple policy that adds connection_ip to blocklist when the same connection_ip
    /// is encountered 3 times
    Test3ConnIP,
    /// Test policy that inspects the proxy_ip and connection_ip to ensure they are present
    /// in the tally. Tests IP forwarding. To be used only in tests that submit transactions
    /// through a client
    TestInspectIp,
    /// Test policy that panics when invoked. To be used as an error policy in tests that do
    /// not expect request errors in order to verify that the error policy is not invoked
    TestPanicOnInvocation,
}

#[derive(Clone, Debug, Default)]
pub struct PolicyResponse {
    pub block_connection_ip: Option<SocketAddr>,
    pub block_proxy_ip: Option<SocketAddr>,
}

pub trait Policy {
    // returns, e.g. (true, false) if connection_ip should be added to blocklist
    // and proxy_ip should not
    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse;
    fn policy_config(&self) -> &PolicyConfig;
}

// Nonserializable representation, also note that inner types are
// not object safe, so we can't use a trait object instead
#[derive(Clone)]
pub enum TrafficControlPolicy {
    NoOp(NoOpPolicy),
    Test3ConnIP(Test3ConnIPPolicy),
    TestInspectIp(TestInspectIpPolicy),
    TestPanicOnInvocation(TestPanicOnInvocationPolicy),
}

impl Policy for TrafficControlPolicy {
    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::Test3ConnIP(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestInspectIp(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestPanicOnInvocation(policy) => policy.handle_tally(tally),
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.policy_config(),
            TrafficControlPolicy::Test3ConnIP(policy) => policy.policy_config(),
            TrafficControlPolicy::TestInspectIp(policy) => policy.policy_config(),
            TrafficControlPolicy::TestPanicOnInvocation(policy) => policy.policy_config(),
        }
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PolicyConfig {
    pub tallyable_error_codes: Vec<SuiError>,
    pub connection_blocklist_ttl_sec: u64,
    pub proxy_blocklist_ttl_sec: u64,
    pub spam_policy_type: PolicyType,
    pub error_policy_type: PolicyType,
    pub channel_capacity: usize,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            tallyable_error_codes: vec![],
            connection_blocklist_ttl_sec: 0,
            proxy_blocklist_ttl_sec: 0,
            spam_policy_type: PolicyType::NoOp,
            error_policy_type: PolicyType::NoOp,
            channel_capacity: 100,
        }
    }
}

impl PolicyConfig {
    pub fn to_spam_policy(&self) -> TrafficControlPolicy {
        self.to_policy(&self.spam_policy_type)
    }

    pub fn to_error_policy(&self) -> TrafficControlPolicy {
        self.to_policy(&self.error_policy_type)
    }

    fn to_policy(&self, policy_type: &PolicyType) -> TrafficControlPolicy {
        match policy_type {
            PolicyType::NoOp => TrafficControlPolicy::NoOp(NoOpPolicy::new(self.clone())),
            PolicyType::Test3ConnIP => {
                TrafficControlPolicy::Test3ConnIP(Test3ConnIPPolicy::new(self.clone()))
            }
            PolicyType::TestInspectIp => {
                TrafficControlPolicy::TestInspectIp(TestInspectIpPolicy::new(self.clone()))
            }
            PolicyType::TestPanicOnInvocation => TrafficControlPolicy::TestPanicOnInvocation(
                TestPanicOnInvocationPolicy::new(self.clone()),
            ),
        }
    }
}

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

////////////// *** Test policies below this point *** //////////////

#[derive(Clone)]
pub struct Test3ConnIPPolicy {
    config: PolicyConfig,
    frequencies: HashMap<SocketAddr, u64>,
}

impl Test3ConnIPPolicy {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            frequencies: HashMap::new(),
        }
    }

    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        // increment the count for the IP
        if let Some(ip) = tally.connection_ip {
            let count = self.frequencies.entry(ip).or_insert(0);
            *count += 1;
            PolicyResponse {
                block_connection_ip: if *count >= 3 { Some(ip) } else { None },
                block_proxy_ip: None,
            }
        } else {
            PolicyResponse::default()
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
    }
}

#[derive(Clone)]
pub struct TestInspectIpPolicy {
    config: PolicyConfig,
}

impl TestInspectIpPolicy {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        assert!(tally.proxy_ip.is_some(), "Expected proxy_ip to be present");
        PolicyResponse {
            block_connection_ip: None,
            block_proxy_ip: None,
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        &self.config
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

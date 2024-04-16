// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, net::IpAddr, sync::Arc};

use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use std::fmt::Debug;
use std::time::SystemTime;
use sui_types::traffic_control::{PolicyConfig, PolicyType, ServiceResponse};

#[derive(Clone, Debug)]
pub struct TrafficTally {
    pub connection_ip: Option<IpAddr>,
    pub proxy_ip: Option<IpAddr>,
    pub result: ServiceResponse,
    pub timestamp: SystemTime,
}

#[derive(Clone, Debug, Default)]
pub struct PolicyResponse {
    pub block_connection_ip: Option<IpAddr>,
    pub block_proxy_ip: Option<IpAddr>,
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
    TestNConnIP(TestNConnIPPolicy),
    TestInspectIp(TestInspectIpPolicy),
    TestPanicOnInvocation(TestPanicOnInvocationPolicy),
}

impl Policy for TrafficControlPolicy {
    fn handle_tally(&mut self, tally: TrafficTally) -> PolicyResponse {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestNConnIP(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestInspectIp(policy) => policy.handle_tally(tally),
            TrafficControlPolicy::TestPanicOnInvocation(policy) => policy.handle_tally(tally),
        }
    }

    fn policy_config(&self) -> &PolicyConfig {
        match self {
            TrafficControlPolicy::NoOp(policy) => policy.policy_config(),
            TrafficControlPolicy::TestNConnIP(policy) => policy.policy_config(),
            TrafficControlPolicy::TestInspectIp(policy) => policy.policy_config(),
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
            PolicyType::TestNConnIP(n) => {
                Self::TestNConnIP(TestNConnIPPolicy::new(policy_config, n).await)
            }
            PolicyType::TestInspectIp => {
                Self::TestInspectIp(TestInspectIpPolicy::new(policy_config))
            }
            PolicyType::TestPanicOnInvocation => {
                Self::TestPanicOnInvocation(TestPanicOnInvocationPolicy::new(policy_config))
            }
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
        let ip = if let Some(ip) = tally.connection_ip {
            ip
        } else {
            return PolicyResponse::default();
        };

        // increment the count for the IP
        let mut frequencies = self.frequencies.write();
        let count = frequencies.entry(tally.connection_ip.unwrap()).or_insert(0);
        *count += 1;
        PolicyResponse {
            block_connection_ip: if *count >= self.threshold {
                Some(ip)
            } else {
                None
            },
            block_proxy_ip: None,
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

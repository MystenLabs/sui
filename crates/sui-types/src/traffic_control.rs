// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::SuiResult;
use core::hash::Hash;
use jsonrpsee::core::server::helpers::MethodResponse;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::fmt::Debug;

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
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteFirewallConfig {
    pub remote_fw_url: String,
    pub destination_port: u16,
    #[serde(default)]
    pub delegate_spam_blocking: bool,
    #[serde(default)]
    pub delegate_error_blocking: bool,
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
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            connection_blocklist_ttl_sec: 0,
            proxy_blocklist_ttl_sec: 0,
            spam_policy_type: PolicyType::NoOp,
            error_policy_type: PolicyType::NoOp,
            channel_capacity: 100,
        }
    }
}

pub fn default_connection_blocklist_ttl_sec() -> u64 {
    60
}
pub fn default_channel_capacity() -> usize {
    100
}

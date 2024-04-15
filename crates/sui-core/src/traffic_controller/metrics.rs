// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};

#[derive(Clone)]
pub struct TrafficControllerMetrics {
    pub connection_ip_blocklist_len: IntGauge,
    pub proxy_ip_blocklist_len: IntGauge,
    pub requests_blocked_at_protocol: IntCounter,
    pub blocks_delegated_to_firewall: IntCounter,
    pub firewall_delegation_request_fail: IntCounter,
}

impl TrafficControllerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            connection_ip_blocklist_len: register_int_gauge_with_registry!(
                "connection_ip_blocklist_len",
                // make the below a multiline string
                "Number of connection IP addresses (IP addresses as registered \
                    via direct socket connection to the reporting node) in the \
                    protocol layer blocklist",
                registry
            )
            .unwrap(),
            proxy_ip_blocklist_len: register_int_gauge_with_registry!(
                "proxy_ip_blocklist_len",
                // make the below a multiline string
                "Number of proxy IP addresses (IP addresses as collected \
                    via some mechanism through proxy node such as fullnode) \
                    in the protocol layer blocklist",
                registry
            )
            .unwrap(),
            requests_blocked_at_protocol: register_int_counter_with_registry!(
                "requests_blocked_at_protocol",
                "Number of requests blocked by this node at the protocol level",
                registry
            )
            .unwrap(),
            blocks_delegated_to_firewall: register_int_counter_with_registry!(
                "blocks_delegated_to_firewall",
                "Number of delegation requests to firewall to add to blocklist",
                registry
            )
            .unwrap(),
            firewall_delegation_request_fail: register_int_counter_with_registry!(
                "firewall_delegation_request_fail",
                "Number of failed http requests to firewall for blocklist delegation",
                registry
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        Self::new(&Registry::new())
    }
}

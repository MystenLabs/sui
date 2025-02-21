// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, Registry,
};

#[derive(Clone)]
pub struct TrafficControllerMetrics {
    pub tallies: IntCounter,
    pub connection_ip_blocklist_len: IntGauge,
    pub proxy_ip_blocklist_len: IntGauge,
    pub requests_blocked_at_protocol: IntCounter,
    pub blocks_delegated_to_firewall: IntCounter,
    pub firewall_delegation_request_fail: IntCounter,
    pub tally_channel_overflow: IntCounter,
    pub num_dry_run_blocked_requests: IntCounter,
    pub tally_handled: IntCounter,
    pub error_tally_handled: IntCounter,
    pub tally_error_types: IntCounterVec,
    pub deadmans_switch_enabled: IntGauge,
    pub highest_direct_spam_rate: IntGauge,
    pub highest_proxied_spam_rate: IntGauge,
    pub highest_direct_error_rate: IntGauge,
    pub highest_proxied_error_rate: IntGauge,
    pub spam_client_threshold: IntGauge,
    pub error_client_threshold: IntGauge,
    pub spam_proxied_client_threshold: IntGauge,
    pub error_proxied_client_threshold: IntGauge,
}

impl TrafficControllerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tallies: register_int_counter_with_registry!("tallies", "Number of tallies", registry)
                .unwrap(),
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
            tally_channel_overflow: register_int_counter_with_registry!(
                "tally_channel_overflow",
                "Traffic controller tally channel overflow count",
                registry
            )
            .unwrap(),
            num_dry_run_blocked_requests: register_int_counter_with_registry!(
                "traffic_control_num_dry_run_blocked_requests",
                "Number of requests blocked in traffic controller dry run mode",
                registry
            )
            .unwrap(),
            tally_handled: register_int_counter_with_registry!(
                "traffic_control_tally_handled",
                "Number of tallies handled",
                registry
            )
            .unwrap(),
            error_tally_handled: register_int_counter_with_registry!(
                "traffic_control_error_tally_handled",
                "Number of error tallies handled",
                registry
            )
            .unwrap(),
            tally_error_types: register_int_counter_vec_with_registry!(
                "traffic_control_tally_error_types",
                "Number of tally errors, grouped by error type",
                &["error_type"],
                registry
            )
            .unwrap(),
            deadmans_switch_enabled: register_int_gauge_with_registry!(
                "deadmans_switch_enabled",
                "If 1, the deadman's switch is enabled and all traffic control
                should be getting bypassed",
                registry
            )
            .unwrap(),
            highest_direct_spam_rate: register_int_gauge_with_registry!(
                "highest_direct_spam_rate",
                "Highest direct spam rate seen recently",
                registry
            )
            .unwrap(),
            highest_proxied_spam_rate: register_int_gauge_with_registry!(
                "highest_proxied_spam_rate",
                "Highest proxied spam rate seen recently",
                registry
            )
            .unwrap(),
            highest_direct_error_rate: register_int_gauge_with_registry!(
                "highest_direct_error_rate",
                "Highest direct error rate seen recently",
                registry
            )
            .unwrap(),
            highest_proxied_error_rate: register_int_gauge_with_registry!(
                "highest_proxied_error_rate",
                "Highest proxied error rate seen recently",
                registry
            )
            .unwrap(),
            spam_client_threshold: register_int_gauge_with_registry!(
                "spam_client_threshold",
                "Spam client threshold",
                registry
            )
            .unwrap(),
            error_client_threshold: register_int_gauge_with_registry!(
                "error_client_threshold",
                "Error client threshold",
                registry
            )
            .unwrap(),
            spam_proxied_client_threshold: register_int_gauge_with_registry!(
                "spam_proxied_client_threshold",
                "Spam proxied client threshold",
                registry
            )
            .unwrap(),
            error_proxied_client_threshold: register_int_gauge_with_registry!(
                "error_proxied_client_threshold",
                "Error proxied client threshold",
                registry
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        Self::new(&Registry::new())
    }
}

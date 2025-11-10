// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    HistogramVec, IntCounterVec, IntGaugeVec, Registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry,
};
use std::sync::Arc;

/// Bridge-specific metrics for comprehensive cross-chain monitoring
#[derive(Clone)]
pub struct BridgeIndexerMetrics {
    /// Bridge transaction event counters by type and direction
    pub bridge_events_total: IntCounterVec,

    /// Token transfer metrics by direction and status
    pub token_transfers_total: IntCounterVec,
    pub token_transfer_volume_usd: IntCounterVec,
    pub token_transfer_gas_used: IntCounterVec,

    /// Bridge security and governance metrics
    pub governance_actions_total: IntCounterVec,
    pub bridge_errors_total: IntCounterVec,
    pub bridge_emergency_events_total: IntCounterVec,

    /// Cross-chain latency tracking
    pub bridge_transfer_latency: HistogramVec,

    /// Current bridge state gauges
    pub bridge_committee_voting_power: IntGaugeVec,
    pub bridge_token_limits_current: IntGaugeVec,
    pub bridge_pause_status: IntGaugeVec,

    /// Token-specific metrics
    pub bridge_token_reserves: IntGaugeVec,
    pub bridge_supported_tokens: IntGaugeVec,
}

impl BridgeIndexerMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            bridge_events_total: register_int_counter_vec_with_registry!(
                "bridge_events_total",
                "Total bridge events processed by event type",
                &["event_type", "chain_source"],
                registry
            )
            .unwrap(),

            token_transfers_total: register_int_counter_vec_with_registry!(
                "bridge_token_transfers_total",
                "Total token transfers by direction and status",
                &["direction", "status", "token_type"],
                registry
            )
            .unwrap(),

            token_transfer_volume_usd: register_int_counter_vec_with_registry!(
                "bridge_token_transfer_volume_usd_total",
                "Total USD value of token transfers",
                &["direction", "token_type"],
                registry
            )
            .unwrap(),

            token_transfer_gas_used: register_int_counter_vec_with_registry!(
                "bridge_token_transfer_gas_used_total",
                "Total gas consumed by bridge transactions",
                &["direction", "success"],
                registry
            )
            .unwrap(),

            governance_actions_total: register_int_counter_vec_with_registry!(
                "bridge_governance_actions_total",
                "Total governance actions by type",
                &["action_type", "chain_source"],
                registry
            )
            .unwrap(),

            bridge_errors_total: register_int_counter_vec_with_registry!(
                "bridge_errors_total",
                "Total bridge transaction errors by type",
                &["error_type", "transaction_type"],
                registry
            )
            .unwrap(),

            bridge_emergency_events_total: register_int_counter_vec_with_registry!(
                "bridge_emergency_events_total",
                "Critical bridge emergency events",
                &["event_type", "severity"],
                registry
            )
            .unwrap(),

            bridge_transfer_latency: register_histogram_vec_with_registry!(
                "bridge_transfer_latency_seconds",
                "Time between deposit and claim completion",
                &["direction", "token_type"],
                vec![
                    1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0
                ],
                registry
            )
            .unwrap(),

            bridge_committee_voting_power: register_int_gauge_vec_with_registry!(
                "bridge_committee_voting_power",
                "Current voting power distribution",
                &["validator_address", "status"],
                registry
            )
            .unwrap(),

            bridge_token_limits_current: register_int_gauge_vec_with_registry!(
                "bridge_token_limits_current",
                "Current bridge transfer limits",
                &["token_type", "direction", "limit_type"],
                registry
            )
            .unwrap(),

            bridge_pause_status: register_int_gauge_vec_with_registry!(
                "bridge_pause_status",
                "Bridge operational status (1=active, 0=paused)",
                &["component"],
                registry
            )
            .unwrap(),

            bridge_token_reserves: register_int_gauge_vec_with_registry!(
                "bridge_token_reserves",
                "Token reserves in bridge contracts",
                &["token_type", "chain"],
                registry
            )
            .unwrap(),

            bridge_supported_tokens: register_int_gauge_vec_with_registry!(
                "bridge_supported_tokens",
                "Number of supported tokens by chain",
                &["chain"],
                registry
            )
            .unwrap(),
        })
    }
}

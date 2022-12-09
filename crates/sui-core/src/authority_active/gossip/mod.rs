// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};

/// See the `new` function for description for each metrics.
#[derive(Clone, Debug)]
pub struct GossipMetrics {
    pub concurrent_followed_validators: IntGauge,
    pub reconnect_interval_ms: IntGauge,
    pub total_tx_received: IntCounter,
    pub total_batch_received: IntCounter,
    pub wait_for_finality_latency_sec: Histogram,
    pub total_attempts_cert_downloads: IntCounter,
    pub total_successful_attempts_cert_downloads: IntCounter,
    pub follower_stream_duration: Histogram,
}

const WAIT_FOR_FINALITY_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];
const FOLLOWER_STREAM_DURATION_SEC_BUCKETS: &[f64] = &[
    0.1, 1., 5., 10., 20., 30., 40., 50., 60., 90., 120., 180., 240., 300.,
];

impl GossipMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            concurrent_followed_validators: register_int_gauge_with_registry!(
                "gossip_concurrent_followed_validators",
                "Number of validators being followed concurrently at the moment.",
                registry,
            )
            .unwrap(),
            reconnect_interval_ms: register_int_gauge_with_registry!(
                "gossip_reconnect_interval_ms",
                "Interval to start the next gossip/node sync task, in millisec",
                registry,
            )
            .unwrap(),
            total_tx_received: register_int_counter_with_registry!(
                "gossip_total_tx_received",
                "Total number of transactions received through gossip/node sync",
                registry,
            )
            .unwrap(),
            total_batch_received: register_int_counter_with_registry!(
                "gossip_total_batch_received",
                "Total number of signed batches received through gossip/node sync",
                registry,
            )
            .unwrap(),
            wait_for_finality_latency_sec: register_histogram_with_registry!(
                "gossip_wait_for_finality_latency_sec",
                "Latency histogram for gossip/node sync process to wait for txs to become final, in seconds",
                WAIT_FOR_FINALITY_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_attempts_cert_downloads: register_int_counter_with_registry!(
                "gossip_total_attempts_cert_downloads",
                "Total number of certs/effects download attempts through gossip/node sync process",
                registry,
            )
            .unwrap(),
            total_successful_attempts_cert_downloads: register_int_counter_with_registry!(
                "gossip_total_successful_attempts_cert_downloads",
                "Total number of success certs/effects downloads through gossip/node sync process",
                registry,
            )
            .unwrap(),
            follower_stream_duration: register_histogram_with_registry!(
                "follower_stream_duration",
                "Latency histogram of the duration of the follower streams to peers, in seconds",
                FOLLOWER_STREAM_DURATION_SEC_BUCKETS.to_vec(),
                registry,
            )
                .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

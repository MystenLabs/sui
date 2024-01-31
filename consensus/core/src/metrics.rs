// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use std::sync::Arc;

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4,
    1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.,
    12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

pub(crate) struct Metrics {
    pub node_metrics: NodeMetrics,
    pub channel_metrics: ChannelMetrics,
}

pub(crate) fn initialise_metrics(registry: Registry) -> Arc<Metrics> {
    let node_metrics = NodeMetrics::new(&registry);
    let channel_metrics = ChannelMetrics::new(&registry);

    Arc::new(Metrics {
        node_metrics,
        channel_metrics,
    })
}

#[cfg(test)]
pub(crate) fn test_metrics() -> Arc<Metrics> {
    initialise_metrics(Registry::new())
}

pub(crate) struct NodeMetrics {
    pub uptime: Histogram,
    pub quorum_receive_latency: Histogram,
    #[allow(unused)]
    pub committed_leaders_total: IntCounterVec,
    pub core_lock_enqueued: IntCounter,
    pub core_lock_dequeued: IntCounter,
    pub leader_timeout_total: IntCounter,
    pub threshold_clock_round: IntGauge,
}

impl NodeMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            uptime: register_histogram_with_registry!(
                "uptime",
                "Total node uptime",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            quorum_receive_latency: register_histogram_with_registry!(
                "quorum_receive_latency",
                "The time it took to receive a new round quorum of blocks",
                registry
            )
            .unwrap(),
            committed_leaders_total: register_int_counter_vec_with_registry!(
                "committed_leaders_total",
                "Total number of (direct or indirect) committed leaders per authority",
                &["authority", "commit_type"],
                registry,
            )
            .unwrap(),
            core_lock_enqueued: register_int_counter_with_registry!(
                "core_lock_enqueued",
                "Number of enqueued core requests",
                registry,
            )
            .unwrap(),
            core_lock_dequeued: register_int_counter_with_registry!(
                "core_lock_dequeued",
                "Number of dequeued core requests",
                registry,
            )
            .unwrap(),
            leader_timeout_total: register_int_counter_with_registry!(
                "leader_timeout_total",
                "Total number of leader timeouts",
                registry,
            )
            .unwrap(),
            threshold_clock_round: register_int_gauge_with_registry!(
                "threshold_clock_round",
                "The current threshold clock round. We only advance to a new round when a quorum of parents have been synced.",
                registry,
            ).unwrap(),
        }
    }
}

pub(crate) struct ChannelMetrics {
    /// occupancy of the channel from TransactionsClient to TransactionsConsumer
    pub tx_transactions_submit: IntGauge,
    /// total received on channel from TransactionsClient to TransactionsConsumer
    pub tx_transactions_submit_total: IntCounter,
    /// occupancy of the CoreThread commands channel
    pub core_thread: IntGauge,
    /// total received on the CoreThread commands channel
    pub core_thread_total: IntCounter,
}

impl ChannelMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_transactions_submit: register_int_gauge_with_registry!(
                "tx_transactions_submit",
                "occupancy of the channel from the `TransactionsClient` to the `TransactionsConsumer`",
                registry
            ).unwrap(),
            tx_transactions_submit_total: register_int_counter_with_registry!(
                "tx_transactions_submit_total",
                "total received on channel from the `TransactionsClient` to the `TransactionsConsumer`",
                registry
            ).unwrap(),
            core_thread: register_int_gauge_with_registry!(
                "core_thread",
                "occupancy of the `CoreThread` commands channel",
                registry
            ).unwrap(),
            core_thread_total: register_int_counter_with_registry!(
                "core_thread_total",
                "total received on the `CoreThread` commands channel",
                registry
            ).unwrap(),
        }
    }
}

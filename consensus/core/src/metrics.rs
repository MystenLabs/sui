// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry,
};

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4,
    1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.,
    12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

pub(crate) struct Metrics {
    pub(crate) node_metrics: NodeMetrics,
    pub(crate) channel_metrics: ChannelMetrics,
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
    pub core_lock_enqueued: IntCounter,
    pub core_lock_dequeued: IntCounter,
    pub leader_timeout_total: IntCounter,
    pub threshold_clock_round: IntGauge,
    pub suspended_blocks: IntCounterVec,
    pub unsuspended_blocks: IntCounterVec,
    pub invalid_blocks: IntCounterVec,
    pub block_timestamp_drift_wait_ms: IntCounterVec,
    pub broadcaster_rtt_estimate_ms: IntGaugeVec,

    // Commit Metrics
    pub last_decided_leader_round: IntGauge,
    pub last_committed_leader_round: IntGauge,
    pub decided_leaders_total: IntCounterVec,
    pub blocks_per_commit_count: Histogram,
    pub sub_dags_per_commit_count: Histogram,
    pub block_commit_latency: Histogram,
    pub fetched_blocks: IntCounterVec,
    pub fetch_blocks_scheduler_inflight: IntGauge,
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
            suspended_blocks: register_int_counter_vec_with_registry!(
                "suspended_blocks",
                "The number of suspended blocks. The counter is reported uniquely, so if a block is sent for reprocessing while alreadly suspended then is not double counted",
                &["authority"],
                registry,
            ).unwrap(),
            unsuspended_blocks: register_int_counter_vec_with_registry!(
                "unsuspended_blocks",
                "The number of unsuspended blocks",
                &["authority"],
                registry,
            ).unwrap(),
            // TODO: add a short status label.
            invalid_blocks: register_int_counter_vec_with_registry!(
                "invalid_blocks",
                "Number of invalid blocks per peer authority",
                &["authority", "source"],
                registry,
            )
            .unwrap(),
            block_timestamp_drift_wait_ms: register_int_counter_vec_with_registry!(
                "block_timestamp_drift_wait_ms",
                "Total time in ms spent waiting, when a received block has timestamp in future.",
                &["authority"],
                registry,
            )
            .unwrap(),
            broadcaster_rtt_estimate_ms: register_int_gauge_vec_with_registry!(
                "broadcaster_rtt_estimate_ms",
                "Estimated RTT latency per peer authority, for block sending in Broadcaster",
                &["peer"],
                registry,
            )
            .unwrap(),

            // Commit Metrics
            last_decided_leader_round: register_int_gauge_with_registry!(
                "last_decided_leader_round",
                "The last round where a commit decision was made.",
                registry,
            ).unwrap(),
            last_committed_leader_round: register_int_gauge_with_registry!(
                "last_committed_leader_round",
                "The last round where a leader was committed to store and sent to commit consumer.",
                registry,
            ).unwrap(),
            decided_leaders_total: register_int_counter_vec_with_registry!(
                "committed_leaders_total",
                "Total number of (direct or indirect) committed leaders per authority",
                &["authority", "commit_type"],
                registry,
            )
            .unwrap(),
            block_commit_latency: register_histogram_with_registry!(
                "block_commit_latency",
                "The time taken between block creation and block commit.",
                registry,
            )
            .unwrap(),
            blocks_per_commit_count: register_histogram_with_registry!(
                "blocks_per_commit_count",
                "The number of blocks per commit.",
                registry,
            )
            .unwrap(),
            sub_dags_per_commit_count: register_histogram_with_registry!(
                "sub_dags_per_commit_count",
                "The number of subdags per commit.",
                registry,
            )
            .unwrap(),
            fetched_blocks: register_int_counter_vec_with_registry!(
                "fetched_blocks",
                "Number of fetched blocks per peer authority via the synchronizer.",
                &["authority", "type"],
                registry,
            ).unwrap(),
            fetch_blocks_scheduler_inflight: register_int_gauge_with_registry!(
                "fetch_blocks_scheduler_inflight",
                "Designates whether the synchronizer scheduler task to fetch blocks is currently running",
                registry,
            ).unwrap()
        }
    }
}

pub(crate) struct ChannelMetrics {
    /// occupancy of the channel from TransactionClient to TransactionConsumer
    pub tx_transactions_submit: IntGauge,
    /// total received on channel from TransactionClient to TransactionConsumer
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
                "occupancy of the channel from the `TransactionClient` to the `TransactionConsumer`",
                registry
            ).unwrap(),
            tx_transactions_submit_total: register_int_counter_with_registry!(
                "tx_transactions_submit_total",
                "total received on channel from the `TransactionClient` to the `TransactionConsumer`",
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

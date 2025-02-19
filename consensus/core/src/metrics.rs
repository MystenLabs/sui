// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    exponential_buckets, register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry,
};

use crate::network::metrics::NetworkMetrics;

// starts from 1μs, 50μs, 100μs...
const FINE_GRAINED_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.000_001, 0.000_050, 0.000_100, 0.000_500, 0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.25,
    0.3, 0.35, 0.4, 0.45, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4, 1.6, 1.8, 2.0, 2.5, 3.0, 3.5,
    4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10., 20., 30., 60., 120.,
];

const NUM_BUCKETS: &[f64] = &[
    1.0,
    2.0,
    4.0,
    8.0,
    10.0,
    20.0,
    40.0,
    80.0,
    100.0,
    150.0,
    200.0,
    400.0,
    800.0,
    1000.0,
    2000.0,
    3000.0,
    5000.0,
    10000.0,
    20000.0,
    30000.0,
    50000.0,
    100_000.0,
    200_000.0,
    300_000.0,
    500_000.0,
    1_000_000.0,
];

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.6, 0.7, 0.8, 0.9,
    1.0, 1.2, 1.4, 1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5,
    9.0, 9.5, 10., 12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

const SIZE_BUCKETS: &[f64] = &[
    100.,
    400.,
    800.,
    1_000.,
    2_000.,
    5_000.,
    10_000.,
    20_000.,
    50_000.,
    100_000.,
    200_000.0,
    300_000.0,
    400_000.0,
    500_000.0,
    1_000_000.0,
    2_000_000.0,
    3_000_000.0,
    5_000_000.0,
    10_000_000.0,
]; // size in bytes

pub(crate) struct Metrics {
    pub(crate) node_metrics: NodeMetrics,
    pub(crate) network_metrics: NetworkMetrics,
}

pub(crate) fn initialise_metrics(registry: Registry) -> Arc<Metrics> {
    let node_metrics = NodeMetrics::new(&registry);
    let network_metrics = NetworkMetrics::new(&registry);

    Arc::new(Metrics {
        node_metrics,
        network_metrics,
    })
}

#[cfg(test)]
pub(crate) fn test_metrics() -> Arc<Metrics> {
    initialise_metrics(Registry::new())
}

pub(crate) struct NodeMetrics {
    pub(crate) block_commit_latency: Histogram,
    pub(crate) proposed_blocks: IntCounterVec,
    pub(crate) proposed_block_size: Histogram,
    pub(crate) proposed_block_transactions: Histogram,
    pub(crate) proposed_block_ancestors: Histogram,
    pub(crate) proposed_block_ancestors_depth: HistogramVec,
    pub(crate) highest_verified_authority_round: IntGaugeVec,
    pub(crate) lowest_verified_authority_round: IntGaugeVec,
    pub(crate) block_proposal_interval: Histogram,
    pub(crate) block_proposal_leader_wait_ms: IntCounterVec,
    pub(crate) block_proposal_leader_wait_count: IntCounterVec,
    pub(crate) block_timestamp_drift_wait_ms: IntCounterVec,
    pub(crate) blocks_per_commit_count: Histogram,
    pub(crate) broadcaster_rtt_estimate_ms: IntGaugeVec,
    pub(crate) core_add_blocks_batch_size: Histogram,
    pub(crate) core_check_block_refs_batch_size: Histogram,
    pub(crate) core_lock_dequeued: IntCounter,
    pub(crate) core_lock_enqueued: IntCounter,
    pub(crate) core_skipped_proposals: IntCounterVec,
    pub(crate) highest_accepted_authority_round: IntGaugeVec,
    pub(crate) highest_accepted_round: IntGauge,
    pub(crate) accepted_blocks: IntCounterVec,
    pub(crate) dag_state_recent_blocks: IntGauge,
    pub(crate) dag_state_recent_refs: IntGauge,
    pub(crate) dag_state_store_read_count: IntCounterVec,
    pub(crate) dag_state_store_write_count: IntCounter,
    pub(crate) fetch_blocks_scheduler_inflight: IntGauge,
    pub(crate) fetch_blocks_scheduler_skipped: IntCounterVec,
    pub(crate) synchronizer_fetched_blocks_by_peer: IntCounterVec,
    pub(crate) synchronizer_missing_blocks_by_authority: IntCounterVec,
    pub(crate) synchronizer_current_missing_blocks_by_authority: IntGaugeVec,
    pub(crate) synchronizer_fetched_blocks_by_authority: IntCounterVec,
    pub(crate) network_received_excluded_ancestors_from_authority: IntCounterVec,
    pub(crate) network_excluded_ancestors_sent_to_fetch: IntCounterVec,
    pub(crate) network_excluded_ancestors_count_by_authority: IntCounterVec,
    pub(crate) invalid_blocks: IntCounterVec,
    pub(crate) rejected_blocks: IntCounterVec,
    pub(crate) rejected_future_blocks: IntCounterVec,
    pub(crate) subscribed_blocks: IntCounterVec,
    pub(crate) verified_blocks: IntCounterVec,
    pub(crate) committed_leaders_total: IntCounterVec,
    pub(crate) last_committed_authority_round: IntGaugeVec,
    pub(crate) last_committed_leader_round: IntGauge,
    pub(crate) last_commit_index: IntGauge,
    pub(crate) last_known_own_block_round: IntGauge,
    pub(crate) sync_last_known_own_block_retries: IntCounter,
    pub(crate) commit_round_advancement_interval: Histogram,
    pub(crate) last_decided_leader_round: IntGauge,
    pub(crate) leader_timeout_total: IntCounterVec,
    pub(crate) smart_selection_wait: IntCounter,
    pub(crate) ancestor_state_change_by_authority: IntCounterVec,
    pub(crate) excluded_proposal_ancestors_count_by_authority: IntCounterVec,
    pub(crate) included_excluded_proposal_ancestors_count_by_authority: IntCounterVec,
    pub(crate) missing_blocks_total: IntCounter,
    pub(crate) missing_blocks_after_fetch_total: IntCounter,
    pub(crate) num_of_bad_nodes: IntGauge,
    pub(crate) quorum_receive_latency: Histogram,
    pub(crate) reputation_scores: IntGaugeVec,
    pub(crate) scope_processing_time: HistogramVec,
    pub(crate) sub_dags_per_commit_count: Histogram,
    pub(crate) block_suspensions: IntCounterVec,
    pub(crate) block_unsuspensions: IntCounterVec,
    pub(crate) suspended_block_time: HistogramVec,
    pub(crate) block_manager_suspended_blocks: IntGauge,
    pub(crate) block_manager_missing_ancestors: IntGauge,
    pub(crate) block_manager_missing_blocks: IntGauge,
    pub(crate) block_manager_missing_blocks_by_authority: IntCounterVec,
    pub(crate) block_manager_missing_ancestors_by_authority: IntCounterVec,
    pub(crate) block_manager_gc_unsuspended_blocks: IntCounterVec,
    pub(crate) block_manager_skipped_blocks: IntCounterVec,
    pub(crate) threshold_clock_round: IntGauge,
    pub(crate) subscriber_connection_attempts: IntCounterVec,
    pub(crate) subscribed_to: IntGaugeVec,
    pub(crate) subscribed_by: IntGaugeVec,
    pub(crate) commit_sync_inflight_fetches: IntGauge,
    pub(crate) commit_sync_pending_fetches: IntGauge,
    pub(crate) commit_sync_fetched_commits: IntCounter,
    pub(crate) commit_sync_fetched_blocks: IntCounter,
    pub(crate) commit_sync_total_fetched_blocks_size: IntCounter,
    pub(crate) commit_sync_quorum_index: IntGauge,
    pub(crate) commit_sync_highest_synced_index: IntGauge,
    pub(crate) commit_sync_highest_fetched_index: IntGauge,
    pub(crate) commit_sync_local_index: IntGauge,
    pub(crate) commit_sync_gap_on_processing: IntCounter,
    pub(crate) commit_sync_fetch_loop_latency: Histogram,
    pub(crate) commit_sync_fetch_once_latency: Histogram,
    pub(crate) commit_sync_fetch_once_errors: IntCounterVec,
    pub(crate) commit_sync_fetch_missing_blocks: IntCounterVec,
    pub(crate) round_prober_received_quorum_round_gaps: IntGaugeVec,
    pub(crate) round_prober_accepted_quorum_round_gaps: IntGaugeVec,
    pub(crate) round_prober_low_received_quorum_round: IntGaugeVec,
    pub(crate) round_prober_low_accepted_quorum_round: IntGaugeVec,
    pub(crate) round_prober_current_received_round_gaps: IntGaugeVec,
    pub(crate) round_prober_current_accepted_round_gaps: IntGaugeVec,
    pub(crate) round_prober_propagation_delays: Histogram,
    pub(crate) round_prober_last_propagation_delay: IntGauge,
    pub(crate) round_prober_request_errors: IntCounterVec,
    pub(crate) uptime: Histogram,
}

impl NodeMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            block_commit_latency: register_histogram_with_registry!(
                "block_commit_latency",
                "The time taken between block creation and block commit.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            proposed_blocks: register_int_counter_vec_with_registry!(
                "proposed_blocks",
                "Total number of proposed blocks. If force is true then this block has been created forcefully via a leader timeout event.",
                &["force"],
                registry,
            ).unwrap(),
            proposed_block_size: register_histogram_with_registry!(
                "proposed_block_size",
                "The size (in bytes) of proposed blocks",
                SIZE_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            proposed_block_transactions: register_histogram_with_registry!(
                "proposed_block_transactions",
                "# of transactions contained in proposed blocks",
                NUM_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            proposed_block_ancestors: register_histogram_with_registry!(
                "proposed_block_ancestors",
                "Number of ancestors in proposed blocks",
                exponential_buckets(1.0, 1.4, 20).unwrap(),
                registry,
            ).unwrap(),
            proposed_block_ancestors_depth: register_histogram_vec_with_registry!(
                "proposed_block_ancestors_depth",
                "The depth in rounds of ancestors included in newly proposed blocks",
                &["authority"],
                exponential_buckets(1.0, 2.0, 14).unwrap(),
                registry,
            ).unwrap(),
            highest_verified_authority_round: register_int_gauge_vec_with_registry!(
                "highest_verified_authority_round",
                "The highest round of verified block for the corresponding authority",
                &["authority"],
                registry,
            ).unwrap(),
            lowest_verified_authority_round: register_int_gauge_vec_with_registry!(
                "lowest_verified_authority_round",
                "The lowest round of verified block for the corresponding authority",
                &["authority"],
                registry,
            ).unwrap(),
            block_proposal_interval: register_histogram_with_registry!(
                "block_proposal_interval",
                "Intervals (in secs) between block proposals.",
                FINE_GRAINED_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            block_proposal_leader_wait_ms: register_int_counter_vec_with_registry!(
                "block_proposal_leader_wait_ms",
                "Total time in ms spent waiting for a leader when proposing blocks.",
                &["authority"],
                registry,
            ).unwrap(),
            block_proposal_leader_wait_count: register_int_counter_vec_with_registry!(
                "block_proposal_leader_wait_count",
                "Total times waiting for a leader when proposing blocks.",
                &["authority"],
                registry,
            ).unwrap(),
            block_timestamp_drift_wait_ms: register_int_counter_vec_with_registry!(
                "block_timestamp_drift_wait_ms",
                "Total time in ms spent waiting, when a received block has timestamp in future.",
                &["authority", "source"],
                registry,
            ).unwrap(),
            blocks_per_commit_count: register_histogram_with_registry!(
                "blocks_per_commit_count",
                "The number of blocks per commit.",
                NUM_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            broadcaster_rtt_estimate_ms: register_int_gauge_vec_with_registry!(
                "broadcaster_rtt_estimate_ms",
                "Estimated RTT latency per peer authority, for block sending in Broadcaster",
                &["peer"],
                registry,
            ).unwrap(),
            core_add_blocks_batch_size: register_histogram_with_registry!(
                "core_add_blocks_batch_size",
                "The number of blocks received from Core for processing on a single batch",
                NUM_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            core_check_block_refs_batch_size: register_histogram_with_registry!(
                "core_check_block_refs_batch_size",
                "The number of excluded blocks received from Core for search on a single batch",
                NUM_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            core_lock_dequeued: register_int_counter_with_registry!(
                "core_lock_dequeued",
                "Number of dequeued core requests",
                registry,
            ).unwrap(),
            core_lock_enqueued: register_int_counter_with_registry!(
                "core_lock_enqueued",
                "Number of enqueued core requests",
                registry,
            ).unwrap(),
            core_skipped_proposals: register_int_counter_vec_with_registry!(
                "core_skipped_proposals",
                "Number of proposals skipped in the Core, per reason",
                &["reason"],
                registry,
            ).unwrap(),
            highest_accepted_authority_round: register_int_gauge_vec_with_registry!(
                "highest_accepted_authority_round",
                "The highest round where a block has been accepted per authority. Resets on restart.",
                &["authority"],
                registry,
            ).unwrap(),
            highest_accepted_round: register_int_gauge_with_registry!(
                "highest_accepted_round",
                "The highest round where a block has been accepted. Resets on restart.",
                registry,
            ).unwrap(),
            accepted_blocks: register_int_counter_vec_with_registry!(
                "accepted_blocks",
                "Number of accepted blocks by source (own, others)",
                &["source"],
                registry,
            ).unwrap(),
            dag_state_recent_blocks: register_int_gauge_with_registry!(
                "dag_state_recent_blocks",
                "Number of recent blocks cached in the DagState",
                registry,
            ).unwrap(),
            dag_state_recent_refs: register_int_gauge_with_registry!(
                "dag_state_recent_refs",
                "Number of recent refs cached in the DagState",
                registry,
            ).unwrap(),
            dag_state_store_read_count: register_int_counter_vec_with_registry!(
                "dag_state_store_read_count",
                "Number of times DagState needs to read from store per operation type",
                &["type"],
                registry,
            ).unwrap(),
            dag_state_store_write_count: register_int_counter_with_registry!(
                "dag_state_store_write_count",
                "Number of times DagState needs to write to store",
                registry,
            ).unwrap(),
            fetch_blocks_scheduler_inflight: register_int_gauge_with_registry!(
                "fetch_blocks_scheduler_inflight",
                "Designates whether the synchronizer scheduler task to fetch blocks is currently running",
                registry,
            ).unwrap(),
            fetch_blocks_scheduler_skipped: register_int_counter_vec_with_registry!(
                "fetch_blocks_scheduler_skipped",
                "Number of times the scheduler skipped fetching blocks",
                &["reason"],
                registry
            ).unwrap(),
            synchronizer_fetched_blocks_by_peer: register_int_counter_vec_with_registry!(
                "synchronizer_fetched_blocks_by_peer",
                "Number of fetched blocks per peer authority via the synchronizer and also by block authority",
                &["peer", "type"],
                registry,
            ).unwrap(),
            synchronizer_missing_blocks_by_authority: register_int_counter_vec_with_registry!(
                "synchronizer_missing_blocks_by_authority",
                "Number of missing blocks per block author, as observed by the synchronizer during periodic sync.",
                &["authority"],
                registry,
            ).unwrap(),
            synchronizer_current_missing_blocks_by_authority: register_int_gauge_vec_with_registry!(
                "synchronizer_current_missing_blocks_by_authority",
                "Current number of missing blocks per block author, as observed by the synchronizer during periodic sync.",
                &["authority"],
                registry,
            ).unwrap(),
            synchronizer_fetched_blocks_by_authority: register_int_counter_vec_with_registry!(
                "synchronizer_fetched_blocks_by_authority",
                "Number of fetched blocks per block author via the synchronizer",
                &["authority", "type"],
                registry,
            ).unwrap(),
            network_received_excluded_ancestors_from_authority: register_int_counter_vec_with_registry!(
                "network_received_excluded_ancestors_from_authority",
                "Number of excluded ancestors received from each authority.",
                &["authority"],
                registry,
            ).unwrap(),
            network_excluded_ancestors_count_by_authority: register_int_counter_vec_with_registry!(
                "network_excluded_ancestors_count_by_authority",
                "Total number of excluded ancestors per authority.",
                &["authority"],
                registry,
            ).unwrap(),
            network_excluded_ancestors_sent_to_fetch: register_int_counter_vec_with_registry!(
                "network_excluded_ancestors_sent_to_fetch",
                "Number of excluded ancestors sent to fetch.",
                &["authority"],
                registry,
            ).unwrap(),
            last_known_own_block_round: register_int_gauge_with_registry!(
                "last_known_own_block_round",
                "The highest round of our own block as this has been synced from peers during an amnesia recovery",
                registry,
            ).unwrap(),
            sync_last_known_own_block_retries: register_int_counter_with_registry!(
                "sync_last_known_own_block_retries",
                "Number of times this node tried to fetch the last own block from peers",
                registry,
            ).unwrap(),
            // TODO: add a short status label.
            invalid_blocks: register_int_counter_vec_with_registry!(
                "invalid_blocks",
                "Number of invalid blocks per peer authority",
                &["authority", "source", "error"],
                registry,
            ).unwrap(),
            rejected_blocks: register_int_counter_vec_with_registry!(
                "rejected_blocks",
                "Number of blocks rejected before verifications",
                &["reason"],
                registry,
            ).unwrap(),
            rejected_future_blocks: register_int_counter_vec_with_registry!(
                "rejected_future_blocks",
                "Number of blocks rejected because their timestamp is too far in the future",
                &["authority"],
                registry,
            ).unwrap(),
            subscribed_blocks: register_int_counter_vec_with_registry!(
                "subscribed_blocks",
                "Number of blocks received from each peer before verification",
                &["authority"],
                registry,
            ).unwrap(),
            verified_blocks: register_int_counter_vec_with_registry!(
                "verified_blocks",
                "Number of blocks received from each peer that are verified",
                &["authority"],
                registry,
            ).unwrap(),
            committed_leaders_total: register_int_counter_vec_with_registry!(
                "committed_leaders_total",
                "Total number of (direct or indirect) committed leaders per authority",
                &["authority", "commit_type"],
                registry,
            ).unwrap(),
            last_committed_authority_round: register_int_gauge_vec_with_registry!(
                "last_committed_authority_round",
                "The last round committed by authority.",
                &["authority"],
                registry,
            ).unwrap(),
            last_committed_leader_round: register_int_gauge_with_registry!(
                "last_committed_leader_round",
                "The last round where a leader was committed to store and sent to commit consumer.",
                registry,
            ).unwrap(),
            last_commit_index: register_int_gauge_with_registry!(
                "last_commit_index",
                "Index of the last commit.",
                registry,
            ).unwrap(),
            commit_round_advancement_interval: register_histogram_with_registry!(
                "commit_round_advancement_interval",
                "Intervals (in secs) between commit round advancements.",
                FINE_GRAINED_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            last_decided_leader_round: register_int_gauge_with_registry!(
                "last_decided_leader_round",
                "The last round where a commit decision was made.",
                registry,
            ).unwrap(),
            leader_timeout_total: register_int_counter_vec_with_registry!(
                "leader_timeout_total",
                "Total number of leader timeouts, either when the min round time has passed, or max leader timeout",
                &["timeout_type"],
                registry,
            ).unwrap(),
            smart_selection_wait: register_int_counter_with_registry!(
                "smart_selection_wait",
                "Number of times we waited for smart ancestor selection.",
                registry,
            ).unwrap(),
            ancestor_state_change_by_authority: register_int_counter_vec_with_registry!(
                "ancestor_state_change_by_authority",
                "The total number of times an ancestor state changed to EXCLUDE or INCLUDE.",
                &["authority", "state"],
                registry,
            ).unwrap(),
            excluded_proposal_ancestors_count_by_authority: register_int_counter_vec_with_registry!(
                "excluded_proposal_ancestors_count_by_authority",
                "Total number of excluded ancestors per authority during proposal.",
                &["authority"],
                registry,
            ).unwrap(),
            included_excluded_proposal_ancestors_count_by_authority: register_int_counter_vec_with_registry!(
                "included_excluded_proposal_ancestors_count_by_authority",
                "Total number of ancestors per authority with 'excluded' status that got included in proposal. Either weak or strong type.",
                &["authority", "type"],
                registry,
            ).unwrap(),
            missing_blocks_total: register_int_counter_with_registry!(
                "missing_blocks_total",
                "Total cumulative number of missing blocks",
                registry,
            ).unwrap(),
            missing_blocks_after_fetch_total: register_int_counter_with_registry!(
                "missing_blocks_after_fetch_total",
                "Total number of missing blocks after fetching blocks from peer",
                registry,
            ).unwrap(),
            num_of_bad_nodes: register_int_gauge_with_registry!(
                "num_of_bad_nodes",
                "The number of bad nodes in the new leader schedule",
                registry
            ).unwrap(),
            quorum_receive_latency: register_histogram_with_registry!(
                "quorum_receive_latency",
                "The time it took to receive a new round quorum of blocks",
                registry
            ).unwrap(),
            reputation_scores: register_int_gauge_vec_with_registry!(
                "reputation_scores",
                "Reputation scores for each authority",
                &["authority"],
                registry,
            ).unwrap(),
            scope_processing_time: register_histogram_vec_with_registry!(
                "scope_processing_time",
                "The processing time of a specific code scope",
                &["scope"],
                FINE_GRAINED_LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            sub_dags_per_commit_count: register_histogram_with_registry!(
                "sub_dags_per_commit_count",
                "The number of subdags per commit.",
                registry,
            ).unwrap(),
            block_suspensions: register_int_counter_vec_with_registry!(
                "block_suspensions",
                "The number block suspensions. The counter is reported uniquely, so if a block is sent for reprocessing while already suspended then is not double counted",
                &["authority"],
                registry,
            ).unwrap(),
            block_unsuspensions: register_int_counter_vec_with_registry!(
                "block_unsuspensions",
                "The number of block unsuspensions.",
                &["authority"],
                registry,
            ).unwrap(),
            suspended_block_time: register_histogram_vec_with_registry!(
                "suspended_block_time",
                "The time for which a block remains suspended",
                &["authority"],
                registry,
            ).unwrap(),
            block_manager_suspended_blocks: register_int_gauge_with_registry!(
                "block_manager_suspended_blocks",
                "The number of blocks currently suspended in the block manager",
                registry,
            ).unwrap(),
            block_manager_missing_ancestors: register_int_gauge_with_registry!(
                "block_manager_missing_ancestors",
                "The number of missing ancestors tracked in the block manager",
                registry,
            ).unwrap(),
            block_manager_missing_blocks: register_int_gauge_with_registry!(
                "block_manager_missing_blocks",
                "The number of blocks missing content tracked in the block manager",
                registry,
            ).unwrap(),
            block_manager_missing_blocks_by_authority: register_int_counter_vec_with_registry!(
                "block_manager_missing_blocks_by_authority",
                "The number of new missing blocks by block authority",
                &["authority"],
                registry,
            ).unwrap(),
            block_manager_missing_ancestors_by_authority: register_int_counter_vec_with_registry!(
                "block_manager_missing_ancestors_by_authority",
                "The number of missing ancestors by ancestor authority across received blocks",
                &["authority"],
                registry,
            ).unwrap(),
            block_manager_gc_unsuspended_blocks: register_int_counter_vec_with_registry!(
                "block_manager_gc_unsuspended_blocks",
                "The number of blocks unsuspended because their missing ancestors are garbage collected by the block manager, counted by block's source authority",
                &["authority"],
                registry,
            ).unwrap(),
            block_manager_skipped_blocks: register_int_counter_vec_with_registry!(
                "block_manager_skipped_blocks",
                "The number of blocks skipped by the block manager due to block round being <= gc_round",
                &["authority"],
                registry,
            ).unwrap(),
            threshold_clock_round: register_int_gauge_with_registry!(
                "threshold_clock_round",
                "The current threshold clock round. We only advance to a new round when a quorum of parents have been synced.",
                registry,
            ).unwrap(),
            subscriber_connection_attempts: register_int_counter_vec_with_registry!(
                "subscriber_connection_attempts",
                "The number of connection attempts per peer",
                &["authority", "status"],
                registry,
            ).unwrap(),
            subscribed_to: register_int_gauge_vec_with_registry!(
                "subscribed_to",
                "Peers that this authority subscribed to for block streams.",
                &["authority"],
                registry,
            ).unwrap(),
            subscribed_by: register_int_gauge_vec_with_registry!(
                "subscribed_by",
                "Peers subscribing for block streams from this authority.",
                &["authority"],
                registry,
            ).unwrap(),
            commit_sync_inflight_fetches: register_int_gauge_with_registry!(
                "commit_sync_inflight_fetches",
                "The number of inflight fetches in commit syncer",
                registry,
            ).unwrap(),
            commit_sync_pending_fetches: register_int_gauge_with_registry!(
                "commit_sync_pending_fetches",
                "The number of pending fetches in commit syncer",
                registry,
            ).unwrap(),
            commit_sync_fetched_commits: register_int_counter_with_registry!(
                "commit_sync_fetched_commits",
                "The number of commits fetched via commit syncer",
                registry,
            ).unwrap(),
            commit_sync_fetched_blocks: register_int_counter_with_registry!(
                "commit_sync_fetched_blocks",
                "The number of blocks fetched via commit syncer",
                registry,
            ).unwrap(),
            commit_sync_total_fetched_blocks_size: register_int_counter_with_registry!(
                "commit_sync_total_fetched_blocks_size",
                "The total size in bytes of blocks fetched via commit syncer",
                registry,
            ).unwrap(),
            commit_sync_quorum_index: register_int_gauge_with_registry!(
                "commit_sync_quorum_index",
                "The maximum commit index voted by a quorum of authorities",
                registry,
            ).unwrap(),
            commit_sync_highest_synced_index: register_int_gauge_with_registry!(
                "commit_sync_fetched_index",
                "The max commit index among local and fetched commits",
                registry,
            ).unwrap(),
            commit_sync_highest_fetched_index: register_int_gauge_with_registry!(
                "commit_sync_highest_fetched_index",
                "The max commit index that has been fetched via network",
                registry,
            ).unwrap(),
            commit_sync_local_index: register_int_gauge_with_registry!(
                "commit_sync_local_index",
                "The local commit index",
                registry,
            ).unwrap(),
            commit_sync_gap_on_processing: register_int_counter_with_registry!(
                "commit_sync_gap_on_processing",
                "Number of instances where a gap was found in fetched commit processing",
                registry,
            ).unwrap(),
            commit_sync_fetch_loop_latency: register_histogram_with_registry!(
                "commit_sync_fetch_loop_latency",
                "The time taken to finish fetching commits and blocks from a given range",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            commit_sync_fetch_once_latency: register_histogram_with_registry!(
                "commit_sync_fetch_once_latency",
                "The time taken to fetch commits and blocks once",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            commit_sync_fetch_once_errors: register_int_counter_vec_with_registry!(
                "commit_sync_fetch_once_errors",
                "Number of errors when attempting to fetch commits and blocks from single authority during commit sync.",
                &["authority", "error"],
                registry
            ).unwrap(),
            commit_sync_fetch_missing_blocks: register_int_counter_vec_with_registry!(
                "commit_sync_fetch_missing_blocks",
                "Number of ancestor blocks that are missing when processing blocks via commit sync.",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_received_quorum_round_gaps: register_int_gauge_vec_with_registry!(
                "round_prober_received_quorum_round_gaps",
                "Received round gaps among peers for blocks proposed from each authority",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_accepted_quorum_round_gaps: register_int_gauge_vec_with_registry!(
                "round_prober_accepted_quorum_round_gaps",
                "Accepted round gaps among peers for blocks proposed & accepted from each authority",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_low_received_quorum_round: register_int_gauge_vec_with_registry!(
                "round_prober_low_received_quorum_round",
                "Low quorum round among peers for blocks proposed from each authority",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_low_accepted_quorum_round: register_int_gauge_vec_with_registry!(
                "round_prober_low_accepted_quorum_round",
                "Low quorum round among peers for blocks proposed & accepted from each authority",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_current_received_round_gaps: register_int_gauge_vec_with_registry!(
                "round_prober_current_received_round_gaps",
                "Received round gaps from local last proposed round to the low received quorum round of each peer. Can be negative.",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_current_accepted_round_gaps: register_int_gauge_vec_with_registry!(
                "round_prober_current_accepted_round_gaps",
                "Accepted round gaps from local last proposed & accepted round to the low accepted quorum round of each peer. Can be negative.",
                &["authority"],
                registry
            ).unwrap(),
            round_prober_propagation_delays: register_histogram_with_registry!(
                "round_prober_propagation_delays",
                "Round gaps between the last proposed block round and the lower bound of own quorum round",
                NUM_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            round_prober_last_propagation_delay: register_int_gauge_with_registry!(
                "round_prober_last_propagation_delay",
                "Most recent propagation delay observed by RoundProber",
                registry
            ).unwrap(),
            round_prober_request_errors: register_int_counter_vec_with_registry!(
                "round_prober_request_errors",
                "Number of errors when probing against peers per error type",
                &["error_type"],
                registry
            ).unwrap(),
            uptime: register_histogram_with_registry!(
                "uptime",
                "Total node uptime",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
        }
    }
}

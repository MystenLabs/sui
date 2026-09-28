// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Histogram;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::IntGaugeVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_histogram_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_vec_with_registry;

/// Histogram buckets for checkpoint timestamp lag, in seconds.
const LAG_SEC_BUCKETS: &[f64] = &[
    0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9,
    0.95, 1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 20.0, 50.0, 100.0, 1000.0,
];

/// Histogram buckets for subscription lifetime, in seconds: sub-second churn through multi-hour
/// holds, with day/week edges to size the long-lived tail (subscriptions may run indefinitely).
const DURATION_SEC_BUCKETS: &[f64] = &[
    1.0, 5.0, 15.0, 30.0, // seconds
    60.0, 300.0, 900.0, 1800.0, // 1m, 5m, 15m, 30m
    3600.0, 7200.0, 21600.0, // 1h, 2h, 6h
    43200.0, 86400.0, // 12h, 24h
    172800.0, 604800.0, // 2d, 7d
];

/// Histogram buckets for the per-payload throttle pause, in seconds: sub-millisecond when the rate
/// budget is generous, up to a few seconds for a heavy payload against a tight budget.
const THROTTLE_DELAY_SEC_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
];

/// Metrics specific to the streaming subscription feature. Subscription payloads also record into
/// the shared field-resolution (`fields_*`) and query-complexity (`input_nodes`, `output_nodes`)
/// metrics on `RpcMetrics`.
pub struct SubscriptionMetrics {
    // Metrics for the shared upstream checkpoint stream.
    pub upstream_connection_failures: IntCounterVec,
    pub upstream_disconnections: IntCounter,
    pub upstream_terminations: IntCounterVec,
    pub upstream_processed_checkpoints: IntCounterVec,
    pub upstream_latest_processed_checkpoint: IntGaugeVec,
    pub upstream_processed_checkpoint_timestamp_lag: HistogramVec,
    pub upstream_latest_processed_checkpoint_timestamp_ms: IntGaugeVec,
    pub upstream_gap_recoveries: IntCounter,
    pub upstream_malformed_checkpoints: IntCounter,

    // Metrics aggregated across all subscribers.
    pub active_subscriptions: IntGaugeVec,
    pub subscriptions_opened: IntCounterVec,
    pub subscriptions_rejected: IntCounterVec,
    pub subscription_terminations: IntCounterVec,
    pub subscription_duration: HistogramVec,
    pub payloads_delivered: IntCounterVec,
    pub live_payload_delivery_checkpoint_timestamp_lag: HistogramVec,
    pub subscriber_throttle_delay: Histogram,
}

impl SubscriptionMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            upstream_connection_failures: register_int_counter_vec_with_registry!(
                "graphql_subscription_upstream_connection_failures",
                "Total failed attempts to connect to the upstream checkpoint stream, by gRPC status code",
                &["code"],
                registry,
            )
            .unwrap(),
            upstream_disconnections: register_int_counter_with_registry!(
                "graphql_subscription_upstream_disconnections",
                "Total times an established upstream checkpoint stream dropped and reconnected",
                registry,
            )
            .unwrap(),
            upstream_terminations: register_int_counter_vec_with_registry!(
                "graphql_subscription_upstream_terminations",
                "Total permanent terminations of the shared checkpoint stream task, by reason",
                &["reason"],
                registry,
            )
            .unwrap(),
            upstream_processed_checkpoints: register_int_counter_vec_with_registry!(
                "graphql_subscription_upstream_processed_checkpoints",
                "Total checkpoints decoded from the upstream stream and broadcast to subscribers, by phase",
                &["phase"],
                registry,
            )
            .unwrap(),
            upstream_latest_processed_checkpoint: register_int_gauge_vec_with_registry!(
                "graphql_subscription_upstream_latest_processed_checkpoint",
                "Highest checkpoint sequence number decoded from the upstream stream, by phase",
                &["phase"],
                registry,
            )
            .unwrap(),
            upstream_processed_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "graphql_subscription_upstream_processed_checkpoint_timestamp_lag",
                "Wall-clock lag of each checkpoint decoded from the upstream stream, in seconds, by phase",
                &["phase"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            upstream_latest_processed_checkpoint_timestamp_ms: register_int_gauge_vec_with_registry!(
                "graphql_subscription_upstream_latest_processed_checkpoint_timestamp_ms",
                "Timestamp in milliseconds of the latest checkpoint decoded from the upstream stream, by phase",
                &["phase"],
                registry,
            )
            .unwrap(),
            upstream_gap_recoveries: register_int_counter_with_registry!(
                "graphql_subscription_upstream_gap_recoveries",
                "Total gap-recovery passes run to backfill checkpoints missed between the last broadcast checkpoint and a live message",
                registry,
            )
            .unwrap(),
            upstream_malformed_checkpoints: register_int_counter_with_registry!(
                "graphql_subscription_upstream_malformed_checkpoints",
                "Total checkpoints from the live upstream stream that failed to decode",
                registry,
            )
            .unwrap(),
            active_subscriptions: register_int_gauge_vec_with_registry!(
                "graphql_subscription_active_subscriptions",
                "Number of subscriptions currently active, by type",
                &["type"],
                registry,
            )
            .unwrap(),
            subscriptions_opened: register_int_counter_vec_with_registry!(
                "graphql_subscription_opened",
                "Total subscriptions opened, by type",
                &["type"],
                registry,
            )
            .unwrap(),
            subscriptions_rejected: register_int_counter_vec_with_registry!(
                "graphql_subscription_rejected",
                "Total subscriptions refused before opening, by type and reason (e.g. the server was at its concurrent-subscription capacity)",
                &["type", "reason"],
                registry,
            )
            .unwrap(),
            subscription_terminations: register_int_counter_vec_with_registry!(
                "graphql_subscription_terminations",
                "Total subscriptions that have ended, by type and reason",
                &["type", "reason"],
                registry,
            )
            .unwrap(),
            subscription_duration: register_histogram_vec_with_registry!(
                "graphql_subscription_duration_seconds",
                "Lifetime of each subscription from open to close, in seconds, by type",
                &["type"],
                DURATION_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            payloads_delivered: register_int_counter_vec_with_registry!(
                "graphql_subscription_payloads_delivered",
                "Total payloads delivered to subscribers, by type and phase (live or backfill)",
                &["type", "phase"],
                registry,
            )
            .unwrap(),
            live_payload_delivery_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "graphql_subscription_live_payload_delivery_checkpoint_timestamp_lag",
                "Seconds from a live payload's checkpoint timestamp to when it was delivered to a subscriber, by type (server-side; backfill deliveries excluded, since their lag is catch-up distance not staleness)",
                &["type"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            subscriber_throttle_delay: register_histogram_with_registry!(
                "graphql_subscription_throttle_delay_seconds",
                "Seconds the delivery throttle paused before the next payload, observed once per payload across all subscribers and phases (zero while the rate budget is not binding)",
                THROTTLE_DELAY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }

    /// Record a failed attempt to connect to the upstream stream, labelled by gRPC status code (or
    /// `"transport"` when it isn't a `tonic::Status`).
    pub(crate) fn record_connect_failure(&self, e: &anyhow::Error) {
        let code = e
            .downcast_ref::<tonic::Status>()
            .map(|s| format!("{:?}", s.code()))
            .unwrap_or_else(|| "transport".to_string());
        self.upstream_connection_failures
            .with_label_values(&[&code])
            .inc();
    }

    /// Record a permanent termination of the shared stream task, by `reason`.
    pub(crate) fn record_termination(&self, reason: &str) {
        self.upstream_terminations
            .with_label_values(&[reason])
            .inc();
    }

    /// Record a checkpoint processed by the upstream stream under `phase` (`live` or `recovery`):
    /// bump the count, and set the sequence number, raw `timestamp_ms` (epoch milliseconds), and
    /// wall-clock lag of the most recent one.
    pub(crate) fn record_processed_checkpoint(
        &self,
        phase: &str,
        sequence_number: u64,
        timestamp_ms: u64,
    ) {
        self.upstream_processed_checkpoints
            .with_label_values(&[phase])
            .inc();
        self.upstream_latest_processed_checkpoint
            .with_label_values(&[phase])
            .set(sequence_number as i64);
        self.upstream_latest_processed_checkpoint_timestamp_ms
            .with_label_values(&[phase])
            .set(timestamp_ms as i64);
        let lag_ms = chrono::Utc::now().timestamp_millis() - timestamp_ms as i64;
        self.upstream_processed_checkpoint_timestamp_lag
            .with_label_values(&[phase])
            .observe(lag_ms as f64 / 1000.0);
    }

    #[cfg(test)]
    pub(crate) fn new_for_test() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new(&Registry::new()))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    #[test]
    fn records_connect_failure_by_code() {
        let metrics = SubscriptionMetrics::new(&Registry::new());

        // A gRPC status, wrapped in `.context()` as the real connect path produces it.
        let status = Err::<(), _>(tonic::Status::unavailable("down"))
            .context("Failed to connect")
            .unwrap_err();
        metrics.record_connect_failure(&status);
        assert_eq!(
            metrics
                .upstream_connection_failures
                .with_label_values(&["Unavailable"])
                .get(),
            1
        );

        // A non-gRPC transport error falls back to "transport".
        metrics.record_connect_failure(&anyhow::anyhow!("connection refused"));
        assert_eq!(
            metrics
                .upstream_connection_failures
                .with_label_values(&["transport"])
                .get(),
            1
        );
    }
}

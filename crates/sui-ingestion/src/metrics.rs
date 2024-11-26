// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Histogram, IntCounter,
    IntCounterVec, IntGauge, Registry,
};
use tracing::warn;

use crate::error::Error;

/// Histogram buckets for the distribution of checkpoint fetching latencies.
const INGESTION_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
];

/// Histogram buckets for the distribution of checkpoint lag (difference between the system time and
/// the timestamp in the checkpoint).
const LAG_SEC_BUCKETS: &[f64] = &[
    0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9,
    0.95, 1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 20.0, 50.0, 100.0, 1000.0,
];

#[derive(Clone)]
pub struct IngestionMetrics {
    // Statistics related to fetching data from the remote store.
    pub total_ingested_checkpoints: IntCounter,
    pub total_ingested_transactions: IntCounter,
    pub total_ingested_events: IntCounter,
    pub total_ingested_inputs: IntCounter,
    pub total_ingested_outputs: IntCounter,
    pub total_ingested_bytes: IntCounter,
    total_ingested_transient_retries: IntCounterVec,
    pub total_ingested_not_found_retries: IntCounter,

    pub latest_ingested_checkpoint: IntGauge,
    pub latest_ingested_checkpoint_timestamp_lag_ms: IntGauge,
    pub ingested_checkpoint_timestamp_lag: Histogram,

    pub ingested_checkpoint_latency: Histogram,
}

impl IngestionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_ingested_checkpoints: register_int_counter_with_registry!(
                "indexer_total_ingested_checkpoints",
                "Total number of checkpoints fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_transactions: register_int_counter_with_registry!(
                "indexer_total_ingested_transactions",
                "Total number of transactions fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_events: register_int_counter_with_registry!(
                "indexer_total_ingested_events",
                "Total number of events fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_inputs: register_int_counter_with_registry!(
                "indexer_total_ingested_inputs",
                "Total number of input objects fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_outputs: register_int_counter_with_registry!(
                "indexer_total_ingested_outputs",
                "Total number of output objects fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_bytes: register_int_counter_with_registry!(
                "indexer_total_ingested_bytes",
                "Total number of bytes fetched from the remote store",
                registry,
            )
            .unwrap(),
            total_ingested_transient_retries: register_int_counter_vec_with_registry!(
                "indexer_total_ingested_retries",
                "Total number of retries due to transient errors while fetching data from the \
                 remote store",
                &["reason"],
                registry,
            )
            .unwrap(),
            total_ingested_not_found_retries: register_int_counter_with_registry!(
                "indexer_total_ingested_not_found_retries",
                "Total number of retries due to the not found errors while fetching data from the \
                 remote store",
                registry,
            )
            .unwrap(),
            latest_ingested_checkpoint: register_int_gauge_with_registry!(
                "indexer_latest_ingested_checkpoint",
                "Latest checkpoint sequence number fetched from the remote store",
                registry,
            )
            .unwrap(),
            latest_ingested_checkpoint_timestamp_lag_ms: register_int_gauge_with_registry!(
                "latest_ingested_checkpoint_timestamp_lag_ms",
                "Difference between the system timestamp when the latest checkpoint was fetched and the \
                 timestamp in the checkpoint, in milliseconds",
                registry,
            )
            .unwrap(),
            ingested_checkpoint_timestamp_lag: register_histogram_with_registry!(
                "indexer_ingested_checkpoint_timestamp_lag",
                "Difference between the system timestamp when a checkpoint was fetched and the \
                 timestamp in each checkpoint, in seconds",
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            ingested_checkpoint_latency: register_histogram_with_registry!(
                "indexer_ingested_checkpoint_latency",
                "Time taken to fetch a checkpoint from the remote store, including retries",
                INGESTION_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }

    /// Register that we're retrying a checkpoint fetch due to a transient error, logging the
    /// reason and error.
    pub(crate) fn inc_retry(
        &self,
        checkpoint: u64,
        reason: &str,
        error: Error,
    ) -> backoff::Error<Error> {
        warn!(checkpoint, reason, "Retrying due to error: {error}");

        self.total_ingested_transient_retries
            .with_label_values(&[reason])
            .inc();

        backoff::Error::transient(error)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use prometheus::Registry;

    use super::IngestionMetrics;

    /// Construct metrics for test purposes.
    pub fn test_metrics() -> IngestionMetrics {
        IngestionMetrics::new(&Registry::new())
    }
}

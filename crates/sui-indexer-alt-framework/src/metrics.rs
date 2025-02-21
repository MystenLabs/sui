// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{atomic::AtomicU64, Arc};

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry,
};
use tracing::warn;

use crate::{ingestion::error::Error, pipeline::Processor};

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

/// Histogram buckets for the distribution of latencies for processing a checkpoint in the indexer
/// (without having to call out to other services).
const PROCESSING_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
];

/// Histogram buckets for the distribution of latencies for writing to the database.
const DB_UPDATE_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0,
    2000.0, 5000.0, 10000.0,
];

/// Histogram buckets for the distribution of batch sizes (number of rows) written to the database.
const BATCH_SIZE_BUCKETS: &[f64] = &[
    1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
];

#[derive(Clone)]
pub(crate) struct IndexerMetrics {
    // Statistics related to fetching data from the remote store.
    pub total_ingested_checkpoints: IntCounter,
    pub total_ingested_transactions: IntCounter,
    pub total_ingested_events: IntCounter,
    pub total_ingested_inputs: IntCounter,
    pub total_ingested_outputs: IntCounter,
    pub total_ingested_bytes: IntCounter,
    pub total_ingested_transient_retries: IntCounterVec,
    pub total_ingested_not_found_retries: IntCounter,

    // Checkpoint lag metrics for the ingestion pipeline.
    pub latest_ingested_checkpoint: IntGauge,
    pub latest_ingested_checkpoint_timestamp_lag_ms: IntGauge,
    pub ingested_checkpoint_timestamp_lag: Histogram,

    pub ingested_checkpoint_latency: Histogram,

    // Statistics related to individual ingestion pipelines' handlers.
    pub total_handler_checkpoints_received: IntCounterVec,
    pub total_handler_checkpoints_processed: IntCounterVec,
    pub total_handler_rows_created: IntCounterVec,

    pub latest_processed_checkpoint: IntGaugeVec,
    pub latest_processed_checkpoint_timestamp_lag_ms: IntGaugeVec,
    pub processed_checkpoint_timestamp_lag: HistogramVec,

    pub handler_checkpoint_latency: HistogramVec,

    // Statistics related to individual ingestion pipelines.
    pub total_collector_checkpoints_received: IntCounterVec,
    pub total_collector_rows_received: IntCounterVec,
    pub total_collector_batches_created: IntCounterVec,
    pub total_committer_batches_attempted: IntCounterVec,
    pub total_committer_batches_succeeded: IntCounterVec,
    pub total_committer_batches_failed: IntCounterVec,
    pub total_committer_rows_committed: IntCounterVec,
    pub total_committer_rows_affected: IntCounterVec,
    pub total_watermarks_out_of_order: IntCounterVec,
    pub total_pruner_chunks_attempted: IntCounterVec,
    pub total_pruner_chunks_deleted: IntCounterVec,
    pub total_pruner_rows_deleted: IntCounterVec,

    // Checkpoint lag metrics for the collector.
    pub latest_collected_checkpoint: IntGaugeVec,
    pub latest_collected_checkpoint_timestamp_lag_ms: IntGaugeVec,
    pub collected_checkpoint_timestamp_lag: HistogramVec,

    // Checkpoint lag metrics for the committer.
    // We can only report partially committed checkpoints, since the concurrent committer isn't aware of
    // when a checkpoint is fully committed. So we report whenever we see a checkpoint. Since data from
    // the same checkpoint is batched continuously, this is a good proxy for the last committed checkpoint.
    pub latest_partially_committed_checkpoint: IntGaugeVec,
    pub latest_partially_committed_checkpoint_timestamp_lag_ms: IntGaugeVec,
    pub partially_committed_checkpoint_timestamp_lag: HistogramVec,

    // Checkpoint lag metrics for the watermarker.
    // The latest watermarked checkpoint metric is already covered by watermark_checkpoint_in_db.
    // While we already have watermark_timestamp_in_db_ms metric, reporting the lag explicitly
    // for consistency.
    pub latest_watermarked_checkpoint_timestamp_lag_ms: IntGaugeVec,
    pub watermarked_checkpoint_timestamp_lag: HistogramVec,

    pub collector_gather_latency: HistogramVec,
    pub collector_batch_size: HistogramVec,
    pub committer_commit_latency: HistogramVec,
    pub committer_tx_rows: HistogramVec,
    pub watermark_gather_latency: HistogramVec,
    pub watermark_commit_latency: HistogramVec,
    pub watermark_pruner_read_latency: HistogramVec,
    pub watermark_pruner_write_latency: HistogramVec,
    pub pruner_delete_latency: HistogramVec,

    pub watermark_epoch: IntGaugeVec,
    pub watermark_checkpoint: IntGaugeVec,
    pub watermark_transaction: IntGaugeVec,
    pub watermark_timestamp_ms: IntGaugeVec,
    pub watermark_reader_lo: IntGaugeVec,
    pub watermark_pruner_hi: IntGaugeVec,

    pub watermark_epoch_in_db: IntGaugeVec,
    pub watermark_checkpoint_in_db: IntGaugeVec,
    pub watermark_transaction_in_db: IntGaugeVec,
    pub watermark_timestamp_in_db_ms: IntGaugeVec,
    pub watermark_reader_lo_in_db: IntGaugeVec,
    pub watermark_pruner_hi_in_db: IntGaugeVec,
}

/// A helper struct to report metrics regarding the checkpoint lag at various points in the indexer.
pub(crate) struct CheckpointLagMetricReporter {
    /// Metric to report the lag distribution of each checkpoint.
    checkpoint_time_lag_histogram: Histogram,
    /// Metric to report the lag of the checkpoint with the highest sequence number observed so far.
    /// This is needed since concurrent pipelines observe checkpoints out of order.
    latest_checkpoint_time_lag_gauge: IntGauge,
    /// Metric to report the sequence number of the checkpoint with the highest sequence number observed so far.
    latest_checkpoint_sequence_number_gauge: IntGauge,
    // Internal state to keep track of the highest checkpoint sequence number reported so far.
    latest_reported_checkpoint: AtomicU64,
}

impl IndexerMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
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
            total_handler_checkpoints_received: register_int_counter_vec_with_registry!(
                "indexer_total_handler_checkpoints_received",
                "Total number of checkpoints received by this handler",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_handler_checkpoints_processed: register_int_counter_vec_with_registry!(
                "indexer_total_handler_checkpoints_processed",
                "Total number of checkpoints processed (converted into rows) by this handler",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_handler_rows_created: register_int_counter_vec_with_registry!(
                "indexer_total_handler_rows_created",
                "Total number of rows created by this handler",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_processed_checkpoint: register_int_gauge_vec_with_registry!(
                "indexer_latest_processed_checkpoint",
                "Latest checkpoint sequence number processed by this handler",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_processed_checkpoint_timestamp_lag_ms: register_int_gauge_vec_with_registry!(
                "indexer_latest_processed_checkpoint_timestamp_lag_ms",
                "Difference between the system timestamp when the latest checkpoint was processed and the \
                 timestamp in the checkpoint, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            processed_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "indexer_processed_checkpoint_timestamp_lag",
                "Difference between the system timestamp when a checkpoint was processed and the \
                 timestamp in each checkpoint, in seconds",
                &["pipeline"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handler_checkpoint_latency: register_histogram_vec_with_registry!(
                "indexer_handler_checkpoint_latency",
                "Time taken to process a checkpoint by this handler",
                &["pipeline"],
                PROCESSING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_collector_checkpoints_received: register_int_counter_vec_with_registry!(
                "indexer_total_collector_checkpoints_received",
                "Total number of checkpoints received by this collector",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_collector_rows_received: register_int_counter_vec_with_registry!(
                "indexer_total_collector_rows_received",
                "Total number of rows received by this collector",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_collector_batches_created: register_int_counter_vec_with_registry!(
                "indexer_total_collector_batches_created",
                "Total number of batches created by this collector",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_committer_batches_attempted: register_int_counter_vec_with_registry!(
                "indexer_total_committer_batches_attempted",
                "Total number of batches writes attempted by this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_committer_batches_succeeded: register_int_counter_vec_with_registry!(
                "indexer_total_committer_batches_succeeded",
                "Total number of successful batches writes by this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_committer_batches_failed: register_int_counter_vec_with_registry!(
                "indexer_total_committer_batches_failed",
                "Total number of failed batches writes by this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_committer_rows_committed: register_int_counter_vec_with_registry!(
                "indexer_total_committer_rows_committed",
                "Total number of rows sent to the database by this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_committer_rows_affected: register_int_counter_vec_with_registry!(
                "indexer_total_committer_rows_affected",
                "Total number of rows actually written to the database by this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_watermarks_out_of_order: register_int_counter_vec_with_registry!(
                "indexer_watermark_out_of_order",
                "Number of times this committer encountered a batch for a checkpoint before its watermark",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_pruner_chunks_attempted: register_int_counter_vec_with_registry!(
                "indexer_pruner_chunks_attempted",
                "Number of chunks this pruner attempted to delete",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_pruner_chunks_deleted: register_int_counter_vec_with_registry!(
                "indexer_pruner_chunks_deleted",
                "Number of chunks this pruner successfully deleted",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            total_pruner_rows_deleted: register_int_counter_vec_with_registry!(
                "indexer_pruner_rows_deleted",
                "Number of rows this pruner successfully deleted",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_collected_checkpoint: register_int_gauge_vec_with_registry!(
                "indexer_latest_collected_checkpoint",
                "Latest checkpoint sequence number collected by this collector",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_collected_checkpoint_timestamp_lag_ms: register_int_gauge_vec_with_registry!(
                "indexer_latest_collected_checkpoint_timestamp_lag_ms",
                "Difference between the system timestamp when the latest checkpoint was collected and the \
                 timestamp in the checkpoint, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            collected_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "indexer_collected_checkpoint_timestamp_lag",
                "Difference between the system timestamp when a checkpoint was collected and the \
                 timestamp in each checkpoint, in seconds",
                &["pipeline"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latest_partially_committed_checkpoint: register_int_gauge_vec_with_registry!(
                "indexer_latest_partially_committed_checkpoint",
                "Latest checkpoint sequence number partially committed by this collector",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            latest_partially_committed_checkpoint_timestamp_lag_ms: register_int_gauge_vec_with_registry!(
                "indexer_latest_partially_committed_checkpoint_timestamp_lag_ms",
                "Difference between the system timestamp when the latest checkpoint was partially committed and the \
                 timestamp in the checkpoint, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            partially_committed_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "indexer_partially_committed_checkpoint_timestamp_lag",
                "Difference between the system timestamp when a checkpoint was partially committed and the \
                 timestamp in each checkpoint, in seconds",
                &["pipeline"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            latest_watermarked_checkpoint_timestamp_lag_ms: register_int_gauge_vec_with_registry!(
                "indexer_latest_watermarked_checkpoint_timestamp_lag_ms",
                "Difference between the system timestamp when the latest checkpoint was watermarked and the \
                 timestamp in the checkpoint, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermarked_checkpoint_timestamp_lag: register_histogram_vec_with_registry!(
                "indexer_watermarked_checkpoint_timestamp_lag",
                "Difference between the system timestamp when a checkpoint was watermarked and the \
                 timestamp in each checkpoint, in seconds",
                &["pipeline"],
                LAG_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            collector_gather_latency: register_histogram_vec_with_registry!(
                "indexer_collector_gather_latency",
                "Time taken to gather rows into a batch by this collector",
                &["pipeline"],
                PROCESSING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            collector_batch_size: register_histogram_vec_with_registry!(
                "indexer_collector_batch_size",
                "Number of rows in a batch written to the database by this collector",
                &["pipeline"],
                BATCH_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            committer_commit_latency: register_histogram_vec_with_registry!(
                "indexer_committer_commit_latency",
                "Time taken to write a batch of rows to the database by this committer",
                &["pipeline"],
                DB_UPDATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            committer_tx_rows: register_histogram_vec_with_registry!(
                "indexer_committer_tx_rows",
                "Number of rows written to the database in a single database transaction by this committer",
                &["pipeline"],
                BATCH_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            watermark_gather_latency: register_histogram_vec_with_registry!(
                "indexer_watermark_gather_latency",
                "Time taken to calculate the new high watermark after a write by this committer",
                &["pipeline"],
                PROCESSING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            watermark_commit_latency: register_histogram_vec_with_registry!(
                "indexer_watermark_commit_latency",
                "Time taken to write the new high watermark to the database by this committer",
                &["pipeline"],
                DB_UPDATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            watermark_pruner_read_latency: register_histogram_vec_with_registry!(
                "indexer_watermark_pruner_read_latency",
                "Time taken to read pruner's next upper and lowerbounds from the database by this pruner",
                &["pipeline"],
                DB_UPDATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            watermark_pruner_write_latency: register_histogram_vec_with_registry!(
                "indexer_watermark_pruner_write_latency",
                "Time taken to write the pruner's new upperbound to the database by this pruner",
                &["pipeline"],
                DB_UPDATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            pruner_delete_latency: register_histogram_vec_with_registry!(
                "indexer_pruner_delete_latency",
                "Time taken to delete a chunk of data from the database by this pruner",
                &["pipeline"],
                DB_UPDATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            watermark_epoch: register_int_gauge_vec_with_registry!(
                "indexer_watermark_epoch",
                "Current epoch high watermark for this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_checkpoint: register_int_gauge_vec_with_registry!(
                "indexer_watermark_checkpoint",
                "Current checkpoint high watermark for this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_transaction: register_int_gauge_vec_with_registry!(
                "indexer_watermark_transaction",
                "Current transaction high watermark for this committer",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_timestamp_ms: register_int_gauge_vec_with_registry!(
                "indexer_watermark_timestamp_ms",
                "Current timestamp high watermark for this committer, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_reader_lo: register_int_gauge_vec_with_registry!(
                "indexer_watermark_reader_lo",
                "Current reader low watermark for this pruner",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_pruner_hi: register_int_gauge_vec_with_registry!(
                "indexer_watermark_pruner_hi",
                "Current pruner high watermark for this pruner",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_epoch_in_db: register_int_gauge_vec_with_registry!(
                "indexer_watermark_epoch_in_db",
                "Last epoch high watermark this committer wrote to the DB",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_checkpoint_in_db: register_int_gauge_vec_with_registry!(
                "indexer_watermark_checkpoint_in_db",
                "Last checkpoint high watermark this committer wrote to the DB",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_transaction_in_db: register_int_gauge_vec_with_registry!(
                "indexer_watermark_transaction_in_db",
                "Last transaction high watermark this committer wrote to the DB",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_timestamp_in_db_ms: register_int_gauge_vec_with_registry!(
                "indexer_watermark_timestamp_ms_in_db",
                "Last timestamp high watermark this committer wrote to the DB, in milliseconds",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_reader_lo_in_db: register_int_gauge_vec_with_registry!(
                "indexer_watermark_reader_lo_in_db",
                "Last reader low watermark this pruner wrote to the DB",
                &["pipeline"],
                registry,
            )
            .unwrap(),
            watermark_pruner_hi_in_db: register_int_gauge_vec_with_registry!(
                "indexer_watermark_pruner_hi_in_db",
                "Last pruner high watermark this pruner wrote to the DB",
                &["pipeline"],
                registry,
            )
            .unwrap(),
        })
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

impl CheckpointLagMetricReporter {
    pub fn new(
        checkpoint_time_lag_histogram: Histogram,
        latest_checkpoint_time_lag_gauge: IntGauge,
        latest_checkpoint_sequence_number_gauge: IntGauge,
    ) -> Arc<Self> {
        Arc::new(Self {
            checkpoint_time_lag_histogram,
            latest_checkpoint_time_lag_gauge,
            latest_checkpoint_sequence_number_gauge,
            latest_reported_checkpoint: AtomicU64::new(0),
        })
    }

    pub fn new_for_pipeline<P: Processor>(
        checkpoint_time_lag_histogram: &HistogramVec,
        latest_checkpoint_time_lag_gauge: &IntGaugeVec,
        latest_checkpoint_sequence_number_gauge: &IntGaugeVec,
    ) -> Arc<Self> {
        Self::new(
            checkpoint_time_lag_histogram.with_label_values(&[P::NAME]),
            latest_checkpoint_time_lag_gauge.with_label_values(&[P::NAME]),
            latest_checkpoint_sequence_number_gauge.with_label_values(&[P::NAME]),
        )
    }

    pub fn report_lag(&self, cp_sequence_number: u64, checkpoint_timestamp_ms: u64) {
        let lag = chrono::Utc::now().timestamp_millis() - checkpoint_timestamp_ms as i64;
        self.checkpoint_time_lag_histogram
            .observe((lag as f64) / 1000.0);

        let prev = self
            .latest_reported_checkpoint
            .fetch_max(cp_sequence_number, std::sync::atomic::Ordering::Relaxed);
        if cp_sequence_number > prev {
            self.latest_checkpoint_sequence_number_gauge
                .set(cp_sequence_number as i64);
            self.latest_checkpoint_time_lag_gauge.set(lag);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use prometheus::Registry;

    use super::IndexerMetrics;

    /// Construct metrics for test purposes.
    pub fn test_metrics() -> Arc<IndexerMetrics> {
        IndexerMetrics::new(&Registry::new())
    }
}

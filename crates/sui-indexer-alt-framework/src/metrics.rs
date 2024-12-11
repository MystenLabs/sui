// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use prometheus::{
    core::{Collector, Desc},
    proto::{Counter, Gauge, LabelPair, Metric, MetricFamily, MetricType, Summary},
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry, TextEncoder,
};
use sui_pg_db::Db;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::ingestion::error::Error;

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

/// Service to expose prometheus metrics from the indexer.
pub(crate) struct MetricsService {
    addr: SocketAddr,
    registry: Registry,
    cancel: CancellationToken,
}

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

/// Collects information about the database connection pool.
struct DbConnectionStatsCollector {
    db: Db,
    desc: Vec<(MetricType, Desc)>,
}

impl MetricsService {
    /// Create a new metrics service, exposing Mysten-wide metrics, and Indexer-specific metrics.
    /// Returns the Indexer-specific metrics and the service itself (which must be run with
    /// [Self::run]).
    pub(crate) fn new(
        addr: SocketAddr,
        db: Db,
        cancel: CancellationToken,
    ) -> Result<(Arc<IndexerMetrics>, MetricsService)> {
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None)?;

        let metrics = IndexerMetrics::new(&registry);
        registry.register(Box::new(DbConnectionStatsCollector::new(db)))?;

        let service = Self {
            addr,
            registry,
            cancel,
        };

        Ok((Arc::new(metrics), service))
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub(crate) async fn run(self) -> Result<JoinHandle<()>> {
        let listener = TcpListener::bind(&self.addr).await?;
        let app = Router::new()
            .route("/metrics", get(metrics))
            .layer(Extension(self.registry));

        Ok(tokio::spawn(async move {
            info!("Starting metrics service on {}", self.addr);
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    self.cancel.cancelled().await;
                    info!("Shutdown received, stopping metrics service");
                })
                .await
                .unwrap();
        }))
    }
}

impl IndexerMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
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

impl DbConnectionStatsCollector {
    fn new(db: Db) -> Self {
        let desc = vec![
            (
                MetricType::GAUGE,
                desc(
                    "db_connections",
                    "Number of connections currently being managed by the pool",
                ),
            ),
            (
                MetricType::GAUGE,
                desc(
                    "db_idle_connections",
                    "Number of idle connections in the pool",
                ),
            ),
            (
                MetricType::COUNTER,
                desc("db_connect_direct", "Connections that did not have to wait"),
            ),
            (
                MetricType::SUMMARY,
                desc("db_connect_waited", "Connections that had to wait"),
            ),
            (
                MetricType::COUNTER,
                desc(
                    "db_connect_timed_out",
                    "Connections that timed out waiting for a connection",
                ),
            ),
            (
                MetricType::COUNTER,
                desc(
                    "db_connections_created",
                    "Connections that have been created in the pool",
                ),
            ),
            (
                MetricType::COUNTER,
                desc_with_labels(
                    "db_connections_closed",
                    "Total connections that were closed",
                    &["reason"],
                ),
            ),
        ];

        Self { db, desc }
    }
}

impl Collector for DbConnectionStatsCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.desc.iter().map(|d| &d.1).collect()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        let state = self.db.state();
        let stats = state.statistics;

        vec![
            gauge(&self.desc[0].1, state.connections as f64),
            gauge(&self.desc[1].1, state.idle_connections as f64),
            counter(&self.desc[2].1, stats.get_direct as f64),
            summary(
                &self.desc[3].1,
                stats.get_wait_time.as_millis() as f64,
                stats.get_waited + stats.get_timed_out,
            ),
            counter(&self.desc[4].1, stats.get_timed_out as f64),
            counter(&self.desc[5].1, stats.connections_created as f64),
            counter_with_labels(
                &self.desc[6].1,
                &[
                    ("reason", "broken", stats.connections_closed_broken as f64),
                    ("reason", "invalid", stats.connections_closed_invalid as f64),
                    (
                        "reason",
                        "max_lifetime",
                        stats.connections_closed_max_lifetime as f64,
                    ),
                    (
                        "reason",
                        "idle_timeout",
                        stats.connections_closed_idle_timeout as f64,
                    ),
                ],
            ),
        ]
    }
}

/// Route handler for metrics service
async fn metrics(Extension(registry): Extension<Registry>) -> (StatusCode, String) {
    match TextEncoder.encode_to_string(&registry.gather()) {
        Ok(s) => (StatusCode::OK, s),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encoding metrics: {e}"),
        ),
    }
}

fn desc(name: &str, help: &str) -> Desc {
    desc_with_labels(name, help, &[])
}

fn desc_with_labels(name: &str, help: &str, labels: &[&str]) -> Desc {
    Desc::new(
        name.to_string(),
        help.to_string(),
        labels.iter().map(|s| s.to_string()).collect(),
        Default::default(),
    )
    .expect("Bad metric description")
}

fn gauge(desc: &Desc, value: f64) -> MetricFamily {
    let mut g = Gauge::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    g.set_value(value);
    m.set_gauge(g);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::COUNTER);
    mf
}

fn counter(desc: &Desc, value: f64) -> MetricFamily {
    let mut c = Counter::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    c.set_value(value);
    m.set_counter(c);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::GAUGE);
    mf
}

fn counter_with_labels(desc: &Desc, values: &[(&str, &str, f64)]) -> MetricFamily {
    let mut mf = MetricFamily::new();

    for (name, label, value) in values {
        let mut c = Counter::default();
        let mut l = LabelPair::default();
        let mut m = Metric::default();

        c.set_value(*value);
        l.set_name(name.to_string());
        l.set_value(label.to_string());

        m.set_counter(c);
        m.mut_label().push(l);
        mf.mut_metric().push(m);
    }

    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::COUNTER);
    mf
}

fn summary(desc: &Desc, sum: f64, count: u64) -> MetricFamily {
    let mut s = Summary::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    s.set_sample_sum(sum);
    s.set_sample_count(count);
    m.set_summary(s);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::SUMMARY);
    mf
}

#[cfg(test)]
pub(crate) mod tests {
    use prometheus::Registry;

    use super::IndexerMetrics;

    /// Construct metrics for test purposes.
    pub fn test_metrics() -> IndexerMetrics {
        IndexerMetrics::new(&Registry::new())
    }
}

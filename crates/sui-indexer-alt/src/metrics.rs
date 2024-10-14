// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use anyhow::Result;
use axum::{extract::Extension, routing::get, Router};
use mysten_metrics::RegistryService;
use prometheus::{
    core::{Collector, Desc},
    proto::{Counter, Gauge, LabelPair, Metric, MetricFamily, MetricType, Summary},
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Histogram, IntCounter, IntCounterVec, Registry,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{db::Db, ingestion::error::Error};

/// Histogram buckets for the distribution of checkpoint fetching latencies.
const INGESTION_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
];

/// Service to expose prometheus metrics from the indexer.
pub struct MetricsService {
    addr: SocketAddr,
    service: RegistryService,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub struct IndexerMetrics {
    // Statistics related to fetching data from the remote store.
    pub total_ingested_checkpoints: IntCounter,
    pub total_ingested_transactions: IntCounter,
    pub total_ingested_events: IntCounter,
    pub total_ingested_inputs: IntCounter,
    pub total_ingested_outputs: IntCounter,
    pub total_ingested_bytes: IntCounter,
    pub total_ingested_transient_retries: IntCounterVec,
    pub total_ingested_not_found_retries: IntCounter,

    // Distribution of times taken to fetch data from the remote store, including time taken on
    // retries.
    pub ingested_checkpoint_latency: Histogram,
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
    pub fn new(
        addr: SocketAddr,
        db: Db,
        cancel: CancellationToken,
    ) -> Result<(IndexerMetrics, MetricsService)> {
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None)?;

        let metrics = IndexerMetrics::new(&registry);
        mysten_metrics::init_metrics(&registry);
        registry.register(Box::new(DbConnectionStatsCollector::new(db)))?;

        let service = Self {
            addr,
            service: RegistryService::new(registry),
            cancel,
        };

        Ok((metrics, service))
    }

    /// Start the service. The service will run until the cancellation token is triggered.
    pub async fn run(self) -> Result<JoinHandle<()>> {
        let listener = TcpListener::bind(&self.addr).await?;
        let app = Router::new()
            .route("/metrics", get(mysten_metrics::metrics))
            .layer(Extension(self.service));

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

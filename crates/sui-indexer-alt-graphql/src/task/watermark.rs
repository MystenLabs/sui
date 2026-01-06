// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, anyhow, bail, ensure};
use chrono::{DateTime, Utc};
use diesel::{
    QueryableByName,
    sql_types::{BigInt, Text},
};
use futures::future::OptionFuture;
use sui_futures::service::Service;
use sui_indexer_alt_reader::{
    bigtable_reader::BigtableReader,
    consistent_reader::{self, ConsistentReader, proto::AvailableRangeResponse},
    ledger_grpc_reader::LedgerGrpcReader,
    pg_reader::PgReader,
};
use sui_sql_macro::query;
use tokio::{sync::RwLock, time};
use tracing::{debug, warn};

use crate::{
    api::types::available_range::pipeline_unavailable, config::WatermarkConfig, error::RpcError,
    metrics::RpcMetrics,
};

/// Background task responsible for tracking per-pipeline upper- and lower-bounds.
///
/// Each request takes a snapshot of these bounds when it starts and makes sure all queries to the
/// store are consistent with data from this snapshot.
pub(crate) struct WatermarkTask {
    /// Thread-safe watermark that avoids writer starvation. The outer `Arc` is used to share the
    /// watermarks between the schema and this task. The inner `Arc` is used to allow the task to
    /// efficiently swap in new watermark values.
    watermarks: WatermarksLock,

    /// Access to the Postgres DB
    pg_reader: PgReader,

    /// Access to Bigtable.
    bigtable_reader: Option<BigtableReader>,

    /// Access to the Ledger gRPC service.
    ledger_grpc_reader: Option<LedgerGrpcReader>,

    /// Access to the Consistent Store
    consistent_reader: ConsistentReader,

    /// How long to wait between updating the watermark.
    interval: Duration,

    /// Pipelines that we want to check the watermark for.
    pg_pipelines: Vec<String>,

    /// Access to metrics to report watermark updates.
    metrics: Arc<RpcMetrics>,
}

/// Snapshot of current watermarks. The upperbound is global across all pipelines, and the
/// lowerbounds are per-pipeline.
#[derive(Clone)]
pub(crate) struct Watermarks {
    /// The upperbound across all pipelines (the minimal high watermarks across all pipelines). The
    /// epoch and checkpoint bounds are inclusive and the transaction bound is exclusive.
    global_hi: Watermark,

    /// Timestamp for the inclusive global upperbound checkpoint.
    timestamp_ms_hi_inclusive: i64,

    /// Per-pipeline inclusive lowerbound watermarks
    pipeline_lo: BTreeMap<String, Watermark>,
}

#[derive(Clone, Default)]
pub(crate) struct Watermark {
    epoch: i64,
    checkpoint: i64,
    transaction: i64,
}

#[derive(QueryableByName, Clone)]
struct WatermarkRow {
    #[diesel(sql_type = Text)]
    pipeline: String,

    #[diesel(sql_type = BigInt)]
    epoch_hi_inclusive: i64,

    #[diesel(sql_type = BigInt)]
    checkpoint_hi_inclusive: i64,

    #[diesel(sql_type = BigInt)]
    tx_hi: i64,

    #[diesel(sql_type = BigInt)]
    timestamp_ms_hi_inclusive: i64,

    #[diesel(sql_type = BigInt)]
    epoch_lo: i64,

    #[diesel(sql_type = BigInt)]
    checkpoint_lo: i64,

    #[diesel(sql_type = BigInt)]
    tx_lo: i64,
}

pub(crate) type WatermarksLock = Arc<RwLock<Arc<Watermarks>>>;

impl WatermarkTask {
    pub(crate) fn new(
        config: WatermarkConfig,
        pg_pipelines: Vec<String>,
        pg_reader: PgReader,
        bigtable_reader: Option<BigtableReader>,
        ledger_grpc_reader: Option<LedgerGrpcReader>,
        consistent_reader: ConsistentReader,
        metrics: Arc<RpcMetrics>,
    ) -> Self {
        let WatermarkConfig {
            watermark_polling_interval,
        } = config;

        Self {
            watermarks: Default::default(),
            pg_reader,
            bigtable_reader,
            ledger_grpc_reader,
            consistent_reader,
            interval: watermark_polling_interval,
            pg_pipelines,
            metrics,
        }
    }

    /// The shared watermarks structure that this task writes to.
    pub(crate) fn watermarks(&self) -> WatermarksLock {
        self.watermarks.clone()
    }

    /// Start a new task that regularly polls the database for watermarks.
    pub(crate) fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let Self {
                watermarks,
                pg_reader,
                bigtable_reader,
                ledger_grpc_reader,
                consistent_reader,
                interval,
                pg_pipelines,
                metrics,
            } = self;

            let mut interval = time::interval(interval);

            loop {
                interval.tick().await;

                let rows = match WatermarkRow::read(&pg_reader, bigtable_reader.as_ref(), ledger_grpc_reader.as_ref(), &pg_pipelines).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        warn!("Failed to read watermarks: {e:#}");
                        continue;
                    }
                };

                let mut w = Watermarks::default();
                for row in rows {
                    row.record_metrics(&metrics);
                    w.merge(row);
                }

                match watermark_from_consistent(&consistent_reader, w.global_hi.checkpoint as u64).await {
                    Ok(None) => {}
                    Ok(Some(consistent_row)) => {
                        // Merge the consistent store watermark
                        consistent_row.record_metrics(&metrics);
                        w.merge(consistent_row);
                    }

                    Err(e) => {
                        warn!("Failed to get consistent store watermark: {e:#}");
                        continue;
                    }
                };

                debug!(
                    epoch = w.global_hi.epoch,
                    checkpoint = w.global_hi.checkpoint,
                    transaction = w.global_hi.transaction,
                    timestamp = ?DateTime::from_timestamp_millis(w.timestamp_ms_hi_inclusive).unwrap_or_default(),
                    "Watermark updated"
                );

                *watermarks.write().await = Arc::new(w);
            }
        })
    }
}

impl Watermarks {
    /// The high watermark across all pipelines. Returned as an inclusive checkpoint number,
    /// inclusive epoch number and an exclusive transaction sequence number.
    pub(crate) fn high_watermark(&self) -> &Watermark {
        &self.global_hi
    }

    /// The reader_lo watermark for a pipeline.
    pub(crate) fn pipeline_lo_watermark(&self, pipeline: &str) -> Result<&Watermark, RpcError> {
        self.pipeline_lo
            .get(pipeline)
            .ok_or_else(|| pipeline_unavailable(pipeline))
    }

    /// Timestamp corresponding to high watermark. Can be `None` if the timestamp is out of range
    /// (should not happen under normal operation).
    pub(crate) fn timestamp_hi(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive)
    }

    /// Timestamp corresponding to high watermark, as milliseconds since Unix epoch.
    pub(crate) fn timestamp_hi_ms(&self) -> u64 {
        self.timestamp_ms_hi_inclusive as u64
    }

    fn merge(&mut self, row: WatermarkRow) {
        self.global_hi.epoch = self.global_hi.epoch.min(row.epoch_hi_inclusive);
        self.global_hi.checkpoint = self.global_hi.checkpoint.min(row.checkpoint_hi_inclusive);
        self.global_hi.transaction = self.global_hi.transaction.min(row.tx_hi);
        self.timestamp_ms_hi_inclusive = self
            .timestamp_ms_hi_inclusive
            .min(row.timestamp_ms_hi_inclusive);

        self.pipeline_lo.insert(
            row.pipeline.clone(),
            Watermark {
                epoch: row.epoch_lo,
                checkpoint: row.checkpoint_lo,
                transaction: row.tx_lo,
            },
        );
    }
}

impl Watermark {
    pub(crate) fn checkpoint(&self) -> u64 {
        self.checkpoint as u64
    }

    pub(crate) fn transaction(&self) -> u64 {
        self.transaction as u64
    }
}

impl WatermarkRow {
    async fn read(
        pg_reader: &PgReader,
        bigtable_reader: Option<&BigtableReader>,
        ledger_grpc_reader: Option<&LedgerGrpcReader>,
        pg_pipelines: &[String],
    ) -> anyhow::Result<Vec<WatermarkRow>> {
        let rows = watermarks_from_pg(pg_reader, pg_pipelines);
        let bigtable: OptionFuture<_> = bigtable_reader.map(watermark_from_bigtable).into();
        let ledger_grpc: OptionFuture<_> =
            ledger_grpc_reader.map(watermark_from_ledger_grpc).into();

        let (rows, bigtable, ledger_grpc) = tokio::join!(rows, bigtable, ledger_grpc);
        let mut rows = rows.context("Failed to read watermarks from Postgres")?;

        let bigtable = bigtable
            .transpose()
            .context("Failed to read watermarks from Bigtable")?;
        rows.extend(bigtable);

        let ledger_grpc = ledger_grpc
            .transpose()
            .context("Failed to read watermarks from Ledger gRPC")?;
        rows.extend(ledger_grpc);

        Ok(rows)
    }

    fn record_metrics(&self, metrics: &Arc<RpcMetrics>) {
        metrics
            .watermark_epoch
            .with_label_values(&[&self.pipeline])
            .set(self.epoch_hi_inclusive);

        metrics
            .watermark_checkpoint
            .with_label_values(&[&self.pipeline])
            .set(self.checkpoint_hi_inclusive);

        metrics
            .watermark_transaction
            .with_label_values(&[&self.pipeline])
            .set(self.tx_hi);

        metrics
            .watermark_timestamp_ms
            .with_label_values(&[&self.pipeline])
            .set(self.timestamp_ms_hi_inclusive);

        metrics
            .watermark_reader_epoch_lo
            .with_label_values(&[&self.pipeline])
            .set(self.epoch_lo);

        metrics
            .watermark_reader_checkpoint_lo
            .with_label_values(&[&self.pipeline])
            .set(self.checkpoint_lo);

        metrics
            .watermark_reader_transaction_lo
            .with_label_values(&[&self.pipeline])
            .set(self.tx_lo);
    }
}

impl Default for Watermarks {
    fn default() -> Self {
        Self {
            global_hi: Watermark {
                epoch: i64::MAX,
                checkpoint: i64::MAX,
                transaction: i64::MAX,
            },
            timestamp_ms_hi_inclusive: i64::MAX,
            pipeline_lo: BTreeMap::new(),
        }
    }
}

async fn watermark_from_bigtable(bigtable_reader: &BigtableReader) -> anyhow::Result<WatermarkRow> {
    let summary = bigtable_reader
        .checkpoint_watermark()
        .await
        .context("Failed to get checkpoint watermark")?
        .context("Checkpoint watermark not found")?;

    Ok(WatermarkRow {
        pipeline: "bigtable".to_owned(),
        epoch_hi_inclusive: summary.epoch as i64,
        checkpoint_hi_inclusive: summary.sequence_number as i64,
        tx_hi: summary.network_total_transactions as i64,
        timestamp_ms_hi_inclusive: summary.timestamp_ms as i64,
        epoch_lo: 0,
        checkpoint_lo: 0,
        tx_lo: 0,
    })
}

async fn watermark_from_ledger_grpc(
    ledger_grpc_reader: &LedgerGrpcReader,
) -> anyhow::Result<WatermarkRow> {
    let summary = ledger_grpc_reader
        .checkpoint_watermark()
        .await
        .context("Failed to get checkpoint watermark")?;

    Ok(WatermarkRow {
        pipeline: "ledger_grpc".to_owned(),
        epoch_hi_inclusive: summary.epoch as i64,
        checkpoint_hi_inclusive: summary.sequence_number as i64,
        tx_hi: summary.network_total_transactions as i64,
        timestamp_ms_hi_inclusive: summary.timestamp_ms as i64,
        epoch_lo: 0,
        checkpoint_lo: 0,
        tx_lo: 0,
    })
}

async fn watermarks_from_pg(
    pg_reader: &PgReader,
    pg_pipelines: &[String],
) -> anyhow::Result<Vec<WatermarkRow>> {
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let rows: Vec<WatermarkRow> = conn
        .results(query!(
            r#"
            SELECT
                w.pipeline,
                w.epoch_hi_inclusive,
                w.checkpoint_hi_inclusive,
                w.tx_hi,
                w.timestamp_ms_hi_inclusive,
                c.epoch AS epoch_lo,
                w.reader_lo AS checkpoint_lo,
                c.tx_lo AS tx_lo
            FROM
                watermarks w
            INNER JOIN
                cp_sequence_numbers c
            ON (w.reader_lo = c.cp_sequence_number)
            WHERE
                pipeline = ANY({Array<Text>})
            "#,
            pg_pipelines,
        ))
        .await?;

    ensure!(
        !pg_pipelines.is_empty(),
        "Indexer not tracking any pipelines"
    );

    let mut remaining_pipelines = BTreeSet::from_iter(pg_pipelines.iter());
    for row in &rows {
        remaining_pipelines.remove(&row.pipeline);
    }

    ensure!(
        remaining_pipelines.is_empty(),
        "Missing watermarks for {remaining_pipelines:?}",
    );

    Ok(rows)
}

async fn watermark_from_consistent(
    consistent_reader: &ConsistentReader,
    checkpoint: u64,
) -> anyhow::Result<Option<WatermarkRow>> {
    match consistent_reader.available_range(checkpoint).await {
        Ok(AvailableRangeResponse {
            min_checkpoint: Some(min_checkpoint),
            max_checkpoint: Some(max_checkpoint),
            max_epoch: Some(max_epoch),
            total_transactions: Some(total_transactions),
            max_timestamp_ms: Some(max_timestamp_ms),
            stride: _,
        }) => Ok(Some(WatermarkRow {
            pipeline: "consistent".to_owned(),
            epoch_hi_inclusive: max_epoch as i64,
            checkpoint_hi_inclusive: max_checkpoint as i64,
            tx_hi: total_transactions as i64,
            timestamp_ms_hi_inclusive: max_timestamp_ms as i64,
            epoch_lo: 0,
            checkpoint_lo: min_checkpoint as i64,
            tx_lo: 0,
        })),

        Ok(_) => bail!("Missing data in consistent watermark"),
        Err(consistent_reader::Error::NotConfigured) => Ok(None),
        Err(e) => Err(anyhow!(e).context("Failed to get consistent store watermarks")),
    }
}

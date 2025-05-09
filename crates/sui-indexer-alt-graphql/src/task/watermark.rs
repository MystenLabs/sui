// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{ensure, Context as _};
use chrono::{DateTime, Utc};
use diesel::{
    sql_query,
    sql_types::{Array, BigInt, Text},
    QueryableByName,
};
use futures::future::OptionFuture;
use sui_indexer_alt_reader::{bigtable_reader::BigtableReader, pg_reader::PgReader};
use tokio::{join, sync::RwLock, task::JoinHandle, time};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::config::WatermarkConfig;

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

    /// How long to wait between updating the watermark.
    interval: Duration,

    /// Pipelines that we want to check the watermark for.
    pg_pipelines: Vec<String>,

    /// Signal to cancel the task.
    cancel: CancellationToken,
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
        pg_reader: PgReader,
        bigtable_reader: Option<BigtableReader>,
        cancel: CancellationToken,
    ) -> Self {
        let WatermarkConfig {
            watermark_polling_interval,
            pg_pipelines,
        } = config;

        Self {
            watermarks: Default::default(),
            pg_reader,
            bigtable_reader,
            interval: watermark_polling_interval,
            pg_pipelines,
            cancel,
        }
    }

    /// The shared watermarks structure that this task writes to.
    pub(crate) fn watermarks(&self) -> WatermarksLock {
        self.watermarks.clone()
    }

    /// Start a new task that regularly polls the database for watermarks.
    ///
    /// This operation consume the `self` and returns a handle to the spawned tokio task. The task
    /// will continue to run until its cancellation token is triggered.
    pub(crate) fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let Self {
                watermarks,
                pg_reader,
                bigtable_reader,
                interval,
                pg_pipelines,
                cancel,
            } = self;

            let mut interval = time::interval(interval);

            loop {
                tokio::select! {
                    biased;

                    _ = cancel.cancelled() => {
                        info!("Shutdown signal received, terminating watermark task");
                        break;
                    }

                    _ = interval.tick() => {
                        let w = match Watermarks::read(&pg_reader, bigtable_reader.as_ref(), &pg_pipelines).await {
                            Ok(w) => Arc::new(w),
                            Err(e) => {
                                warn!("Failed to read watermarks: {e:#}");
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

                        *watermarks.write().await = w;
                    }
                }
            }
        })
    }
}

impl Watermarks {
    async fn read(
        pg_reader: &PgReader,
        bigtable_reader: Option<&BigtableReader>,
        pg_pipelines: &[String],
    ) -> anyhow::Result<Self> {
        let rows = watermarks_from_pg(pg_reader, pg_pipelines);
        let last: OptionFuture<_> = bigtable_reader.map(watermark_from_bigtable).into();

        let (rows, last) = join!(rows, last);
        let rows = rows.context("Failed to read watermarks from Postgres")?;
        let last = last
            .transpose()
            .context("Failed to read watermarks from Bigtable")?;

        let mut watermarks = Watermarks::default();
        for row in rows.into_iter().chain(last.into_iter()) {
            watermarks.merge(row)
        }

        Ok(watermarks)
    }

    /// The high watermark across all pipelines. Returned as an inclusive checkpoint number,
    /// inclusive epoch number and an exclusive transaction sequence number.
    pub(crate) fn high_watermark(&self) -> &Watermark {
        &self.global_hi
    }

    /// Timestamp corresponding to high watermark. Can be `None` if the timestamp is out of range
    /// (should not happen under normal operation).
    pub(crate) fn timestamp_hi(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive)
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

async fn watermarks_from_pg(
    pg_reader: &PgReader,
    pg_pipelines: &[String],
) -> anyhow::Result<Vec<WatermarkRow>> {
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    let query = sql_query(
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
            pipeline = ANY($1)
        "#,
    )
    .bind::<Array<Text>, _>(pg_pipelines);

    let rows: Vec<WatermarkRow> = conn.results(query).await?;

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

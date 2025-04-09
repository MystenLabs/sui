// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use anyhow::ensure;
use chrono::{DateTime, Utc};
use diesel::{
    sql_query,
    sql_types::{Array, BigInt, Text},
    QueryableByName,
};
use sui_indexer_alt_reader::pg_reader::{Connection, PgReader};
use tokio::{sync::RwLock, task::JoinHandle, time};
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

    /// How long to wait between updating the watermark.
    interval: Duration,

    /// Pipelines that we want to check the watermark for.
    pg_pipelines: Vec<String>,

    /// Signal to cancel the task.
    cancel: CancellationToken,
}

/// Snapshot of current watermarks. The upperbound is global across all pipelines, and the
/// lowerbounds are per-pipeline.
#[derive(Clone, Default)]
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

pub(crate) type WatermarksLock = Arc<RwLock<Arc<Watermarks>>>;

impl WatermarkTask {
    pub(crate) fn new(
        config: WatermarkConfig,
        pg_reader: PgReader,
        cancel: CancellationToken,
    ) -> Self {
        let WatermarkConfig {
            watermark_polling_interval,
            pg_pipelines,
        } = config;

        Self {
            watermarks: Default::default(),
            pg_reader,
            interval: watermark_polling_interval,
            pg_pipelines,
            cancel,
        }
    }

    /// The shared watermarks structure that this task writes to.
    pub fn watermarks(&self) -> WatermarksLock {
        self.watermarks.clone()
    }

    /// Start a new task that regularly polls the database for watermarks.
    ///
    /// This operation consume the `self` and returns a handle to the spawned tokio task. The task
    /// will continue to run until its cancellation token is triggered.
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let Self {
                watermarks,
                pg_reader,
                interval,
                pg_pipelines,
                cancel,
            } = self;

            let mut interval = time::interval(interval);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Shutdown signal received, terminating watermark task");
                        break;
                    }

                    _ = interval.tick() => {
                        let mut conn = match pg_reader.connect().await {
                            Ok(conn) => conn,
                            Err(e) => {
                                warn!("Failed to connect to database: {e}");
                                continue;
                            }
                        };

                        let w = match Watermarks::read(&mut conn, &pg_pipelines).await {
                            Ok(w) => Arc::new(w),
                            Err(e) => {
                                warn!("Failed to read watermarks: {e}");
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
    async fn read(conn: &mut Connection<'_>, pg_pipelines: &[String]) -> anyhow::Result<Self> {
        #[derive(QueryableByName)]
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

        // SAFETY: rows.len() >= pg_pipelines.len() > 0 based on checks above.
        let mut rows = rows.into_iter();
        let first = rows.next().unwrap();

        let mut watermarks = Watermarks {
            global_hi: Watermark {
                epoch: first.epoch_hi_inclusive,
                checkpoint: first.checkpoint_hi_inclusive,
                transaction: first.tx_hi,
            },
            timestamp_ms_hi_inclusive: first.timestamp_ms_hi_inclusive,
            pipeline_lo: BTreeMap::from_iter([(
                first.pipeline.clone(),
                Watermark {
                    epoch: first.epoch_lo,
                    checkpoint: first.checkpoint_lo,
                    transaction: first.tx_lo,
                },
            )]),
        };

        for row in rows {
            watermarks.global_hi.epoch = watermarks.global_hi.epoch.min(row.epoch_hi_inclusive);
            watermarks.global_hi.checkpoint = watermarks
                .global_hi
                .checkpoint
                .min(row.checkpoint_hi_inclusive);
            watermarks.global_hi.transaction = watermarks.global_hi.transaction.min(row.tx_hi);
            watermarks.timestamp_ms_hi_inclusive = watermarks
                .timestamp_ms_hi_inclusive
                .min(row.timestamp_ms_hi_inclusive);
            watermarks.pipeline_lo.insert(
                row.pipeline.clone(),
                Watermark {
                    epoch: row.epoch_lo,
                    checkpoint: row.checkpoint_lo,
                    transaction: row.tx_lo,
                },
            );
        }

        Ok(watermarks)
    }

    /// Timestamp corresponding to high watermark. Can be `None` if the timestamp is out of range
    /// (should not happen under normal operation).
    pub(crate) fn timestamp_hi(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive)
    }
}

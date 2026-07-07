// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use chrono::DateTime;
use chrono::Utc;
use diesel::QueryableByName;
use diesel::sql_types::BigInt;
use diesel::sql_types::Text;
use futures::future::OptionFuture;
use sui_futures::service::Service;
use sui_indexer_alt_reader::bigtable_reader::BigtableReader;
use sui_indexer_alt_reader::consistent_reader;
use sui_indexer_alt_reader::consistent_reader::ConsistentReader;
use sui_indexer_alt_reader::consistent_reader::proto::AvailableRangeResponse;
use sui_indexer_alt_reader::consistent_reader::proto::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_reader::consistent_reader::proto::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_sql_macro::query;
use tokio::sync::RwLock;
use tokio::sync::watch;
use tokio::time;
use tonic::metadata::AsciiMetadataValue;
use tracing::debug;
use tracing::warn;

use crate::config::AvailabilityConfig;
use crate::config::PipelineAvailability;
use crate::config::PipelineConfig;
use crate::config::WatermarkConfig;
use crate::metrics::RpcMetrics;

/// Background task responsible for tracking per-pipeline upper- and lower-bounds.
///
/// Each request takes a snapshot of these bounds when it starts and makes sure all queries to the
/// store are consistent with data from this snapshot.
pub(crate) const KV_PACKAGES_PIPELINE: &str = "kv_packages";

pub(crate) struct WatermarkTask {
    /// Thread-safe watermark that avoids writer starvation. The outer `Arc` is used to share the
    /// watermarks between the schema and this task. The inner `Arc` is used to allow the task to
    /// efficiently swap in new watermark values.
    watermarks: WatermarksLock,

    /// Publishes the latest watermarks on each update. Consumers can subscribe and use
    /// `wait_for` with a predicate to await specific pipeline conditions without polling.
    watermarks_tx: watch::Sender<Arc<Watermarks>>,

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

    /// Configuration for which pipelines are enabled, used to determine which pipelines to check
    /// the watermark for.
    pipeline: PipelineConfig,

    /// Availability policy applied to each snapshot before it is published.
    availability: AvailabilityConfig,

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

    /// Per-pipeline watermarks keyed by pipeline name.
    per_pipeline: BTreeMap<String, Pipeline>,
}

#[derive(Clone, Default)]
pub(crate) struct Watermark {
    epoch: i64,
    checkpoint: i64,
    transaction: i64,
}

#[derive(Clone, Default)]
pub(crate) struct Pipeline {
    hi: Watermark,
    lo: Watermark,
    timestamp_ms_hi_inclusive: i64,

    /// Whether this pipeline is currently served, per the configured availability policy. Set by
    /// [`Watermarks::apply_availability`].
    available: bool,
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
        pipeline: PipelineConfig,
        availability: AvailabilityConfig,
        pg_reader: PgReader,
        bigtable_reader: Option<BigtableReader>,
        ledger_grpc_reader: Option<LedgerGrpcReader>,
        consistent_reader: ConsistentReader,
        metrics: Arc<RpcMetrics>,
    ) -> Self {
        let WatermarkConfig {
            watermark_polling_interval,
        } = config;

        let (watermarks_tx, _) = watch::channel(Arc::new(Watermarks::default()));
        Self {
            watermarks: Default::default(),
            watermarks_tx,
            pg_reader,
            bigtable_reader,
            ledger_grpc_reader,
            consistent_reader,
            interval: watermark_polling_interval,
            pipeline,
            availability,
            metrics,
        }
    }

    /// The shared watermarks structure that this task writes to.
    pub(crate) fn watermarks(&self) -> WatermarksLock {
        self.watermarks.clone()
    }

    /// Receiver for observing watermark updates. Use `wait_for` with a predicate to await
    /// specific pipeline conditions.
    pub(crate) fn watermarks_rx(&self) -> watch::Receiver<Arc<Watermarks>> {
        self.watermarks_tx.subscribe()
    }

    /// Start a new task that regularly polls the database for watermarks.
    pub(crate) fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let Self {
                watermarks,
                watermarks_tx,
                pg_reader,
                bigtable_reader,
                ledger_grpc_reader,
                consistent_reader,
                interval,
                pipeline,
                availability,
                metrics,
            } = self;

            let mut interval = time::interval(interval);

            loop {
                interval.tick().await;

                let rows = match WatermarkRow::read(&pg_reader, bigtable_reader.as_ref(), ledger_grpc_reader.as_ref()).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        warn!("Failed to read watermarks: {e:#}");
                        continue;
                    }
                };

                let mut w = Watermarks::default();
                for row in rows {
                    // A pipeline missing from `availability` (e.g. one that starts producing
                    // watermarks after startup) falls back to the configured default, same as a
                    // discovered one.
                    let enabled = pipeline
                        .availability
                        .get(&row.pipeline)
                        .copied()
                        .unwrap_or(pipeline.default_availability)
                        == PipelineAvailability::Enabled;

                    if !enabled {
                        continue;
                    }

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

                // Gate pipelines per the configured availability policy and recompute the global
                // upperbound over only the available ones, so a gated pipeline no longer pins the
                // consistency boundary.
                w.apply_availability(&availability);

                let previous = watermarks.read().await.clone();
                for (pipeline, next) in &w.per_pipeline {
                    if let Some(prev) = previous.per_pipeline().get(pipeline)
                        && next.hi.checkpoint < prev.hi.checkpoint
                    {
                        warn!(
                            pipeline,
                            prev = prev.hi.checkpoint,
                            next = next.hi.checkpoint,
                            "Watermark rollback"
                        );
                    }
                }

                debug!(
                    epoch = w.global_hi.epoch,
                    checkpoint = w.global_hi.checkpoint,
                    transaction = w.global_hi.transaction,
                    timestamp = ?DateTime::from_timestamp_millis(w.timestamp_ms_hi_inclusive).unwrap_or_default(),
                    "Watermark updated"
                );

                let w = Arc::new(w);
                // TODO: `WatermarksLock` is effectively a broadcast channel here — redundant
                // with `watermarks_tx`. Follow-up to unify on `watch::Receiver<Arc<Watermarks>>`
                // across request handlers and middleware.
                *watermarks.write().await = w.clone();
                let _ = watermarks_tx.send(w);
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

    /// Per-pipeline watermarks keyed by pipeline name.
    pub(crate) fn per_pipeline(&self) -> &BTreeMap<String, Pipeline> {
        &self.per_pipeline
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

    pub(crate) fn lag_ms(&self, now: DateTime<Utc>) -> u64 {
        now.signed_duration_since(self.timestamp_hi().unwrap_or(now))
            .to_std()
            .unwrap_or_default()
            .as_millis() as u64
    }

    pub(crate) fn initialized(&self) -> bool {
        self.global_hi.checkpoint != i64::MAX
    }

    fn merge(&mut self, row: WatermarkRow) {
        let pipeline = Pipeline {
            hi: Watermark {
                epoch: row.epoch_hi_inclusive,
                checkpoint: row.checkpoint_hi_inclusive,
                transaction: row.tx_hi,
            },
            lo: Watermark {
                epoch: row.epoch_lo,
                checkpoint: row.checkpoint_lo,
                transaction: row.tx_lo,
            },
            timestamp_ms_hi_inclusive: row.timestamp_ms_hi_inclusive,
            available: true,
        };

        self.global_hi.epoch = self.global_hi.epoch.min(pipeline.hi.epoch);
        self.global_hi.checkpoint = self.global_hi.checkpoint.min(pipeline.hi.checkpoint);
        self.global_hi.transaction = self.global_hi.transaction.min(pipeline.hi.transaction);
        self.timestamp_ms_hi_inclusive = self
            .timestamp_ms_hi_inclusive
            .min(pipeline.timestamp_ms_hi_inclusive);

        self.per_pipeline.insert(row.pipeline, pipeline);
    }

    /// Apply the configured availability policy to this snapshot: flag each pipeline as available
    /// or not, then recompute the global upperbound (and its timestamp) over only the available
    /// pipelines. A gated pipeline therefore stops pinning `checkpoint_viewed_at`, while remaining
    /// present in `per_pipeline` for health and metrics reporting.
    fn apply_availability(&mut self, config: &AvailabilityConfig) {
        // Approximate the network tip as the furthest-ahead pipeline. When a Bigtable or Ledger
        // gRPC source is configured its virtual pipeline tracks the true network tip; computing the
        // max over every pipeline (including any that end up gated) keeps the reference stable.
        let network_tip = self
            .per_pipeline
            .values()
            .map(|p| p.hi.checkpoint)
            .max()
            .unwrap_or(0) as u64;

        for (name, pipeline) in self.per_pipeline.iter_mut() {
            // A pipeline's own override wins, then the configured default; a pipeline with
            // neither is always available.
            pipeline.available = match config.policy_for(name) {
                Some(policy) => policy.is_available(pipeline.hi.checkpoint as u64, network_tip),
                None => true,
            };
        }

        let mut global_hi = Watermark {
            epoch: i64::MAX,
            checkpoint: i64::MAX,
            transaction: i64::MAX,
        };
        let mut timestamp_ms_hi_inclusive = i64::MAX;
        let mut any_available = false;
        for pipeline in self.per_pipeline.values().filter(|p| p.available) {
            any_available = true;
            global_hi.epoch = global_hi.epoch.min(pipeline.hi.epoch);
            global_hi.checkpoint = global_hi.checkpoint.min(pipeline.hi.checkpoint);
            global_hi.transaction = global_hi.transaction.min(pipeline.hi.transaction);
            timestamp_ms_hi_inclusive =
                timestamp_ms_hi_inclusive.min(pipeline.timestamp_ms_hi_inclusive);
        }

        // A lag policy can never gate the furthest-ahead pipeline (it is at zero distance from
        // the tip), but `enabled = false` can gate every pipeline. In that case (and for an empty
        // snapshot) keep the merge-time bounds so the snapshot stays `initialized` with a sane
        // `checkpoint_viewed_at` rather than `i64::MAX`.
        if any_available {
            self.global_hi = global_hi;
            self.timestamp_ms_hi_inclusive = timestamp_ms_hi_inclusive;
        }
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

impl Pipeline {
    pub(crate) fn timestamp_hi(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp_ms_hi_inclusive)
    }

    pub(crate) fn lag_ms(&self, now: DateTime<Utc>) -> u64 {
        now.signed_duration_since(self.timestamp_hi().unwrap_or(now))
            .to_std()
            .unwrap_or_default()
            .as_millis() as u64
    }

    pub(crate) fn hi(&self) -> &Watermark {
        &self.hi
    }

    pub(crate) fn lo(&self) -> &Watermark {
        &self.lo
    }

    pub(crate) fn available(&self) -> bool {
        self.available
    }
}

impl WatermarkRow {
    async fn read(
        pg_reader: &PgReader,
        bigtable_reader: Option<&BigtableReader>,
        ledger_grpc_reader: Option<&LedgerGrpcReader>,
    ) -> anyhow::Result<Vec<WatermarkRow>> {
        let rows = watermarks_from_pg(pg_reader);
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
            per_pipeline: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
impl Watermarks {
    /// Build a `Watermarks` snapshot with the given pipeline high-checkpoints, merged the same
    /// way production rows are.
    pub(crate) fn for_test(pipelines: &[(&str, u64)]) -> Self {
        let mut w = Self::default();
        for (name, hi_cp) in pipelines {
            w.merge(WatermarkRow {
                pipeline: name.to_string(),
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: *hi_cp as i64,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                epoch_lo: 0,
                checkpoint_lo: 0,
                tx_lo: 0,
            });
        }
        w
    }

    /// Build a snapshot from `pipelines`, then apply `config`'s availability policy to it.
    pub(crate) fn for_test_with_availability(
        pipelines: &[(&str, u64)],
        config: &AvailabilityConfig,
    ) -> Self {
        let mut w = Self::for_test(pipelines);
        w.apply_availability(config);
        w
    }
}

async fn watermark_from_bigtable(bigtable_reader: &BigtableReader) -> anyhow::Result<WatermarkRow> {
    let wm = bigtable_reader
        .watermark()
        .await
        .context("Failed to get checkpoint watermark")?
        .context("Checkpoint watermark not found")?;
    let checkpoint_hi_inclusive = wm
        .checkpoint_hi_inclusive
        .context("Checkpoint watermark not found")?;

    Ok(WatermarkRow {
        pipeline: "bigtable".to_owned(),
        epoch_hi_inclusive: wm.epoch_hi_inclusive as i64,
        checkpoint_hi_inclusive: checkpoint_hi_inclusive as i64,
        tx_hi: wm.tx_hi as i64,
        timestamp_ms_hi_inclusive: wm.timestamp_ms_hi_inclusive as i64,
        epoch_lo: 0,
        checkpoint_lo: wm.reader_lo as i64,
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

async fn watermarks_from_pg(pg_reader: &PgReader) -> anyhow::Result<Vec<WatermarkRow>> {
    let mut conn = pg_reader
        .connect()
        .await
        .context("Failed to connect to database")?;

    // Filter out pipelines that have been initialized, but do not yet have indexed checkpoints with
    // `reader_lo <= checkpoint_hi_inclusive`.
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
                w.reader_lo <= w.checkpoint_hi_inclusive
            "#,
        ))
        .await?;

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

        Ok(available_range) => {
            bail!("Consistent watermark missing data: {available_range:?}");
        }

        Err(consistent_reader::Error::OutOfRange(status)) => {
            let unknown = AsciiMetadataValue::from_static("<unknown>");

            let min = status
                .metadata()
                .get(LOWEST_AVAILABLE_CHECKPOINT_METADATA)
                .unwrap_or(&unknown);

            let max = status
                .metadata()
                .get(CHECKPOINT_HEIGHT_METADATA)
                .unwrap_or(&unknown);

            bail!("{}: ({min:?}, {max:?})", status.message());
        }

        Err(consistent_reader::Error::NotConfigured) => Ok(None),

        Err(e) => Err(anyhow!(e).context(format!(
            "Failed to get consistent store watermarks at checkpoint {checkpoint}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;
    use diesel::Insertable;
    use diesel::insert_into;
    use diesel_async::RunQueryDsl as _;
    use prometheus::Registry;
    use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_indexer_alt_schema::cp_sequence_numbers::StoredCpSequenceNumbers;
    use sui_indexer_alt_schema::schema::cp_sequence_numbers;
    use sui_pg_db::Db;
    use sui_pg_db::DbArgs;
    use sui_pg_db::schema::watermarks;
    use sui_pg_db::temp::TempDb;

    use super::*;

    /// Mirrors `sui_pg_db::model::StoredWatermark`, which isn't public outside that crate.
    #[derive(Insertable)]
    #[diesel(table_name = watermarks)]
    struct NewWatermark {
        pipeline: String,
        epoch_hi_inclusive: i64,
        checkpoint_hi_inclusive: i64,
        tx_hi: i64,
        timestamp_ms_hi_inclusive: i64,
        reader_lo: i64,
        pruner_timestamp: NaiveDateTime,
        pruner_hi: i64,
    }

    impl NewWatermark {
        fn new(pipeline: &str) -> Self {
            Self {
                pipeline: pipeline.to_owned(),
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: 10,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo: 0,
                pruner_timestamp: Utc::now().naive_utc(),
                pruner_hi: 0,
            }
        }
    }

    /// Set up a temporary database with a watermark row for each of `pipelines` (all sharing one
    /// `cp_sequence_numbers` row, to satisfy `watermarks_from_pg`'s join), and the readers needed
    /// to construct a `WatermarkTask` against it.
    async fn setup(pipelines: &[&str]) -> (TempDb, PgReader, ConsistentReader, Arc<RpcMetrics>) {
        let registry = Registry::new();
        let temp_db = TempDb::new().unwrap();
        let url = temp_db.database().url();

        let writer = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();
        let reader = PgReader::new(None, Some(url.clone()), DbArgs::default(), &registry)
            .await
            .unwrap();

        writer.run_migrations(Some(&MIGRATIONS)).await.unwrap();

        let mut conn = writer.connect().await.unwrap();

        insert_into(cp_sequence_numbers::table)
            .values(StoredCpSequenceNumbers {
                cp_sequence_number: 0,
                tx_lo: 0,
                epoch: 0,
            })
            .execute(&mut conn)
            .await
            .unwrap();

        if !pipelines.is_empty() {
            insert_into(watermarks::table)
                .values(
                    pipelines
                        .iter()
                        .map(|p| NewWatermark::new(p))
                        .collect::<Vec<_>>(),
                )
                .execute(&mut conn)
                .await
                .unwrap();
        }

        let consistent_reader =
            ConsistentReader::new(None, ConsistentReaderArgs::default(), &registry)
                .await
                .unwrap();

        (
            temp_db,
            reader,
            consistent_reader,
            RpcMetrics::new(&registry),
        )
    }

    /// Construct a `WatermarkTask` polling every 20ms, run it, and return the first snapshot it
    /// publishes.
    async fn snapshot(
        pipeline: PipelineConfig,
        pg_reader: PgReader,
        consistent_reader: ConsistentReader,
        metrics: Arc<RpcMetrics>,
    ) -> Arc<Watermarks> {
        let task = WatermarkTask::new(
            WatermarkConfig {
                watermark_polling_interval: Duration::from_millis(20),
            },
            pipeline,
            pg_reader,
            None,
            None,
            consistent_reader,
            metrics,
        );

        let mut rx = task.watermarks_rx();
        let _service = task.run();

        tokio::time::timeout(Duration::from_secs(5), rx.changed())
            .await
            .expect("Timed out waiting for the first watermark snapshot")
            .unwrap();

        rx.borrow_and_update().clone()
    }

    fn within_tip(pipeline: &str, max_checkpoint_lag: u64) -> AvailabilityConfig {
        AvailabilityConfig {
            default: None,
            pipelines: BTreeMap::from([(
                pipeline.to_string(),
                PipelineAvailability::MaxCheckpointLag(max_checkpoint_lag),
            )]),
        }
    }

    fn default_within_tip(max_checkpoint_lag: u64) -> AvailabilityConfig {
        AvailabilityConfig {
            default: Some(PipelineAvailability::MaxCheckpointLag(max_checkpoint_lag)),
            pipelines: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn explicit_enabled_and_disabled_are_respected() {
        let (_db, pg_reader, consistent_reader, metrics) = setup(&["tx_calls", "kv_objects"]).await;

        let pipeline = PipelineConfig {
            availability: BTreeMap::from([
                ("tx_calls".to_string(), PipelineAvailability::Enabled),
                ("kv_objects".to_string(), PipelineAvailability::Disabled),
            ]),
            default_availability: PipelineAvailability::Disabled,
        };

        let w = snapshot(pipeline, pg_reader, consistent_reader, metrics).await;

        assert!(w.per_pipeline().contains_key("tx_calls"));
        assert!(!w.per_pipeline().contains_key("kv_objects"));
    }

    #[tokio::test]
    async fn unlisted_pipeline_falls_back_to_an_enabled_default() {
        let (_db, pg_reader, consistent_reader, metrics) = setup(&["tx_calls", "kv_objects"]).await;

        // Neither pipeline is explicitly listed in `availability`, so both fall back to the
        // default -- this is what lets a pipeline that starts running after boot get tracked
        // without ever being configured or discovered ahead of time.
        let pipeline = PipelineConfig {
            availability: BTreeMap::new(),
            default_availability: PipelineAvailability::Enabled,
        };

        let w = snapshot(pipeline, pg_reader, consistent_reader, metrics).await;

        assert!(w.per_pipeline().contains_key("tx_calls"));
        assert!(w.per_pipeline().contains_key("kv_objects"));
    }

    #[tokio::test]
    async fn unlisted_pipeline_respects_a_disabled_default() {
        let (_db, pg_reader, consistent_reader, metrics) = setup(&["tx_calls"]).await;

        let pipeline = PipelineConfig {
            availability: BTreeMap::new(),
            default_availability: PipelineAvailability::Disabled,
        };

        let w = snapshot(pipeline, pg_reader, consistent_reader, metrics).await;

        assert!(!w.per_pipeline().contains_key("tx_calls"));
    }

    #[test]
    fn empty_config_keeps_all_available_and_global_hi_is_min() {
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        w.apply_availability(&AvailabilityConfig::default());
        assert!(w.per_pipeline["a"].available);
        assert!(w.per_pipeline["b"].available);
        assert_eq!(w.high_watermark().checkpoint(), 100);
    }

    #[test]
    fn lagging_pipeline_is_dropped_from_global_hi() {
        // "a" is the min and lags "b" (the tip) by 100 checkpoints.
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        w.apply_availability(&within_tip("a", 50));
        assert!(!w.per_pipeline["a"].available);
        assert!(w.per_pipeline["b"].available);
        // Gating "a" advances the boundary to "b".
        assert_eq!(w.high_watermark().checkpoint(), 200);
    }

    #[test]
    fn within_tip_gates_on_distance_from_the_tip() {
        // "tip" is the furthest-ahead pipeline; "lagging" is 100 checkpoints behind it.
        let base = Watermarks::for_test(&[("tip", 1_000_000), ("lagging", 999_900)]);

        // A budget of 100 keeps the lagging pipeline available (boundary inclusive).
        let mut within = base.clone();
        within.apply_availability(&within_tip("lagging", 100));
        assert!(within.per_pipeline["lagging"].available);
        assert_eq!(within.high_watermark().checkpoint(), 999_900);

        // A budget of 99 gates it out, advancing the boundary to the tip.
        let mut beyond = base;
        beyond.apply_availability(&within_tip("lagging", 99));
        assert!(!beyond.per_pipeline["lagging"].available);
        assert_eq!(beyond.high_watermark().checkpoint(), 1_000_000);
    }

    #[test]
    fn furthest_ahead_pipeline_is_never_gated() {
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        // Gate every pipeline to zero lag: "a" is 100 behind the tip and drops, but "b" is the tip
        // itself (zero distance) and stays available, so the boundary never resets to `i64::MAX`.
        w.apply_availability(&default_within_tip(0));
        assert!(!w.per_pipeline["a"].available);
        assert!(w.per_pipeline["b"].available);
        assert!(w.initialized());
        assert_eq!(w.high_watermark().checkpoint(), 200);
    }

    #[test]
    fn default_gates_pipelines_without_an_override() {
        // "a" lags "b" (the tip) by 100 checkpoints, beyond the default budget of 50.
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        w.apply_availability(&default_within_tip(50));
        assert!(!w.per_pipeline["a"].available);
        assert!(w.per_pipeline["b"].available);
        assert_eq!(w.high_watermark().checkpoint(), 200);
    }

    #[test]
    fn override_beats_the_default() {
        let base = Watermarks::for_test(&[("a", 100), ("b", 200)]);

        // A looser override keeps "a" available where the default would gate it.
        let mut loosened = base.clone();
        let mut config = default_within_tip(50);
        config.pipelines.insert(
            "a".to_string(),
            PipelineAvailability::MaxCheckpointLag(1000),
        );
        loosened.apply_availability(&config);
        assert!(loosened.per_pipeline["a"].available);
        assert_eq!(loosened.high_watermark().checkpoint(), 100);

        // A tighter override gates "a" where the default would allow it.
        let mut tightened = base;
        let mut config = default_within_tip(1000);
        config
            .pipelines
            .insert("a".to_string(), PipelineAvailability::MaxCheckpointLag(50));
        tightened.apply_availability(&config);
        assert!(!tightened.per_pipeline["a"].available);
        assert_eq!(tightened.high_watermark().checkpoint(), 200);
    }

    #[test]
    fn disabled_pipeline_is_dropped_even_at_the_tip() {
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        let mut config = AvailabilityConfig::default();
        config
            .pipelines
            .insert("b".to_string(), PipelineAvailability::Disabled);
        w.apply_availability(&config);
        assert!(w.per_pipeline["a"].available);
        assert!(!w.per_pipeline["b"].available);
        // With the tip gated, the boundary is the min over the remaining pipelines.
        assert_eq!(w.high_watermark().checkpoint(), 100);
    }

    #[test]
    fn enabled_override_exempts_from_the_default() {
        // "a" lags beyond the default budget but is force-enabled, so it stays available and
        // keeps pinning the boundary.
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        let mut config = default_within_tip(50);
        config
            .pipelines
            .insert("a".to_string(), PipelineAvailability::Enabled);
        w.apply_availability(&config);
        assert!(w.per_pipeline["a"].available);
        assert_eq!(w.high_watermark().checkpoint(), 100);
    }

    #[test]
    fn all_disabled_snapshot_stays_initialized() {
        let mut w = Watermarks::for_test(&[("a", 100), ("b", 200)]);
        w.apply_availability(&AvailabilityConfig {
            default: Some(PipelineAvailability::Disabled),
            pipelines: BTreeMap::new(),
        });
        assert!(!w.per_pipeline["a"].available);
        assert!(!w.per_pipeline["b"].available);
        // No pipeline is available; the merge-time bounds are kept so the snapshot stays usable.
        assert!(w.initialized());
        assert_eq!(w.high_watermark().checkpoint(), 100);
    }
}

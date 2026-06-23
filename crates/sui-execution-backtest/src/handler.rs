// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The backtest as a single `sui-indexer-alt-framework` concurrent pipeline. The framework's
//! `Indexer` drives ingestion (bounded range, adaptive concurrency) and runs [`Backtest::process`]
//! per checkpoint with a configurable fanout; the processor re-executes the checkpoint's
//! transactions serially on a blocking worker and emits typed rows. The sink is abstracted behind
//! [`CommitRows`] so the same pipeline writes either to postgres (durable, queryable) or to an
//! ndjson file (zero-setup) — selected by `--store`.

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl as _;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::postgres::{Connection, Db};
use sui_indexer_alt_framework::store::{ConcurrentStore, Store};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::ChainIdentifier;
use sui_types::full_checkpoint_content::Checkpoint;
use tracing::error;

use crate::StatusFilter;
use crate::context::{EpochCtx, PreparedCheckpoint};
use crate::execute::{CheckpointStats, execute_one_transaction};
use crate::ndjson_store::{NdjsonConnection, NdjsonStore};
use crate::rows::{DivergenceRow, RunStatsRow};
use crate::schema::{divergence, run_stats};
use crate::store::{PackageCache, ScanStore};

/// Cap on rows per committed batch. Bounds postgres bind parameters (≤ ~13 columns × this < the
/// 32,767 limit) and keeps batches small; the framework also commits on its own size/time
/// thresholds.
const MAX_BATCH_ROWS: usize = 2000;

/// A row the pipeline produces. Both row types ride the same `process` output (the checkpoint is
/// executed once) and are split apart at commit time.
pub(crate) enum Row {
    Divergence(DivergenceRow),
    Stats(RunStatsRow),
}

/// Accumulated rows for one committed batch, split by destination table.
#[derive(Default)]
pub(crate) struct BacktestBatch {
    divergences: Vec<DivergenceRow>,
    stats: Vec<RunStatsRow>,
}

impl BacktestBatch {
    fn len(&self) -> usize {
        self.divergences.len() + self.stats.len()
    }
}

/// Store-specific write of a batch. Defined here (locally) so it can be implemented for both the
/// foreign postgres `Db` and our `NdjsonStore` without an orphan-rule conflict.
#[async_trait]
pub(crate) trait CommitRows: ConcurrentStore {
    async fn commit_rows(conn: &mut Self::Connection<'_>, batch: &BacktestBatch) -> Result<usize>;
}

#[async_trait]
impl CommitRows for Db {
    async fn commit_rows(conn: &mut Connection<'_>, batch: &BacktestBatch) -> Result<usize> {
        let mut affected = 0;
        if !batch.divergences.is_empty() {
            affected += diesel::insert_into(divergence::table)
                .values(&batch.divergences)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?;
        }
        if !batch.stats.is_empty() {
            affected += diesel::insert_into(run_stats::table)
                .values(&batch.stats)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?;
        }
        Ok(affected)
    }
}

#[async_trait]
impl CommitRows for NdjsonStore {
    async fn commit_rows(conn: &mut NdjsonConnection<'_>, batch: &BacktestBatch) -> Result<usize> {
        let store = conn.store();
        let mut written = store.write_rows(&batch.divergences)?;
        written += store.write_rows(&batch.stats)?;
        // Flush per commit so partial results survive an interrupted run.
        store.flush()?;
        Ok(written)
    }
}

/// The backtest pipeline, generic over its sink `S`. Holds the per-epoch execution contexts and the
/// shared package cache; `process` looks up the checkpoint's epoch context and re-executes it.
pub(crate) struct Backtest<S> {
    epochs: Arc<BTreeMap<u64, Arc<EpochCtx>>>,
    packages: Arc<PackageCache>,
    chain_id: ChainIdentifier,
    status: StatusFilter,
    task: String,
    stats_enabled: bool,
    _store: PhantomData<fn() -> S>,
}

impl<S> Backtest<S> {
    pub(crate) fn new(
        epochs: Arc<BTreeMap<u64, Arc<EpochCtx>>>,
        packages: Arc<PackageCache>,
        chain_id: ChainIdentifier,
        status: StatusFilter,
        task: String,
        stats_enabled: bool,
    ) -> Self {
        Self {
            epochs,
            packages,
            chain_id,
            status,
            task,
            stats_enabled,
            _store: PhantomData,
        }
    }

    /// Prefetch the checkpoint's package closure, then re-execute its transactions serially on a
    /// blocking worker, returning the divergence rows plus (unless disabled) a per-checkpoint stats
    /// row. Never errors: per-transaction reconstruction/replay failures are counted, and a panic
    /// in the blocking worker is logged and treated as an empty result, so one bad checkpoint can't
    /// wedge the (retry-forever) pipeline.
    async fn run(&self, checkpoint: &Arc<Checkpoint>) -> Vec<Row> {
        let epoch = checkpoint.summary.epoch;
        let cp = checkpoint.summary.sequence_number;
        let Some(ctx) = self.epochs.get(&epoch).cloned() else {
            error!(
                epoch,
                cp, "no epoch context resolved for checkpoint; skipping"
            );
            return Vec::new();
        };

        // Warm the shared package cache with this checkpoint's closure before the synchronous
        // execute stage (a miss falls through to the lazy fetch).
        crate::store::prefetch_package_closure(checkpoint, &self.packages).await;

        let (objects, latest) = ScanStore::index_object_set(&checkpoint.object_set);
        let tombstones = gather_tombstones(checkpoint);
        let objects = Arc::new(objects);
        let store = Arc::new(ScanStore::new(
            objects.clone(),
            Arc::new(latest),
            Arc::new(tombstones),
            self.packages.clone(),
        ));
        let prepared = Arc::new(PreparedCheckpoint {
            cp,
            ctx,
            chain_id: self.chain_id,
            checkpoint: checkpoint.clone(),
            objects,
        });

        let status = self.status;
        let task = self.task.clone();
        let count = prepared.checkpoint.transactions.len();
        let agg = tokio::task::spawn_blocking(move || {
            let mut agg = CheckpointStats::default();
            for i in 0..count {
                agg.merge(execute_one_transaction(&prepared, &store, i, status, &task));
            }
            agg
        })
        .await
        .unwrap_or_else(|e| {
            error!(epoch, cp, "execution task panicked: {e}");
            CheckpointStats::default()
        });

        let mut rows: Vec<Row> = agg.records.into_iter().map(Row::Divergence).collect();
        if self.stats_enabled {
            rows.push(Row::Stats(RunStatsRow {
                task: self.task.clone(),
                epoch: epoch as i64,
                checkpoint: cp as i64,
                checked: agg.checked as i64,
                executed: agg.executed as i64,
                divergences: agg.divergences as i64,
                reconstruction_errors: agg.reconstruction_errors as i64,
                coin_reservation_skipped: agg.coin_reservation_skipped as i64,
                execute_skipped: agg.execute_skipped as i64,
                gas_from_balance: agg.gas_from_balance as i64,
                cancellation_excluded: agg.cancellation_excluded as i64,
            }));
        }
        rows
    }
}

#[async_trait]
impl<S: CommitRows> Processor for Backtest<S> {
    const NAME: &'static str = "backtest";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Row>> {
        Ok(self.run(checkpoint).await)
    }
}

#[async_trait]
impl<S: CommitRows> Handler for Backtest<S> {
    type Store = S;
    type Batch = BacktestBatch;

    fn batch(
        &self,
        batch: &mut BacktestBatch,
        values: &mut std::vec::IntoIter<Row>,
    ) -> BatchStatus {
        for row in values.by_ref() {
            match row {
                Row::Divergence(d) => batch.divergences.push(d),
                Row::Stats(s) => batch.stats.push(s),
            }
            if batch.len() >= MAX_BATCH_ROWS {
                return BatchStatus::Ready;
            }
        }
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &BacktestBatch,
        conn: &mut <S as Store>::Connection<'a>,
    ) -> Result<usize> {
        S::commit_rows(conn, batch).await
    }
}

/// Tombstones (deleted/wrapped/unwrapped-then-deleted) across all of the checkpoint's transactions,
/// so child reads can tell a dynamic field was removed even though the bundled object set still
/// carries a stale pre-deletion version of it.
fn gather_tombstones(checkpoint: &Checkpoint) -> BTreeMap<ObjectID, BTreeSet<SequenceNumber>> {
    let mut tombstones: BTreeMap<ObjectID, BTreeSet<SequenceNumber>> = BTreeMap::new();
    for executed in &checkpoint.transactions {
        for (id, ver) in executed.effects.all_tombstones() {
            tombstones.entry(id).or_default().insert(ver);
        }
    }
    tombstones
}

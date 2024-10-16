// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures::TryStreamExt;
use mysten_metrics::spawn_monitored_task;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    db::{self, Db},
    metrics::IndexerMetrics,
};

pub mod kv_checkpoints;
pub mod kv_objects;
pub mod kv_transactions;
pub mod tx_affected_objects;
pub mod tx_balance_changes;

/// Extra buffer added to the channel between the handler and the committer. There does not need to
/// be a huge capacity here because the committer is already buffering rows to insert internally.
const COMMITTER_BUFFER: usize = 5;

/// The committer will wait at least this long between commits for any given pipeline.
const COOLDOWN_INTERVAL: Duration = Duration::from_millis(20);

/// The committer will wait at least this long between attempts to commit a failed batch.
const RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data into
/// rows for their table, and how to write those rows to the database.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values). Reasonable defaults have been chosen to balance concurrency with memory
/// usage, but each handle may choose to override these defaults, e.g.
///
/// - Handlers that produce many small rows may wish to increase their batch/chunk/max-pending
///   sizes).
/// - Handlers that do more work during processing may wish to increase their fanout so more of it
///   can be done concurrently, to preserve throughput.
#[async_trait::async_trait]
pub trait Handler {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: &'static str;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// If at least this many rows are pending, the committer will commit them eagerly.
    const BATCH_SIZE: usize = 50;

    /// If there are more than this many rows pending, the committer will only commit this many in
    /// one operation.
    const CHUNK_SIZE: usize = 200;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_SIZE: usize = 1000;

    /// The type of value being inserted by the handler.
    type Value: Send + Sync + 'static;

    /// The processing logic for turning a checkpoint into rows of the table.
    fn handle(checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>>;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>)
        -> anyhow::Result<usize>;
}

#[derive(clap::Args, Debug, Clone)]
pub struct CommitterConfig {
    /// Committer will check for pending data at least this often
    #[arg(
        long,
        default_value = "500",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis),
    )]
    commit_interval: Duration,
}

/// A batch of processed values associated with a single checkpoint. This is an internal type used
/// to communicate between the handler and the committer parts of the pipeline.
struct Indexed<H: Handler> {
    sequence_number: u64,
    values: Vec<H::Value>,
}

/// Start a new indexing pipeline served by the handler, `H`. Each pipeline consists of a handler
/// task which takes checkpoint data and breaks it down into rows, ready for insertion, and a
/// committer which writes those rows out to the database.
///
/// Checkpoint data is fed into the pipeline through the `handler_rx` channel, and an internal
/// channel is created to communicate checkpoint-wise data to the committer. The pipeline can be
/// shutdown using its `cancel` token.
pub fn pipeline<H: Handler + 'static>(
    db: Db,
    handler_rx: mpsc::Receiver<Arc<CheckpointData>>,
    config: CommitterConfig,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> (JoinHandle<()>, JoinHandle<()>) {
    let (handler_tx, committer_rx) = mpsc::channel(H::FANOUT + COMMITTER_BUFFER);

    let handler = handler::<H>(handler_rx, handler_tx, metrics.clone(), cancel.clone());
    let committer = committer::<H>(db, committer_rx, config, metrics, cancel);

    (handler, committer)
}

/// The handler task is responsible for taking checkpoint data and breaking it down into rows ready
/// to commit. It spins up a supervisor that waits on the `rx` channel for checkpoints, and
/// distributes them among `H::FANOUT` workers.
///
/// Each worker processes a checkpoint into rows and sends them on to the committer using the `tx`
/// channel.
///
/// The task will shutdown if the `cancel` token is cancelled, or if any of the workers encounters
/// an error -- there is no retry logic at this level.
fn handler<H: Handler + 'static>(
    rx: mpsc::Receiver<Arc<CheckpointData>>,
    tx: mpsc::Sender<Indexed<H>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    /// Internal type used by workers to propagate errors or shutdown signals up to their
    /// supervisor.
    #[derive(thiserror::Error, Debug)]
    enum Break {
        #[error("Shutdown received")]
        Cancel,

        #[error(transparent)]
        Err(#[from] anyhow::Error),
    }

    spawn_monitored_task!(async move {
        match ReceiverStream::new(rx)
            .map(Ok)
            .try_for_each_concurrent(H::FANOUT, |checkpoint| {
                let tx = tx.clone();
                let metrics = metrics.clone();
                let cancel = cancel.clone();
                async move {
                    if cancel.is_cancelled() {
                        return Err(Break::Cancel);
                    }

                    metrics
                        .total_handler_checkpoints_received
                        .with_label_values(&[H::NAME])
                        .inc();

                    let guard = metrics
                        .handler_checkpoint_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let values = H::handle(&checkpoint)?;
                    let elapsed = guard.stop_and_record();

                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        "Processed checkpoint",
                    );

                    metrics
                        .total_handler_checkpoints_processed
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .total_handler_rows_created
                        .with_label_values(&[H::NAME])
                        .inc_by(values.len() as u64);

                    tx.send(Indexed {
                        sequence_number: checkpoint.checkpoint_summary.sequence_number,
                        values,
                    })
                    .await
                    .map_err(|_| Break::Cancel)?;

                    Ok(())
                }
            })
            .await
        {
            Ok(()) | Err(Break::Cancel) => {
                info!(pipeline = H::NAME, "Shutdown received, stopping handler");
            }

            Err(Break::Err(e)) => {
                error!(pipeline = H::NAME, "Error from handler: {e}");
                cancel.cancel();
            }
        };
    })
}

/// The commiter task is responsible for gathering rows to write to the database. It is a single
/// work loop that gathers checkpoint-wise row information, and periodically writes them out to the
/// database.
///
/// The period between writes is controlled by the following factors:
///
/// - Time since the last write (controlled by `config.commit_interval`). If there are rows pending
///   and this interval has elapsed since the last attempted commit, the committer will attempt
///   another write.
///
/// - Time since last attempted write (controlled by `COOLDOWN_INTERVAL` and `RETRY_INTERVAL`). If
///   there was a recent successful write, the next write will wait at least `COOLDOWN_INTERVAL`,
///   and if there was a recent unsuccessful write, the next write will wait at least
///   `RETRY_INTERVAL`. This is to prevent one committer from hogging the database.
///
/// - Number of pending rows. If this exceeds `H::BATCH_SIZE`, the committer will attempt to write
///   out at most `H::CHUNK_SIZE` worth of rows to the DB.
///
/// If a write fails, the commiter will save the batch it meant to write and try to write them
/// again at the next opportunity, potentially adding more rows to the batch if more have arrived
/// in the interim.
///
/// This task will shutdown if canceled via the `cancel` token, or if the channel it receives data
/// on has been closed by the handler for some reason.
fn committer<H: Handler + 'static>(
    db: Db,
    mut rx: mpsc::Receiver<Indexed<H>>,
    config: CommitterConfig,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    spawn_monitored_task!(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.commit_interval);
        let mut cool = interval(COOLDOWN_INTERVAL);

        // We don't care about keeping a regular cadence -- these intervals are used to guarantee
        // things are spaced at out relative to each other.
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
        cool.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Buffer to gather the next batch to write. This may be non-empty at the top of a tick of
        // the commiter's loop if the previous attempt at a write failed. Attempt is incremented
        // every time a batch write fails, and is reset when it succeeds.
        let mut attempt = 0;
        let mut batch = vec![];

        // Data for checkpoints that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, Vec<H::Value>> = BTreeMap::new();
        let mut pending_rows = 0;

        // TODO: Track the watermark (keep track of out-of-order commits, and update the watermark
        // table when we accumulate a run of contiguous commits starting from the current watermark).

        loop {
            tokio::select! {
                // Break ties in favour of operations that reduce the size of the buffer.
                biased;

                _ = cancel.cancelled() => {
                    info!(pipline = H::NAME, "Shutdown received, stopping committer");
                    break;
                }

                // Time to write out another batch of rows, if there are any.
                _ = poll.tick(), if pending_rows > 0 => {
                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Failed to get connection for DB");
                        cool.reset();
                        continue;
                    };

                    let guard = metrics
                        .committer_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    while batch.len() < H::CHUNK_SIZE {
                        let Some(mut entry) = pending.first_entry() else {
                            break;
                        };

                        let values = entry.get_mut();
                        if batch.len() + values.len() > H::CHUNK_SIZE {
                            let mut for_batch = values.split_off(H::CHUNK_SIZE - batch.len());
                            std::mem::swap(values, &mut for_batch);
                            batch.extend(for_batch);
                            break;
                        } else {
                            batch.extend(entry.remove());
                        }
                    }

                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch.len(),
                        pending = pending_rows,
                        "Gathered batch",
                    );

                    // TODO (optimization): Switch to COPY FROM, which should offer faster inserts?
                    //
                    // Note that COPY FROM cannot handle conflicts -- we need a way to gracefully
                    // fail over to `INSERT INTO ... ON CONFLICT DO NOTHING` if we encounter a
                    // conflict, and in that case, we are also subject to the same constraints on
                    // number of bind parameters as we had before.
                    //
                    // Postgres 17 supports an ON_ERROR option for COPY FROM which can ignore bad
                    // rows, but CloudSQL does not support Postgres 17 yet, and this directive only
                    // works if the FORMAT is set to TEXT or CSV, which are less efficient over the
                    // wire.
                    //
                    // The hope is that in the steady state, there will not be conflicts (they
                    // should only show up during backfills, or when we are handing off between
                    // indexers), so we can use a fallback mechanism for those cases but rely on
                    // COPY FROM most of the time.
                    //
                    // Note that the introduction of watermarks also complicates hand-over between
                    // two indexers writing to the same table: They cannot both update the
                    // watermark. One needs to subordinate to the other (or we need to set the
                    // watermark to the max of what is currently set and what was about to be
                    // written).

                    metrics
                        .total_committer_batches_attempted
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .committer_batch_size
                        .with_label_values(&[H::NAME])
                        .observe(batch.len() as f64);

                    let guard = metrics
                        .committer_commit_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    match H::commit(&batch, &mut conn).await {
                        Ok(affected) => {
                            let elapsed = guard.stop_and_record();

                            metrics
                                .total_committer_rows_committed
                                .with_label_values(&[H::NAME])
                                .inc_by(batch.len() as u64);

                            metrics
                                .total_committer_rows_affected
                                .with_label_values(&[H::NAME])
                                .inc_by(affected as u64);

                            debug!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                attempt,
                                affected,
                                committed = batch.len(),
                                pending = pending_rows,
                                "Wrote batch",
                            );

                            pending_rows -= batch.len();
                            attempt = 0;

                            batch.clear();
                            cool.reset();
                        }

                        Err(e) => {
                            let elapsed = guard.stop_and_record();

                            error!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                attempt,
                                committed = batch.len(),
                                pending = pending_rows,
                                "Error writing batch: {e}",
                            );

                            cool.reset_after(RETRY_INTERVAL);
                            attempt += 1;
                        }
                    }

                }

                // If there are enough pending rows, and we've expended the cooldown, reset the
                // commit polling interval so that on the next iteration of the loop, we will write
                // out another batch.
                _ = cool.tick(), if pending_rows > H::BATCH_SIZE => {
                    poll.reset_immediately();
                }

                indexed = rx.recv(), if pending_rows < H::MAX_PENDING_SIZE => {
                    if let Some(Indexed { sequence_number, values }) = indexed {
                        metrics
                            .total_committer_rows_received
                            .with_label_values(&[H::NAME])
                            .inc_by(values.len() as u64);

                        pending_rows += values.len();
                        pending.insert(sequence_number, values);
                    } else {
                        info!(pipeline = H::NAME, "Handler closed channel, stopping committer");
                        break;
                    }
                }
            }
        }
    })
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, mem, sync::Arc, time::Duration};

use mysten_metrics::spawn_monitored_task;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    db::Db,
    handlers::Handler,
    metrics::IndexerMetrics,
    models::watermarks::CommitterWatermark,
    pipeline::{Indexed, PipelineConfig},
};

/// The committer will wait at least this long between commits for any given pipeline.
const COOLDOWN_INTERVAL: Duration = Duration::from_millis(20);

/// The committer will wait at least this long between attempts to commit a failed batch.
const RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// The committer task is responsible for gathering rows to write to the database. It is a single
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
/// If a write fails, the committer will save the batch it meant to write and try to write them
/// again at the next opportunity, potentially adding more rows to the batch if more have arrived
/// in the interim.
///
/// On every successful write of a batch, the committer sends the checkpoint watermarks associated
/// with the batch to the `watermark_tx` channel, where they will be used to update the watermarks
/// table associated with this pipeline.
///
/// This task will shutdown if canceled via the `cancel` token, or if the channel it receives data
/// on has been closed by the handler for some reason.
pub(super) fn committer<H: Handler + 'static>(
    config: PipelineConfig,
    mut indexed_rx: mpsc::Receiver<Indexed<H>>,
    watermark_tx: mpsc::Sender<Vec<CommitterWatermark<'static>>>,
    db: Db,
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
        // the committer's loop if the previous attempt at a write failed. Attempt is incremented
        // every time a batch write fails, and is reset when it succeeds.
        let mut attempt = 0;
        let mut batch_values = vec![];
        let mut batch_watermarks = vec![];

        // Data for checkpoints that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, Indexed<H>> = BTreeMap::new();
        let mut pending_rows = 0;

        info!(pipeline = H::NAME, "Starting committer");

        loop {
            tokio::select! {
                // Break ties in favour of operations that reduce the size of the buffer.
                //
                // TODO (experiment): Do we need this? It adds some complexity/subtlety to this
                // work loop, so if we don't notice a big difference, we should get rid of it.
                biased;

                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received, stopping committer");
                    break;
                }

                // Time to write out another batch of rows, and update the watermark, if we can.
                _ = poll.tick() => {
                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Committer failed to get connection for DB");
                        cool.reset();
                        continue;
                    };

                    let guard = metrics
                        .committer_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    while batch_values.len() < H::CHUNK_SIZE {
                        let Some(mut entry) = pending.first_entry() else {
                            break;
                        };

                        let indexed = entry.get_mut();
                        let values = &mut indexed.values;
                        if batch_values.len() + values.len() > H::CHUNK_SIZE {
                            let mut for_batch = values.split_off(H::CHUNK_SIZE - batch_values.len());
                            std::mem::swap(values, &mut for_batch);
                            batch_values.extend(for_batch);
                            break;
                        } else {
                            let (watermark, values) = entry.remove().into_batch();
                            batch_values.extend(values);
                            batch_watermarks.push(watermark);
                        }
                    }

                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch_values.len(),
                        pending = pending_rows,
                        "Gathered batch",
                    );

                    // TODO (experiment): Switch to COPY FROM, which should offer faster inserts?
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
                        .observe(batch_values.len() as f64);

                    let guard = metrics
                        .committer_commit_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    // TODO (experiment): Parallelize batch writes?
                    //
                    // Previous findings suggest that having about 5 parallel committers per table
                    // yields the best performance. Is that still true for this new architecture?
                    // If we go down this route, we should consider factoring that work out into a
                    // separate task that also handles the watermark.

                    let affected = if batch_values.is_empty() {
                        0
                    } else {
                        match H::commit(&batch_values, &mut conn).await {
                            Ok(affected) => affected,

                            Err(e) => {
                                let elapsed = guard.stop_and_record();

                                error!(
                                    pipeline = H::NAME,
                                    elapsed_ms = elapsed * 1000.0,
                                    attempt,
                                    committed = batch_values.len(),
                                    pending = pending_rows,
                                    "Error writing batch: {e}",
                                );

                                cool.reset_after(RETRY_INTERVAL);
                                attempt += 1;
                                continue;
                            }
                        }
                    };

                    let elapsed = guard.stop_and_record();

                    metrics
                        .total_committer_rows_committed
                        .with_label_values(&[H::NAME])
                        .inc_by(batch_values.len() as u64);

                    metrics
                        .total_committer_rows_affected
                        .with_label_values(&[H::NAME])
                        .inc_by(affected as u64);

                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        attempt,
                        affected,
                        committed = batch_values.len(),
                        pending = pending_rows,
                        "Wrote batch",
                    );

                    pending_rows -= batch_values.len();
                    batch_values.clear();
                    attempt = 0;

                    if config.skip_watermark {
                        batch_watermarks.clear();
                    } else if watermark_tx.send(mem::take(&mut batch_watermarks)).await.is_err() {
                        info!(pipeline = H::NAME, "Watermark closed channel, stopping committer");
                        break;
                    }

                    // TODO (amnn): Test this behaviour (requires tempdb and migrations).
                    if pending_rows == 0 && indexed_rx.is_closed() {
                        info!(pipeline = H::NAME, "Handler closed channel, pending rows empty, stopping committer");
                        break;
                    }

                    cool.reset();
                }

                // If there are enough pending rows, and we've expended the cooldown, reset the
                // commit polling interval so that on the next iteration of the loop, we will write
                // out another batch.
                //
                // TODO (experiment): Do we need this cooldown to deal with contention on the
                // connection pool? It's possible that this is just going to eat into our
                // throughput.
                _ = cool.tick(), if pending_rows > H::BATCH_SIZE => {
                    poll.reset_immediately();
                }

                Some(indexed) = indexed_rx.recv(), if pending_rows < H::MAX_PENDING_SIZE => {
                    metrics
                        .total_committer_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(indexed.values.len() as u64);

                    pending_rows += indexed.values.len();
                    pending.insert(indexed.cp_sequence_number, indexed);
                }
            }
        }
    })
}

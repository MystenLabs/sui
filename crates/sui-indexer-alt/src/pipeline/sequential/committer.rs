// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use diesel_async::{scoped_futures::ScopedFutureExt, AsyncConnection};
use mysten_metrics::spawn_monitored_task;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    db::Db,
    metrics::IndexerMetrics,
    models::watermarks::CommitterWatermark,
    pipeline::{Indexed, PipelineConfig, LOUD_WATERMARK_UPDATE_INTERVAL, WARN_PENDING_WATERMARKS},
};

use super::Handler;

/// The committer task gathers rows into batches and writes them to the database.
///
/// Data arrives out of order, grouped by checkpoint, on `rx`. The task orders them and waits to
/// write them until either a configural polling interval has passed (controlled by
/// `config.collect_interval`), or `H::BATCH_SIZE` rows have been accumulated and we have received
/// the next expected checkpoint.
///
/// Writes are performed on checkpoint boundaries (more than one checkpoint can be present in a
/// single write), in a single transaction that includes all row updates and an update to the
/// watermark table.
///
/// The committer can optionally be configured to lag behind the ingestion service by a fixed
/// number of checkpoints (configured by `checkpoint_lag`).
///
/// Upon successful write, the task sends its new watermark back to the ingestion service, to
/// unblock its regulator.
///
/// The task can be shutdown using its `cancel` token or if either of its channels are closed.
pub(super) fn committer<H: Handler + 'static>(
    config: PipelineConfig,
    checkpoint_lag: Option<u64>,
    watermark: Option<CommitterWatermark<'static>>,
    mut rx: mpsc::Receiver<Indexed<H>>,
    tx: mpsc::UnboundedSender<(&'static str, u64)>,
    db: Db,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    spawn_monitored_task!(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.collect_interval);
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Buffer to gather the next batch to write. A checkpoint's data is only added to the batch
        // when it is known to come from the next checkpoint after `watermark` (the current tip of
        // the batch), and data from previous checkpoints will be discarded to avoid double writes.
        //
        // The batch may be non-empty at top of a tick of the committer's loop if the previous
        // attempt at a write failed. Attempt is incremented every time a batch write fails, and is
        // reset when it succeeds.
        let mut attempt = 0;
        let mut batch = H::Batch::default();
        let mut batch_rows = 0;
        let mut batch_checkpoints = 0;

        // The task keeps track of the highest (inclusive) checkpoint it has added to the batch,
        // and whether that batch needs to be written out. By extension it also knows the next
        // checkpoint to expect and add to the batch.
        let mut watermark_needs_update = false;
        let (mut watermark, mut next_checkpoint) = if let Some(watermark) = watermark {
            let next = watermark.checkpoint_hi_inclusive as u64 + 1;
            (watermark, next)
        } else {
            (CommitterWatermark::initial(H::NAME.into()), 0)
        };

        // The committer task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut next_loud_watermark_update =
            watermark.checkpoint_hi_inclusive + LOUD_WATERMARK_UPDATE_INTERVAL;

        // Data for checkpoint that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, Indexed<H>> = BTreeMap::new();
        let mut pending_rows = 0;

        info!(pipeline = H::NAME, ?watermark, "Starting committer");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    if pending.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = pending.len(),
                            "Pipeline has a large number of pending watermarks",
                        );
                    }

                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Failed to get connection for DB");
                        continue;
                    };

                    // Determine whether we need to hold back checkpoints from being committed
                    // because of checkpoint lag.
                    //
                    // TODO(amnn): Test this (depends on migrations and tempdb)
                    let commit_hi_inclusive = match (checkpoint_lag, pending.last_key_value()) {
                        (Some(lag), None) => {
                            debug!(pipeline = H::NAME, lag, "No pending checkpoints");
                            if rx.is_closed() && rx.is_empty() {
                                info!(pipeline = H::NAME, "Processor closed channel before priming");
                                break;
                            } else {
                                continue;
                            }
                        }

                        (Some(lag), Some((pending_hi, _))) if *pending_hi < lag => {
                            debug!(pipeline = H::NAME, lag, pending_hi, "Priming pipeline");
                            if rx.is_closed() && rx.is_empty() {
                                info!(pipeline = H::NAME, "Processor closed channel while priming");
                                break;
                            } else {
                                continue;
                            }
                        }

                        (Some(lag), Some((pending_hi, _))) => Some(*pending_hi - lag),
                        (None, _) => None,
                    };

                    let guard = metrics
                        .collector_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    // Push data into the next batch as long as it's from contiguous checkpoints,
                    // outside of the checkpoint lag and we haven't gathered information from too
                    // many checkpoints already.
                    //
                    // We don't worry about overall size because the handler may have optimized
                    // writes by combining rows, but we will limit the number of checkpoints we try
                    // and batch together as a way to impose some limit on the size of the batch
                    // (and therefore the length of the write transaction).
                    while batch_checkpoints < H::MAX_BATCH_CHECKPOINTS {
                        let Some(entry) = pending.first_entry() else {
                            break;
                        };

                        if matches!(commit_hi_inclusive, Some(hi) if hi < *entry.key()) {
                            break;
                        }

                        match next_checkpoint.cmp(entry.key()) {
                            // Next pending checkpoint is from the future.
                            Ordering::Less => break,

                            // This is the next checkpoint -- include it.
                            Ordering::Equal => {
                                let indexed = entry.remove();
                                batch_rows += indexed.len();
                                batch_checkpoints += 1;
                                H::batch(&mut batch, indexed.values);
                                watermark = indexed.watermark;
                                watermark_needs_update = true;
                                next_checkpoint += 1;
                            }

                            // Next pending checkpoint is in the past, ignore it to avoid double
                            // writes.
                            Ordering::Greater => {
                                metrics
                                    .total_watermarks_out_of_order
                                    .with_label_values(&[H::NAME])
                                    .inc();
                                let indexed = entry.remove();
                                pending_rows -= indexed.len();
                                continue;
                            }
                        }
                    }

                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch_rows,
                        pending = pending_rows,
                        "Gathered batch",
                    );

                    metrics
                        .collector_batch_size
                        .with_label_values(&[H::NAME])
                        .observe(batch_rows as f64);

                    metrics
                        .total_committer_batches_attempted
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .watermark_epoch
                        .with_label_values(&[H::NAME])
                        .set(watermark.epoch_hi_inclusive);

                    metrics
                        .watermark_checkpoint
                        .with_label_values(&[H::NAME])
                        .set(watermark.checkpoint_hi_inclusive);

                    metrics
                        .watermark_transaction
                        .with_label_values(&[H::NAME])
                        .set(watermark.tx_hi);

                    metrics
                        .watermark_timestamp_ms
                        .with_label_values(&[H::NAME])
                        .set(watermark.timestamp_ms_hi_inclusive);

                    let guard = metrics
                        .committer_commit_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    // Write all the object updates out along with the watermark update, in a
                    // single transaction. The handler's `commit` implementation is responsible for
                    // chunking up the writes into a manageable size.
                    let affected = conn.transaction::<_, anyhow::Error, _>(|conn| async {
                        // TODO: If initial_watermark is empty, when we update watermark
                        // for the first time, we should also update the low watermark.
                        watermark.update(conn).await?;
                        H::commit(&batch, conn).await
                    }.scope_boxed()).await;

                    // Drop the connection eagerly to avoid it holding on to references borrowed by
                    // the transaction closure.
                    drop(conn);

                    let elapsed = guard.stop_and_record();

                    let affected = match affected {
                        Ok(affected) => affected,

                        Err(e) => {
                            warn!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                attempt,
                                committed = batch_rows,
                                pending = pending_rows,
                                "Error writing batch: {e}",
                            );

                            attempt += 1;
                            continue;
                        }
                    };

                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        attempt,
                        affected,
                        committed = batch_rows,
                        pending = pending_rows,
                        "Wrote batch",
                    );

                    metrics
                        .total_committer_batches_succeeded
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .total_committer_rows_committed
                        .with_label_values(&[H::NAME])
                        .inc_by(batch_rows as u64);

                    metrics
                        .total_committer_rows_affected
                        .with_label_values(&[H::NAME])
                        .inc_by(affected as u64);

                    metrics
                        .watermark_epoch_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.epoch_hi_inclusive);

                    metrics
                        .watermark_checkpoint_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.checkpoint_hi_inclusive);

                    metrics
                        .watermark_transaction_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.tx_hi);

                    metrics
                        .watermark_timestamp_in_db_ms
                        .with_label_values(&[H::NAME])
                        .set(watermark.timestamp_ms_hi_inclusive);

                    if watermark.checkpoint_hi_inclusive > next_loud_watermark_update {
                        next_loud_watermark_update += LOUD_WATERMARK_UPDATE_INTERVAL;
                        info!(
                            pipeline = H::NAME,
                            epoch = watermark.epoch_hi_inclusive,
                            checkpoint = watermark.checkpoint_hi_inclusive,
                            transaction = watermark.tx_hi,
                            timestamp = %watermark.timestamp(),
                            "Watermark",
                        );
                    } else {
                        debug!(
                            pipeline = H::NAME,
                            epoch = watermark.epoch_hi_inclusive,
                            checkpoint = watermark.checkpoint_hi_inclusive,
                            transaction = watermark.tx_hi,
                            timestamp = %watermark.timestamp(),
                            "Watermark",
                        );
                    }

                    if watermark_needs_update {
                        // Ignore the result -- the ingestion service will close this channel
                        // once it is done, but there may still be checkpoints buffered that need
                        // processing.
                        let _ = tx.send((H::NAME, watermark.checkpoint_hi_inclusive as u64));
                    }

                    let _ = std::mem::take(&mut batch);
                    watermark_needs_update = false;
                    pending_rows -= batch_rows;
                    batch_checkpoints = 0;
                    batch_rows = 0;
                    attempt = 0;

                    // If there is a pending checkpoint, no greater than the expected next
                    // checkpoint, and less than or equal to the inclusive upperbound due to
                    // checkpoint lag, then the pipeline can do more work immediately (without
                    // waiting).
                    //
                    // Otherwise, if its channels have been closed, we know that it is guaranteed
                    // not to make any more progress, and we can stop the task.
                    if pending
                        .first_key_value()
                        .is_some_and(|(next, _)| {
                            *next <= next_checkpoint && commit_hi_inclusive.map_or(true, |hi| *next <= hi)
                        })
                    {
                        poll.reset_immediately();
                    } else if rx.is_closed() && rx.is_empty() {
                        info!(pipeline = H::NAME, "Processor closed channel, pending rows empty");
                        break;
                    }
                }

                Some(indexed) = rx.recv() => {
                    pending_rows += indexed.len();
                    pending.insert(indexed.checkpoint(), indexed);

                    // Once data has been inserted, check if we need to schedule a write before the
                    // next polling interval. This is appropriate if there are a minimum number of
                    // rows to write, and they are already in the batch, or we can process the next
                    // checkpoint to extract them.

                    if pending_rows < H::MIN_EAGER_ROWS {
                        continue;
                    }

                    if batch_rows > 0 {
                        poll.reset_immediately();
                        continue;
                    }

                    let Some((next, _)) = pending.first_key_value() else {
                        continue;
                    };

                    match (checkpoint_lag, pending.last_key_value()) {
                        (Some(_), None) => continue,
                        (Some(lag), Some((last, _))) if last.saturating_sub(lag) <= *next => {
                            continue;
                        }
                        _ => if *next <= next_checkpoint {
                            poll.reset_immediately();
                        }
                    }
                }
            }
        }

        info!(pipeline = H::NAME, ?watermark, "Stopping committer");
    })
}

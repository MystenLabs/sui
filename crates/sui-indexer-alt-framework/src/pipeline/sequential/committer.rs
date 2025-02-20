// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use diesel_async::{scoped_futures::ScopedFutureExt, AsyncConnection};
use sui_pg_db::Db;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    metrics::IndexerMetrics,
    models::watermarks::CommitterWatermark,
    pipeline::{logging::WatermarkLogger, IndexedCheckpoint, WARN_PENDING_WATERMARKS},
};

use super::{Handler, SequentialConfig};

/// The committer task gathers rows into batches and writes them to the database.
///
/// Data arrives out of order, grouped by checkpoint, on `rx`. The task orders them and waits to
/// write them until either a configural polling interval has passed (controlled by
/// `config.collect_interval()`), or `H::BATCH_SIZE` rows have been accumulated and we have
/// received the next expected checkpoint.
///
/// Writes are performed on checkpoint boundaries (more than one checkpoint can be present in a
/// single write), in a single transaction that includes all row updates and an update to the
/// watermark table.
///
/// The committer can be configured to lag behind the ingestion service by a fixed number of
/// checkpoints (configured by `checkpoint_lag`). A value of `0` means no lag.
///
/// Upon successful write, the task sends its new watermark back to the ingestion service, to
/// unblock its regulator.
///
/// The task can be shutdown using its `cancel` token or if either of its channels are closed.
pub(super) fn committer<H: Handler + 'static>(
    config: SequentialConfig,
    watermark: Option<CommitterWatermark<'static>>,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    tx: mpsc::UnboundedSender<(&'static str, u64)>,
    db: Db,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.committer.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let checkpoint_lag = config.checkpoint_lag;

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
        let (mut watermark, mut next_checkpoint) = if let Some(watermark) = watermark {
            let next = watermark.checkpoint_hi_inclusive as u64 + 1;
            (watermark, next)
        } else {
            (CommitterWatermark::initial(H::NAME.into()), 0)
        };

        // The committer task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut logger = WatermarkLogger::new("sequential_committer", &watermark);

        // Data for checkpoint that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, IndexedCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows = 0;

        info!(pipeline = H::NAME, ?watermark, "Starting committer");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    if batch_checkpoints == 0
                        && rx.is_closed()
                        && rx.is_empty()
                        && !can_process_pending(next_checkpoint, checkpoint_lag, &pending)
                    {
                        info!(pipeline = H::NAME, "Process closed channel and no more data to commit");
                        break;
                    }

                    if pending.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = pending.len(),
                            "Pipeline has a large number of pending watermarks",
                        );
                    }

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
                        if !can_process_pending(next_checkpoint, checkpoint_lag, &pending) {
                            break;
                        }

                        let Some(entry) = pending.first_entry() else {
                            break;
                        };

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

                    // If there is no new data to commit, we can skip the rest of the process. Note
                    // that we cannot use batch_rows for the check, since it is possible that there
                    // are empty checkpoints with no new rows added, but the watermark still needs
                    // to be updated.
                    if batch_checkpoints == 0 {
                        assert_eq!(batch_rows, 0);
                        continue;
                    }

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

                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Failed to get connection for DB");
                        metrics
                            .total_committer_batches_failed
                            .with_label_values(&[H::NAME])
                            .inc();
                        continue;
                    };

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

                            metrics
                                .total_committer_batches_failed
                                .with_label_values(&[H::NAME])
                                .inc();

                            attempt += 1;
                            continue;
                        }
                    };

                    debug!(
                        pipeline = H::NAME,
                        attempt,
                        affected,
                        committed = batch_rows,
                        pending = pending_rows,
                        "Wrote batch",
                    );

                    logger.log::<H>(&watermark, elapsed);

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
                        .committer_tx_rows
                        .with_label_values(&[H::NAME])
                        .observe(affected as f64);

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

                    // Ignore the result -- the ingestion service will close this channel
                    // once it is done, but there may still be checkpoints buffered that need
                    // processing.
                    let _ = tx.send((H::NAME, watermark.checkpoint_hi_inclusive as u64));

                    let _ = std::mem::take(&mut batch);
                    pending_rows -= batch_rows;
                    batch_checkpoints = 0;
                    batch_rows = 0;
                    attempt = 0;

                    // If we could make more progress immediately, then schedule more work without
                    // waiting.
                    if can_process_pending(next_checkpoint, checkpoint_lag, &pending) {
                        poll.reset_immediately();
                    }
                }

                Some(indexed) = rx.recv() => {
                    // Although there isn't an explicit collector in the sequential pipeline,
                    // keeping this metric consistent with concurrent pipeline is useful
                    // to monitor the backpressure from committer to processor.
                    metrics
                        .total_collector_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(indexed.len() as u64);

                    pending_rows += indexed.len();
                    pending.insert(indexed.checkpoint(), indexed);

                    // Once data has been inserted, check if we need to schedule a write before the
                    // next polling interval. This is appropriate if there are a minimum number of
                    // rows to write, and they are already in the batch, or we can process the next
                    // checkpoint to extract them.
                    if pending_rows < H::MIN_EAGER_ROWS {
                        continue;
                    }

                    if batch_checkpoints > 0
                        || can_process_pending(next_checkpoint, checkpoint_lag, &pending)
                    {
                        poll.reset_immediately();
                    }
                }
            }
        }

        info!(pipeline = H::NAME, ?watermark, "Stopping committer");
    })
}

// Tests whether the first checkpoint in the `pending` buffer can be processed immediately, which
// is subject to the following conditions:
//
// - It is at or before the `next_checkpoint` expected by the committer.
// - It is at least `checkpoint_lag` checkpoints before the last checkpoint in the buffer.
fn can_process_pending<T>(
    next_checkpoint: u64,
    checkpoint_lag: u64,
    pending: &BTreeMap<u64, T>,
) -> bool {
    let Some((&first, _)) = pending.first_key_value() else {
        return false;
    };

    let Some((&last, _)) = pending.last_key_value() else {
        return false;
    };

    first <= next_checkpoint && first + checkpoint_lag <= last
}

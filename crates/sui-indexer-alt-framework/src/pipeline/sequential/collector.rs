// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::metrics::IndexerMetrics;
use crate::pipeline::IndexedCheckpoint;
use crate::pipeline::WARN_PENDING_WATERMARKS;
use crate::pipeline::batch_floor::BatchFloor;
use crate::pipeline::sequential::Handler;
use crate::pipeline::sequential::SequentialConfig;

/// A fully-assembled batch handed off from the collector to the committer task.
///
/// Batches are produced strictly in checkpoint order by the collector and consumed in the
/// same order by the committer, so watermarks advance monotonically even with pipelining.
pub(super) struct BatchedRows<H: Handler> {
    pub batch: H::Batch,
    pub watermark: CommitterWatermark,
    pub batch_rows: usize,
}

/// Collector task — drains `rx` into a reorder buffer, assembles batches in checkpoint order,
/// and hands each ready batch off to the committer via `committer_tx`.
///
/// Data arrives out of order, grouped by checkpoint, on `rx`. The collector orders them, and
/// whenever the next expected checkpoint has arrived and the committer has room for another batch
/// (`committer_tx` has a free slot), it cuts a batch from the contiguous run of checkpoints
/// starting there. While the committer is busy, checkpoints keep accumulating, so batches grow
/// under back-pressure, up to `max_batch_checkpoints`. If `config.committer.min_batch_rows` is
/// set, a batch is only cut once that many rows are pending (counting rows buffered for
/// checkpoints that arrived ahead of their predecessors), or once rows have been pending for
/// `config.committer.max_batch_delay()`. When the processor closes its channel, every checkpoint
/// that can still be written in order is written out.
///
/// Backpressure: when `committer_tx` is full (bounded by `pipeline_depth`), the collector stops
/// cutting batches. It keeps reading `rx` into the reorder buffer until `max_pending_rows` are
/// pending at a time, so the processor→collector channel fills and the adaptive fanout / ingest
/// controllers cut upstream concurrency.
///
/// `max_pending_rows` is a soft cap: when exceeded, the inner drain loop yields so a batch can be
/// cut. `rx.recv()` itself is never gated, because the missing predecessor for a stuck `pending`
/// may still be sitting in `rx`; hard-gating receive would risk deadlock.
pub(super) fn collector<H: Handler>(
    handler: Arc<H>,
    config: SequentialConfig,
    mut next_checkpoint: u64,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    metrics: Arc<IndexerMetrics>,
    max_pending_rows: usize,
    max_batch_checkpoints: usize,
    committer_tx: mpsc::Sender<BatchedRows<H>>,
) -> Service {
    Service::new().spawn_aborting(async move {
        // The task keeps track of the highest (inclusive) checkpoint it has added to a batch
        // through `next_checkpoint`. By extension it also knows the next checkpoint to expect.

        // Data for checkpoints that haven't been written yet.
        let mut pending: BTreeMap<u64, IndexedCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows: usize = 0;
        let mut floor = BatchFloor::new(&config.committer);

        info!(pipeline = H::NAME, "Starting collector");

        loop {
            let ready = has_ready_checkpoint(next_checkpoint, &pending);
            let batch_due = ready && floor.is_reached(pending_rows);

            tokio::select! {
                biased;

                permit = committer_tx.reserve(), if batch_due => {
                    let Ok(permit) = permit else {
                        info!(
                            pipeline = H::NAME,
                            "Committer task closed; stopping collector"
                        );
                        return Ok(());
                    };

                    if let Some(batched) = gather_batch(
                        &*handler,
                        &mut next_checkpoint,
                        &mut pending,
                        &mut pending_rows,
                        max_batch_checkpoints,
                        &metrics,
                    ) {
                        permit.send(batched);
                    }

                    floor.set_pending(!pending.is_empty());
                }

                received = rx.recv() => {
                    // The processor has stopped, so write out everything that can be written.
                    let Some(mut indexed) = received else {
                        break;
                    };

                    // Eagerly drain the channel to avoid scheduler ping-pong. `rx.recv`
                    // itself is never gated — the reorder buffer must stay unbounded,
                    // because a missing predecessor may still be sitting in `rx` and
                    // capping receive could deadlock. The `max_pending_rows` check inside
                    // the drain loop only yields so that a batch can be cut, it is not a
                    // hard-cap on the amount of rows buffered.
                    let mut recv_rows = 0usize;
                    loop {
                        recv_rows += indexed.len();
                        pending_rows += indexed.len();
                        pending.insert(indexed.checkpoint(), indexed);

                        if pending_rows >= max_pending_rows {
                            break;
                        }

                        match rx.try_recv() {
                            Ok(next) => indexed = next,
                            Err(_) => break,
                        }
                    }

                    // Although there isn't an explicit collector in the sequential pipeline,
                    // keeping this metric consistent with the concurrent pipeline is useful
                    // to monitor the backpressure from committer to processor.
                    metrics
                        .total_collector_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(recv_rows as u64);

                    floor.set_pending(true);

                    if pending.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = pending.len(),
                            "Pipeline has a large number of pending watermarks",
                        );
                    }
                }

                () = floor.delay_elapsed(), if ready && !batch_due => {}
            }
        }

        while has_ready_checkpoint(next_checkpoint, &pending) {
            let Some(batched) = gather_batch(
                &*handler,
                &mut next_checkpoint,
                &mut pending,
                &mut pending_rows,
                max_batch_checkpoints,
                &metrics,
            ) else {
                continue;
            };

            if committer_tx.send(batched).await.is_err() {
                info!(
                    pipeline = H::NAME,
                    "Committer task closed; stopping collector"
                );
                return Ok(());
            }
        }

        info!(
            pipeline = H::NAME,
            "Processor closed channel and no more data to commit"
        );
        Ok(())
    })
}

/// Cut the next batch from the contiguous run of checkpoints in `pending` that starts at
/// `next_checkpoint`, taking at most `max_batch_checkpoints` of them, and discarding stale
/// duplicates of checkpoints that were already written. Returns `None` if no checkpoint was taken.
fn gather_batch<H: Handler>(
    handler: &H,
    next_checkpoint: &mut u64,
    pending: &mut BTreeMap<u64, IndexedCheckpoint<H>>,
    pending_rows: &mut usize,
    max_batch_checkpoints: usize,
    metrics: &IndexerMetrics,
) -> Option<BatchedRows<H>> {
    let guard = metrics
        .collector_gather_latency
        .with_label_values(&[H::NAME])
        .start_timer();

    let mut batch = H::Batch::default();
    let mut batch_rows = 0;
    let mut batch_checkpoints = 0;
    let mut watermark: Option<CommitterWatermark> = None;

    // Push data into the next batch as long as it's from contiguous checkpoints and we haven't
    // gathered information from too many checkpoints already.
    //
    // We don't worry about overall size because the handler may have optimized writes by
    // combining rows, but we will limit the number of checkpoints we try and batch together as a
    // way to impose some limit on the size of the batch (and therefore the length of the write
    // transaction).
    // docs::#batch  (see https://docs.sui.io/develop/accessing-data/custom-indexer/custom-indexers)
    while batch_checkpoints < max_batch_checkpoints {
        let Some(entry) = pending.first_entry() else {
            break;
        };

        match (*next_checkpoint).cmp(entry.key()) {
            // Next pending checkpoint is from the future.
            Ordering::Less => break,

            // This is the next checkpoint -- include it.
            Ordering::Equal => {
                let indexed = entry.remove();
                batch_rows += indexed.len();
                batch_checkpoints += 1;
                handler.batch(&mut batch, indexed.values.into_iter());
                watermark = Some(indexed.watermark);
                *next_checkpoint += 1;
            }

            // Next pending checkpoint is in the past, ignore it to avoid double writes.
            Ordering::Greater => {
                metrics
                    .total_watermarks_out_of_order
                    .with_label_values(&[H::NAME])
                    .inc();

                let indexed = entry.remove();
                *pending_rows -= indexed.len();
            }
        }
    }
    // docs::/#batch

    let elapsed = guard.stop_and_record();
    debug!(
        pipeline = H::NAME,
        elapsed_ms = elapsed * 1000.0,
        rows = batch_rows,
        pending = *pending_rows,
        "Gathered batch",
    );

    let watermark = watermark?;
    *pending_rows -= batch_rows;

    metrics
        .collector_batch_size
        .with_label_values(&[H::NAME])
        .observe(batch_rows as f64);

    metrics
        .watermark_epoch
        .with_label_values(&[H::NAME])
        .set(watermark.epoch_hi_inclusive as i64);

    metrics
        .watermark_checkpoint
        .with_label_values(&[H::NAME])
        .set(watermark.checkpoint_hi_inclusive as i64);

    metrics
        .watermark_transaction
        .with_label_values(&[H::NAME])
        .set(watermark.tx_hi as i64);

    metrics
        .watermark_timestamp_ms
        .with_label_values(&[H::NAME])
        .set(watermark.timestamp_ms_hi_inclusive as i64);

    Some(BatchedRows {
        batch,
        watermark,
        batch_rows,
    })
}

// Whether the first entry in `pending` is ready to be consumed by the collector — either to be
// included in the next batch (if it matches `next_checkpoint`) or discarded as a stale duplicate
// (if it predates `next_checkpoint`).
fn has_ready_checkpoint<T>(next_checkpoint: u64, pending: &BTreeMap<u64, T>) -> bool {
    pending
        .first_key_value()
        .is_some_and(|(&first, _)| first <= next_checkpoint)
}

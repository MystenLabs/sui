// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio::time::interval;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::metrics::IndexerMetrics;
use crate::pipeline::IndexedCheckpoint;
use crate::pipeline::WARN_PENDING_WATERMARKS;
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
/// Data arrives out of order, grouped by checkpoint, on `rx`. The collector orders them and
/// waits to dispatch them until either a configurable polling interval has passed (controlled
/// by `config.collect_interval()`), or `H::BATCH_SIZE` rows have been accumulated and the next
/// expected checkpoint has arrived.
///
/// Backpressure: when `committer_tx` is full (bounded by `pipeline_depth`), the collector
/// blocks on send and stops reading `rx`, so the processor→collector channel fills and the
/// adaptive fanout / ingest controllers cut upstream concurrency.
pub(super) fn collector<H: Handler>(
    handler: Arc<H>,
    config: SequentialConfig,
    mut next_checkpoint: u64,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    metrics: Arc<IndexerMetrics>,
    min_eager_rows: usize,
    max_batch_checkpoints: usize,
    committer_tx: mpsc::Sender<BatchedRows<H>>,
) -> Service {
    Service::new().spawn_aborting(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.committer.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Buffer to gather the next batch to write. A checkpoint's data is only added to the batch
        // when it is known to come from the next checkpoint after `watermark` (the current tip of
        // the batch), and data from previous checkpoints will be discarded to avoid double writes.
        let mut batch = H::Batch::default();
        let mut batch_rows = 0;
        let mut batch_checkpoints = 0;

        // The task keeps track of the highest (inclusive) checkpoint it has added to the batch
        // through `next_checkpoint`. By extension it also knows the next checkpoint to expect and
        // add to the batch.
        let mut watermark: Option<CommitterWatermark> = None;

        // Data for checkpoint that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, IndexedCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows: usize = 0;

        info!(pipeline = H::NAME, "Starting collector");

        loop {
            tokio::select! {
                _ = poll.tick() => {
                    if batch_checkpoints == 0
                        && rx.is_closed()
                        && rx.is_empty()
                        && !has_ready_checkpoint(next_checkpoint, &pending)
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

                    // Push data into the next batch as long as it's from contiguous checkpoints
                    // and we haven't gathered information from too many checkpoints already.
                    //
                    // We don't worry about overall size because the handler may have optimized
                    // writes by combining rows, but we will limit the number of checkpoints we try
                    // and batch together as a way to impose some limit on the size of the batch
                    // (and therefore the length of the write transaction).
                    // docs::#batch  (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    while batch_checkpoints < max_batch_checkpoints {
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
                                handler.batch(&mut batch, indexed.values.into_iter());
                                watermark = Some(indexed.watermark);
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
                    // docs::/#batch

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
                    // to be updated. Conversely, if there is no watermark to be updated, we know
                    // there is no data to write out.
                    if batch_checkpoints == 0 {
                        assert_eq!(batch_rows, 0);
                        continue;
                    }

                    let Some(watermark) = watermark else {
                        continue;
                    };

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

                    // Hand the assembled batch off to the committer. When `committer_tx` is full,
                    // this blocks — natural backpressure: the collector stops reading `rx`, the
                    // processor→collector channel fills, the adaptive `fanout` controller cuts
                    // processor concurrency, and the broadcaster→processor channel eventually
                    // fills as well, signalling the ingest controller to throttle fetches.
                    let batched = BatchedRows {
                        batch: std::mem::take(&mut batch),
                        watermark,
                        batch_rows,
                    };
                    if committer_tx.send(batched).await.is_err() {
                        info!(pipeline = H::NAME, "Committer task closed; stopping collector");
                        break;
                    }

                    pending_rows -= batch_rows;
                    batch_checkpoints = 0;
                    batch_rows = 0;

                    // If we could make more progress immediately, then schedule more work without
                    // waiting.
                    if has_ready_checkpoint(next_checkpoint, &pending) {
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
                    if pending_rows < min_eager_rows {
                        continue;
                    }

                    if batch_checkpoints > 0 || has_ready_checkpoint(next_checkpoint, &pending) {
                        poll.reset_immediately();
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping collector");
        Ok(())
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

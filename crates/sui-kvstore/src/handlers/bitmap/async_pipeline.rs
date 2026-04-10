// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Async streaming write pipeline for [`super::BitmapIndexHandler`].
//!
//! The handler's `commit()` pushes a `(batch, watermark)` onto `commit_tx`
//! and returns immediately. Four roles make up the pipeline:
//!
//! - **Distributor** (tokio task): drains `commit_rx` serially, assigns a
//!   monotonic `commit_gen`, partitions the commit's values by
//!   `shard_for(row_key) & SHARD_MASK`, and fans out one
//!   [`ShardWorkMsg::Work`] per shard (even empty) plus a single
//!   [`CoordMsg::CommitBegin`] to the coord. The distributor is cheap and
//!   serial so `commit_gen` is an ordered id.
//! - **Shard tasks** (`NUM_SHARDS` × `tokio::spawn`): one tokio task per
//!   shard owns its `ShardState` by value. Share-nothing is preserved by
//!   single-consumer channel discipline: exactly one task drains each
//!   shard's `work_rx` and `feedback_rx`. A `tokio::select!` biased toward
//!   feedback plus a greedy `try_recv` drain keeps the task busy until
//!   both channels are empty without re-parking.
//! - **Write loop** (tokio): chunks WriteRows and pipes concurrent
//!   `MutateRows` RPCs.
//! - **Coord** (tokio task): a [`CommitAggregator`] per in-flight
//!   commit_gen tracks the number of `ShardCommitDone` acks still owed.
//!   Aggregators may close out of order, but a [`ReorderBuffer`] pops
//!   them back into strict `commit_gen` order before their watermarks
//!   are pushed to `pending_watermarks`.
//!
//! See the per-row oldest-unwritten-cp invariants in `accumulated.rs`.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bytes::Bytes;
use futures::StreamExt;
use futures::stream;
use mysten_common::zip_debug_eq::ZipDebugEqIteratorExt;
use prometheus::Histogram;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_histogram_with_registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;
use rustc_hash::FxHashSet;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::Watermark;
use crate::bigtable::client::BigTableClient;
use crate::bigtable::client::PartialWriteError;
use crate::handlers::bitmap::BitmapIndexValue;
use crate::handlers::bitmap::accumulated::NUM_SHARDS;
use crate::handlers::bitmap::accumulated::ShardState;
use crate::handlers::bitmap::accumulated::shard_for;
use crate::handlers::bitmap::reorder_buffer::ReorderBuffer;
use crate::rate_limiter::CompositeRateLimiter;
use crate::tables;

const WRITE_RETRY_BACKOFF: Duration = Duration::from_millis(100);
const COMMIT_OBSERVED_TS_SOFT_CAP_MULT: usize = 10;

/// Bounded capacity of each per-shard work channel. Small — distributor
/// fills fast; shards drain greedily. When full, distributor blocks on
/// `.send()` → `commit_tx` fills → `commit()` blocks → framework
/// throttles.
const SHARD_WORK_CHANNEL_CAPACITY: usize = 4;

/// Bounded capacity of the coord's inbound channel. See the comment
/// where the channel is created for sizing rationale.
const COORD_CHANNEL_CAPACITY: usize = 4096;

pub(crate) type CommitGen = u64;
pub(crate) type ShardId = u16;

/// Smallest `bucket_id` whose seal covers `tx_hi`. `seal_fn(b)` is the
/// (exclusive) upper bound of bucket `b`'s tx range, so the bucket
/// containing `tx_hi` is the first `b` where `seal_fn(b) > tx_hi`. Buckets
/// are power-of-two-sized in practice; the small linear walk is fine —
/// for `bucket_size = 2^20` and `tx_hi < 2^60`, this is bounded by 40 steps
/// and runs only on graduations and coord startup.
#[inline]
pub(crate) fn bucket_of(tx_hi: u64, seal_fn: fn(u64) -> u64) -> u64 {
    let mut b: u64 = 0;
    while seal_fn(b) <= tx_hi {
        b += 1;
    }
    b
}

/// A single write destined for BigTable, serialized on a shard task and
/// shipped to the write loop.
pub(crate) struct WriteRow {
    pub row_key: Bytes,
    pub serialized: Bytes,
    pub max_ts_ms: u64,
    pub oldest_unwritten_cp: u64,
    pub emit_version: u64,
}

/// Row identifier returned to the owning shard after a successful write.
pub(crate) struct DurableRow {
    pub row_key: Bytes,
    pub emit_version: u64,
    pub oldest_unwritten_cp: u64,
}

/// Reference into the distributor's retained batch. Avoids cloning
/// `BitmapIndexValue`s into per-shard work messages: the shard resolves
/// `(arc_index, value_index)` against its own `batch: Arc<Vec<Arc<...>>>`
/// clone, which bumps only two `Arc` refcounts regardless of value count.
pub(crate) struct WorkValueRef {
    pub arc_index: u32,
    pub value_index: u32,
}

/// Message from framework writer → distributor. Shape preserved from the
/// previous design so `handler.rs` needs no change to the send path.
pub(crate) enum MergeMsg {
    Commit {
        batch: Vec<Arc<Vec<BitmapIndexValue>>>,
        watermark: CommitterWatermark,
        commit_observed_at: Instant,
    },
    /// Test-only barrier — acked after all shards ack the barrier AND the
    /// coord observes a fully quiesced pipeline.
    #[cfg(test)]
    Barrier { ack: oneshot::Sender<()> },
}

/// Distributor → shard (BOUNDED per-shard channel).
pub(crate) enum ShardWorkMsg {
    Work {
        commit_gen: CommitGen,
        watermark: CommitterWatermark,
        batch: Arc<Vec<Arc<Vec<BitmapIndexValue>>>>,
        value_refs: Vec<WorkValueRef>,
    },
    #[cfg(test)]
    Barrier { barrier_id: u64 },
}

/// Coord → shard (UNBOUNDED per-shard channel). Unbounded is load-bearing:
/// a bounded feedback channel would close the backpressure cycle
/// `rows_tx → shard → coord → shard`, deadlocking the pipeline. Message
/// volume here is bounded in practice by the number of in-flight
/// WriteRows, which is bounded by `rows_tx`'s capacity.
pub(crate) enum ShardFeedbackMsg {
    Durable { rows: Vec<DurableRow> },
    Remerge { row_keys: Vec<Bytes> },
    SweepEviction,
}

/// Shard / write-loop / distributor / watermark-writer → coord (BOUNDED).
///
/// Bounded so that a slow coord applies backpressure to its producers,
/// which cascades up to the framework's adaptive ingestion controller.
/// Unbounded would let `WriteDone` messages (each carrying chunk-size
/// worth of serialized `Bytes`) pile up without limit when the coord
/// falls behind.
pub(crate) enum CoordMsg {
    /// Sent by the distributor BEFORE any `ShardWorkMsg::Work` for the
    /// same `commit_gen`. Seeds the aggregator so every subsequent
    /// `ShardCommitDone` has somewhere to land.
    CommitBegin {
        commit_gen: CommitGen,
        watermark: CommitterWatermark,
        commit_observed_at: Instant,
    },
    ShardCommitDone {
        commit_gen: CommitGen,
        shard_id: ShardId,
        emitted_oldest_cps: Vec<u64>,
        /// Per-shard backfill signal: nothing this shard saw was worth
        /// promoting. A commit promotes iff AT LEAST ONE shard observed
        /// something (`!all_suppressed`).
        suppress: bool,
    },
    WriteDone {
        rows: Vec<WriteRow>,
        result: Vec<bool>,
    },
    /// Move a row's in-flight accounting from `old_cp` to `new_cp` after a
    /// `handle_durable` inline re-emit.
    InflightMoved { old_cp: u64, new_cp: u64 },
    /// Decrement `inflight[old_cp]` for a row that went clean on
    /// `handle_durable`.
    InflightCleared { old_cp: u64 },
    /// Sent by the dedicated watermark writer after
    /// `set_pipeline_watermark` succeeds. The coord pops
    /// `drained_count` entries from `pending_watermarks`, advances the
    /// persisted-tx-hi mirror, and broadcasts `SweepEviction`.
    WatermarkDone {
        top_tx_hi: u64,
        drained_count: usize,
        /// `checkpoint_hi_inclusive` from the original request; the
        /// coord asserts this matches its in-flight slot.
        expected_cp: u64,
    },
    /// Test-only: distributor assigned `barrier_id` to this barrier; coord
    /// records it and acks once the pipeline is quiesced and all shards
    /// have acked the barrier. The ID is drawn from its own counter —
    /// barriers must NOT share `next_commit_gen` with real commits,
    /// because a barrier never produces a `CommitBegin`, which would
    /// leave a permanent `None` slot in the coord's `ReorderBuffer` and
    /// freeze graduation of any later commit.
    #[cfg(test)]
    BarrierBegin {
        barrier_id: u64,
        ack: oneshot::Sender<()>,
    },
    #[cfg(test)]
    BarrierShardAck { barrier_id: u64, shard_id: ShardId },
}

/// Coord → watermark-writer request. The writer processes one at a
/// time, retries on failure, and reports back via
/// [`CoordMsg::WatermarkDone`]. Serial processing preserves the
/// monotonic-watermarks invariant.
pub(crate) struct WatermarkReq {
    watermark: CommitterWatermark,
    drained_count: usize,
    top_tx_hi: u64,
    /// `checkpoint_hi_inclusive` of the commit that first put a transaction
    /// into `bucket_of(watermark.tx_hi)`. Persisted alongside the watermark
    /// so `init_watermark` on the next restart can clamp to bucket start.
    /// `0` when unknown (very first restart after Deploy 1 before any
    /// bucket transition has occurred).
    bucket_start_cp: u64,
}

/// Coord-local aggregator for one in-flight commit. Inflight-tracking
/// (`inflight_by_oldest_cp`) is populated as each shard reports in, not
/// when the aggregator graduates; see the comment in the
/// `ShardCommitDone` handler for why.
struct CommitAggregator {
    watermark: CommitterWatermark,
    commit_observed_at: Instant,
    shards_remaining: u32,
    all_suppressed: bool,
}

pub(crate) struct BitmapIndexMetrics {
    pub commit_queue_depth: IntGauge,
    /// Registered for scrape visibility; updated from the write loop.
    pub rows_to_write_depth: IntGauge,
    pub inflight_writes: IntGauge,
    pub pending_watermarks_depth: IntGauge,
    /// Sampled each coord_loop iteration. Growth here indicates coord
    /// is falling behind its inbound rate — the likely causes are
    /// `set_pipeline_watermark` latency blocking the loop, or a flood
    /// of `WriteDone`s (each carrying chunk-size worth of serialized
    /// Bytes). If this gauge climbs while memory climbs, the coord is
    /// the bottleneck.
    pub coord_inbox_depth: IntGauge,
    pub coord_inbox_depth_max: IntGauge,
    pub merge_latency: Histogram,
    pub write_chunk_latency: Histogram,
    pub watermark_lag_ms: Histogram,
    pub remerge_count: IntCounter,
}

impl BitmapIndexMetrics {
    pub(crate) fn new(pipeline: &'static str, registry: &Registry) -> Arc<Self> {
        let latency_buckets = prometheus::exponential_buckets(0.0005, 2.0, 18).unwrap();
        let lag_buckets = prometheus::exponential_buckets(1.0, 2.0, 18).unwrap();
        Arc::new(Self {
            commit_queue_depth: register_int_gauge_with_registry!(
                format!("bitmap_commit_queue_depth_{pipeline}"),
                "Depth of the (batch, watermark) channel between framework writer and distributor",
                registry,
            )
            .unwrap(),
            rows_to_write_depth: register_int_gauge_with_registry!(
                format!("bitmap_rows_to_write_depth_{pipeline}"),
                "Depth of the rows-to-write channel between shards and write loop",
                registry,
            )
            .unwrap(),
            inflight_writes: register_int_gauge_with_registry!(
                format!("bitmap_inflight_writes_{pipeline}"),
                "Concurrent in-flight BigTable write RPCs",
                registry,
            )
            .unwrap(),
            pending_watermarks_depth: register_int_gauge_with_registry!(
                format!("bitmap_pending_watermarks_depth_{pipeline}"),
                "Length of the coordinator's pending_watermarks queue",
                registry,
            )
            .unwrap(),
            coord_inbox_depth: register_int_gauge_with_registry!(
                format!("bitmap_coord_inbox_depth_{pipeline}"),
                "Depth of the coordinator's CoordMsg inbox, sampled each loop iteration",
                registry,
            )
            .unwrap(),
            coord_inbox_depth_max: register_int_gauge_with_registry!(
                format!("bitmap_coord_inbox_depth_max_{pipeline}"),
                "High-water mark of the coordinator's CoordMsg inbox depth since startup",
                registry,
            )
            .unwrap(),
            merge_latency: register_histogram_with_registry!(
                format!("bitmap_merge_latency_seconds_{pipeline}"),
                "Time from CommitBegin to shards_remaining==0 per commit_gen",
                latency_buckets.clone(),
                registry,
            )
            .unwrap(),
            write_chunk_latency: register_histogram_with_registry!(
                format!("bitmap_write_chunk_latency_seconds_{pipeline}"),
                "BigTable MutateRows latency per chunk",
                latency_buckets,
                registry,
            )
            .unwrap(),
            watermark_lag_ms: register_histogram_with_registry!(
                format!("bitmap_watermark_lag_ms_{pipeline}"),
                "Wall-clock ms from commit observed until its watermark is persisted",
                lag_buckets,
                registry,
            )
            .unwrap(),
            remerge_count: register_int_counter_with_registry!(
                format!("bitmap_remerge_count_total_{pipeline}"),
                "Rows routed from write loop back to shards after write failure",
                registry,
            )
            .unwrap(),
        })
    }

    pub(crate) fn noop() -> Arc<Self> {
        Self::new_with_unique_prefix(&Registry::new())
    }

    fn new_with_unique_prefix(registry: &Registry) -> Arc<Self> {
        use std::sync::atomic::AtomicUsize;
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let name: &'static str = Box::leak(format!("test_{n}").into_boxed_str());
        Self::new(name, registry)
    }
}

/// Handles owned by the handler: senders into the pipeline + task join
/// handles.
pub(crate) struct PipelineHandles {
    pub commit_tx: mpsc::Sender<MergeMsg>,
    #[allow(dead_code)]
    pub distributor_handle: JoinHandle<()>,
    #[allow(dead_code)]
    pub write_handle: JoinHandle<()>,
    #[allow(dead_code)]
    pub coord_handle: JoinHandle<()>,
    #[allow(dead_code)]
    pub watermark_handle: JoinHandle<()>,
    #[allow(dead_code)]
    pub shard_handles: Vec<JoinHandle<()>>,
    pub min_seal_mirrors: Arc<[AtomicU64]>,
    #[allow(dead_code)]
    pub metrics: Arc<BitmapIndexMetrics>,
    /// Per-shard mirrors of `ShardState.rows.len()`. Handler's
    /// `accumulated_rows` test helper sums across all shards.
    #[cfg(test)]
    pub accumulated_rows: Arc<[AtomicUsize]>,
}

/// Wire the pipeline and return the handles + senders.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_pipeline(
    pipeline: &'static str,
    table: &'static str,
    column: &'static str,
    seal_fn: fn(u64) -> u64,
    startup_tx_hi: u64,
    startup_bucket_start_cp: u64,
    client: BigTableClient,
    rate_limiter: Arc<CompositeRateLimiter>,
    flush_write_chunk_size: usize,
    flush_write_concurrency: usize,
    flush_only_when_sealed: bool,
    commit_channel_capacity: usize,
    metrics: Arc<BitmapIndexMetrics>,
) -> PipelineHandles {
    // `startup_tx_hi` is used only to seed the coord's `current_bucket_id`;
    // shards no longer need it since straddler reconciliation is gone.
    let (commit_tx, commit_rx) = mpsc::channel::<MergeMsg>(commit_channel_capacity);
    let rows_channel_capacity = flush_write_chunk_size
        .saturating_mul(flush_write_concurrency)
        .max(1);
    let (rows_tx, rows_rx) = mpsc::channel::<WriteRow>(rows_channel_capacity);
    // Bounded so a slow coord applies backpressure to its producers.
    // Capacity chosen to absorb a worst-case BigTable-watermark-write
    // stall: at ~5k CoordMsg/sec and ~500ms stall, ~2500 queue; 4096
    // gives headroom. A too-tight value would manifest as frequent
    // `.send().await` blocking in shards / write loop / writer under
    // steady state; a too-loose one would defeat the fix.
    let (coord_tx, coord_rx) = mpsc::channel::<CoordMsg>(COORD_CHANNEL_CAPACITY);
    // Coord → dedicated watermark writer. Single-slot semantics in
    // practice (coord holds an in-flight gate); capacity 8 gives
    // cheap headroom without costing anything.
    let (watermark_req_tx, watermark_req_rx) = mpsc::channel::<WatermarkReq>(8);

    // Per-shard min-seal mirrors. Handler reads the min across them as a
    // tripwire for backfill-mode fast paths; shards update their slot
    // whenever their `EvictionIndex` front advances.
    let min_seal_mirrors: Arc<[AtomicU64]> = (0..NUM_SHARDS)
        .map(|_| AtomicU64::new(u64::MAX))
        .collect::<Vec<_>>()
        .into();
    // Coord's last successfully-persisted watermark `tx_hi`. Advanced via
    // `fetch_max` on every promote; read by each shard for sealed+clean
    // eviction.
    let latest_persisted_tx_hi = Arc::new(AtomicU64::new(0));

    #[cfg(test)]
    let accumulated_rows: Arc<[AtomicUsize]> = (0..NUM_SHARDS)
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>()
        .into();

    // Per-shard channels.
    let mut shard_work_senders: Vec<mpsc::Sender<ShardWorkMsg>> = Vec::with_capacity(NUM_SHARDS);
    let mut shard_work_receivers: Vec<mpsc::Receiver<ShardWorkMsg>> =
        Vec::with_capacity(NUM_SHARDS);
    let mut shard_feedback_senders: Vec<mpsc::UnboundedSender<ShardFeedbackMsg>> =
        Vec::with_capacity(NUM_SHARDS);
    let mut shard_feedback_receivers: Vec<mpsc::UnboundedReceiver<ShardFeedbackMsg>> =
        Vec::with_capacity(NUM_SHARDS);
    for _ in 0..NUM_SHARDS {
        let (wtx, wrx) = mpsc::channel::<ShardWorkMsg>(SHARD_WORK_CHANNEL_CAPACITY);
        #[allow(clippy::disallowed_methods)]
        let (ftx, frx) = mpsc::unbounded_channel::<ShardFeedbackMsg>();
        shard_work_senders.push(wtx);
        shard_work_receivers.push(wrx);
        shard_feedback_senders.push(ftx);
        shard_feedback_receivers.push(frx);
    }

    let mut shard_handles: Vec<JoinHandle<()>> = Vec::with_capacity(NUM_SHARDS);
    for (i, (work_rx, feedback_rx)) in shard_work_receivers
        .into_iter()
        .zip_debug_eq(shard_feedback_receivers)
        .enumerate()
    {
        let rows_tx = rows_tx.clone();
        let coord_tx = coord_tx.clone();
        let state = ShardState::new(
            i as ShardId,
            table,
            seal_fn,
            flush_only_when_sealed,
            Arc::clone(&min_seal_mirrors),
            latest_persisted_tx_hi.clone(),
            #[cfg(test)]
            Arc::clone(&accumulated_rows),
        );
        let handle = tokio::spawn(shard_task(state, work_rx, feedback_rx, rows_tx, coord_tx));
        shard_handles.push(handle);
    }

    let distributor_handle = tokio::spawn(distributor_loop(
        pipeline,
        commit_rx,
        shard_work_senders,
        coord_tx.clone(),
    ));
    drop(rows_tx);

    let write_handle = tokio::spawn(write_loop(
        pipeline,
        table,
        column,
        client.clone(),
        rate_limiter,
        rows_rx,
        coord_tx.clone(),
        flush_write_chunk_size,
        flush_write_concurrency,
        metrics.clone(),
    ));

    let watermark_handle = tokio::spawn(watermark_writer_loop(
        pipeline,
        client,
        watermark_req_rx,
        coord_tx.clone(),
    ));
    drop(coord_tx);

    let coord_handle = tokio::spawn(coord_loop(
        pipeline,
        coord_rx,
        shard_feedback_senders,
        metrics.clone(),
        commit_channel_capacity,
        latest_persisted_tx_hi,
        watermark_req_tx,
        seal_fn,
        startup_tx_hi,
        startup_bucket_start_cp,
    ));

    PipelineHandles {
        commit_tx,
        distributor_handle,
        write_handle,
        coord_handle,
        watermark_handle,
        shard_handles,
        min_seal_mirrors,
        metrics,
        #[cfg(test)]
        accumulated_rows,
    }
}

async fn distributor_loop(
    pipeline: &'static str,
    mut commit_rx: mpsc::Receiver<MergeMsg>,
    shard_work_senders: Vec<mpsc::Sender<ShardWorkMsg>>,
    coord_tx: mpsc::Sender<CoordMsg>,
) {
    info!(pipeline, "Bitmap distributor started");

    let mut next_commit_gen: CommitGen = 0;
    // Barriers use their own ID space so they never land in the coord's
    // `ReorderBuffer` (which is indexed by `CommitGen` and requires every
    // slot to receive a `CommitBegin`). Sharing the counter would leave
    // gap slots after each barrier that permanently block graduation of
    // later commits.
    #[cfg(test)]
    let mut next_barrier_id: u64 = 0;
    while let Some(msg) = commit_rx.recv().await {
        match msg {
            MergeMsg::Commit {
                batch,
                watermark,
                commit_observed_at,
            } => {
                let commit_gen = next_commit_gen;
                next_commit_gen += 1;

                // Pre-announce the commit so the coord's aggregator is
                // ready before any shard can ack.
                if coord_tx
                    .send(CoordMsg::CommitBegin {
                        commit_gen,
                        watermark,
                        commit_observed_at,
                    })
                    .await
                    .is_err()
                {
                    warn!(pipeline, "Coord closed during CommitBegin send");
                    break;
                }

                // Partition value refs by shard.
                let mut by_shard: Vec<Vec<WorkValueRef>> =
                    (0..NUM_SHARDS).map(|_| Vec::new()).collect();
                for (arc_index, arc_vec) in batch.iter().enumerate() {
                    for (value_index, v) in arc_vec.iter().enumerate() {
                        by_shard[shard_for(&v.row_key)].push(WorkValueRef {
                            arc_index: arc_index as u32,
                            value_index: value_index as u32,
                        });
                    }
                }

                let batch_arc = Arc::new(batch);
                for (shard_id, refs) in by_shard.into_iter().enumerate() {
                    // Send Work to every shard — empty refs still required
                    // for post-value dirty-sweep and uniform aggregator
                    // accounting.
                    let work = ShardWorkMsg::Work {
                        commit_gen,
                        watermark,
                        batch: Arc::clone(&batch_arc),
                        value_refs: refs,
                    };
                    if shard_work_senders[shard_id].send(work).await.is_err() {
                        warn!(pipeline, shard_id, "Shard closed during Work send");
                        return;
                    }
                }
            }
            #[cfg(test)]
            MergeMsg::Barrier { ack } => {
                let barrier_id = next_barrier_id;
                next_barrier_id += 1;
                if coord_tx
                    .send(CoordMsg::BarrierBegin { barrier_id, ack })
                    .await
                    .is_err()
                {
                    break;
                }
                for shard_tx in shard_work_senders.iter() {
                    if shard_tx
                        .send(ShardWorkMsg::Barrier { barrier_id })
                        .await
                        .is_err()
                    {
                        warn!(pipeline, "Shard closed during Barrier broadcast");
                        return;
                    }
                }
            }
        }
    }

    info!(pipeline, "Bitmap distributor exiting");
}

async fn shard_task(
    mut state: ShardState,
    mut work_rx: mpsc::Receiver<ShardWorkMsg>,
    mut feedback_rx: mpsc::UnboundedReceiver<ShardFeedbackMsg>,
    rows_tx: mpsc::Sender<WriteRow>,
    coord_tx: mpsc::Sender<CoordMsg>,
) {
    let shard_id = state.shard_id;
    let table = state.table;
    info!(table, shard_id, "Bitmap shard task started");

    loop {
        // Block on whichever channel has a message first. Prefer feedback
        // when both are ready — it's cheap to process and unblocks the
        // coord. If we starved feedback, Durable messages would pile up,
        // `InflightCleared` would never fire, watermarks would never
        // promote, evictions would never run, and memory would balloon.
        tokio::select! {
            biased;
            Some(fb) = feedback_rx.recv() => {
                handle_shard_feedback(&mut state, fb, &rows_tx, &coord_tx).await;
            }
            Some(w) = work_rx.recv() => {
                handle_shard_work(&mut state, w, &rows_tx, &coord_tx).await;
            }
            else => break,
        }

        // Greedy drain: absorb queued messages without re-parking. Feedback
        // first each pass (same rationale as the select arm above).
        loop {
            let mut progress = false;
            while let Ok(fb) = feedback_rx.try_recv() {
                handle_shard_feedback(&mut state, fb, &rows_tx, &coord_tx).await;
                progress = true;
            }
            while let Ok(w) = work_rx.try_recv() {
                handle_shard_work(&mut state, w, &rows_tx, &coord_tx).await;
                progress = true;
            }
            if !progress {
                break;
            }
        }
    }

    info!(table, shard_id, "Bitmap shard task exiting");
}

async fn handle_shard_work(
    state: &mut ShardState,
    msg: ShardWorkMsg,
    rows_tx: &mpsc::Sender<WriteRow>,
    coord_tx: &mpsc::Sender<CoordMsg>,
) {
    match msg {
        ShardWorkMsg::Work {
            commit_gen,
            watermark,
            batch,
            value_refs,
        } => {
            let tx_hi = watermark.tx_hi;
            let commit_cp = watermark.checkpoint_hi_inclusive;

            // Resolve references against the distributor's retained
            // batch Arc — zero value clones.
            let values: Vec<&BitmapIndexValue> = value_refs
                .iter()
                .map(|r| &batch[r.arc_index as usize][r.value_index as usize])
                .collect();

            let current_min_seal = state.current_min_dirty_seal_tx_hi();
            let pre_merge_any_sealed =
                state.flush_only_when_sealed && current_min_seal.is_some_and(|m| m <= tx_hi);

            let mut emitted_oldest_cps: Vec<u64> = Vec::new();
            state
                .merge_shard(
                    &values,
                    tx_hi,
                    commit_cp,
                    pre_merge_any_sealed,
                    rows_tx,
                    &mut emitted_oldest_cps,
                )
                .await;

            let suppress = state.flush_only_when_sealed && emitted_oldest_cps.is_empty();
            if coord_tx
                .send(CoordMsg::ShardCommitDone {
                    commit_gen,
                    shard_id: state.shard_id,
                    emitted_oldest_cps,
                    suppress,
                })
                .await
                .is_err()
            {
                warn!(
                    table = state.table,
                    shard_id = state.shard_id,
                    "Coord closed during ShardCommitDone send"
                );
            }
        }
        #[cfg(test)]
        ShardWorkMsg::Barrier { barrier_id } => {
            let _ = coord_tx
                .send(CoordMsg::BarrierShardAck {
                    barrier_id,
                    shard_id: state.shard_id,
                })
                .await;
        }
    }
}

async fn handle_shard_feedback(
    state: &mut ShardState,
    msg: ShardFeedbackMsg,
    rows_tx: &mpsc::Sender<WriteRow>,
    coord_tx: &mpsc::Sender<CoordMsg>,
) {
    match msg {
        ShardFeedbackMsg::Durable { rows } => {
            state.handle_durable(rows, rows_tx, coord_tx).await;
        }
        ShardFeedbackMsg::Remerge { row_keys } => {
            state.handle_remerge(&row_keys, rows_tx).await;
        }
        ShardFeedbackMsg::SweepEviction => {
            state.handle_sweep_eviction();
        }
    }
}

async fn write_loop(
    pipeline: &'static str,
    table: &'static str,
    column: &'static str,
    client: BigTableClient,
    rate_limiter: Arc<CompositeRateLimiter>,
    rows_rx: mpsc::Receiver<WriteRow>,
    coord_tx: mpsc::Sender<CoordMsg>,
    chunk_size: usize,
    write_concurrency: usize,
    metrics: Arc<BitmapIndexMetrics>,
) {
    info!(pipeline, "Bitmap write loop started");

    let depth_metric = metrics.rows_to_write_depth.clone();
    let rows_stream = stream::unfold(rows_rx, move |mut rx| {
        let depth_metric = depth_metric.clone();
        async move {
            let row = rx.recv().await?;
            depth_metric.set(rx.len() as i64);
            Some((row, rx))
        }
    });

    let completion_stream = rows_stream
        .ready_chunks(chunk_size)
        .map(|chunk: Vec<WriteRow>| {
            let mut client = client.clone();
            let rate_limiter = rate_limiter.clone();
            let metrics = metrics.clone();
            async move {
                rate_limiter.acquire(chunk.len()).await;
                metrics.inflight_writes.inc();
                let entries: Vec<_> = chunk
                    .iter()
                    .map(|r| {
                        tables::make_entry(
                            r.row_key.clone(),
                            [(column, r.serialized.clone())],
                            Some(r.max_ts_ms),
                        )
                    })
                    .collect();

                let write_start = Instant::now();
                let result = client.write_entries(table, entries.clone()).await;
                let result = match result {
                    Ok(()) => Ok(()),
                    Err(e) if e.is::<PartialWriteError>() => Err(e),
                    Err(_transient) => {
                        tokio::time::sleep(WRITE_RETRY_BACKOFF).await;
                        client.write_entries(table, entries).await
                    }
                };
                metrics
                    .write_chunk_latency
                    .observe(write_start.elapsed().as_secs_f64());
                metrics.inflight_writes.dec();

                let success_mask = split_write_result(&result, &chunk);
                CoordMsg::WriteDone {
                    rows: chunk,
                    result: success_mask,
                }
            }
        })
        .buffer_unordered(write_concurrency);

    tokio::pin!(completion_stream);
    while let Some(msg) = completion_stream.next().await {
        if coord_tx.send(msg).await.is_err() {
            warn!(pipeline, "Coordinator closed during WriteDone send");
            break;
        }
    }

    info!(pipeline, "Bitmap write loop exiting");
    drop(coord_tx);
}

fn split_write_result(result: &anyhow::Result<()>, chunk: &[WriteRow]) -> Vec<bool> {
    match result {
        Ok(()) => vec![true; chunk.len()],
        Err(e) => {
            if let Some(partial) = e.downcast_ref::<PartialWriteError>() {
                let failed: FxHashSet<&Bytes> =
                    partial.failed_keys.iter().map(|f| &f.key).collect();
                chunk.iter().map(|r| !failed.contains(&r.row_key)).collect()
            } else {
                vec![false; chunk.len()]
            }
        }
    }
}

/// Dedicated task that owns `set_pipeline_watermark` calls. Drains
/// requests from the coord serially, retries BigTable failures with
/// backoff, and reports success back via `CoordMsg::WatermarkDone`.
/// Serial processing preserves monotonicity: the coord sends requests
/// in `pending_watermarks` order; BigTable sees them in that order.
async fn watermark_writer_loop(
    pipeline: &'static str,
    mut client: BigTableClient,
    mut req_rx: mpsc::Receiver<WatermarkReq>,
    coord_tx: mpsc::Sender<CoordMsg>,
) {
    info!(pipeline, "Bitmap watermark writer started");

    while let Some(req) = req_rx.recv().await {
        let pw: Watermark = req.watermark.into();
        loop {
            match client
                .set_pipeline_watermark(pipeline, &pw, Some(req.bucket_start_cp))
                .await
            {
                Ok(()) => break,
                Err(e) => {
                    error!(pipeline, %e, "set_pipeline_watermark failed; retrying");
                    tokio::time::sleep(WRITE_RETRY_BACKOFF).await;
                }
            }
        }
        let done = CoordMsg::WatermarkDone {
            top_tx_hi: req.top_tx_hi,
            drained_count: req.drained_count,
            expected_cp: req.watermark.checkpoint_hi_inclusive,
        };
        if coord_tx.send(done).await.is_err() {
            info!(pipeline, "Watermark writer: coord closed; exiting");
            return;
        }
    }

    info!(pipeline, "Bitmap watermark writer exiting");
}

#[allow(clippy::too_many_arguments)]
async fn coord_loop(
    pipeline: &'static str,
    mut coord_rx: mpsc::Receiver<CoordMsg>,
    shard_feedback_senders: Vec<mpsc::UnboundedSender<ShardFeedbackMsg>>,
    metrics: Arc<BitmapIndexMetrics>,
    commit_channel_capacity: usize,
    latest_persisted_tx_hi: Arc<AtomicU64>,
    watermark_req_tx: mpsc::Sender<WatermarkReq>,
    seal_fn: fn(u64) -> u64,
    startup_tx_hi: u64,
    startup_bucket_start_cp: u64,
) {
    info!(pipeline, "Bitmap coordinator started");

    // Each pending watermark is paired with the `bucket_start_cp` that was
    // current when it graduated. `try_promote` uses the paired value so
    // that coalesced promotions persist the correct bucket start for the
    // selected `top`.
    let mut pending_watermarks: VecDeque<(CommitterWatermark, u64)> = VecDeque::new();
    // Tracks the bucket containing the most recent graduated watermark's
    // `tx_hi` and the `checkpoint_hi_inclusive` of the commit that first
    // contributed a transaction to that bucket. Seeded at startup from
    // `startup_tx_hi` and the persisted `bitmap_bucket_start_cp` column;
    // advanced in `try_graduate_commits` whenever a graduating watermark's
    // bucket differs from the current one.
    let mut current_bucket_id: u64 = bucket_of(startup_tx_hi, seal_fn);
    let mut current_bucket_start_cp: u64 = startup_bucket_start_cp;
    let mut inflight_by_oldest_cp: BTreeMap<u64, usize> = BTreeMap::new();
    let mut commit_observed_ts: HashMap<u64, Instant> = HashMap::new();
    let mut commits: ReorderBuffer<CommitAggregator> = ReorderBuffer::new();
    // `checkpoint_hi_inclusive` of the watermark currently being
    // persisted by the dedicated writer, or None if no request is in
    // flight. At most one request at a time; preserves monotonic-
    // watermark ordering. Cleared on WatermarkDone.
    let mut in_flight_watermark_cp: Option<u64> = None;
    #[cfg(test)]
    let mut pending_barriers: HashMap<u64, (oneshot::Sender<()>, u32)> = HashMap::new();
    let soft_cap = commit_channel_capacity.saturating_mul(COMMIT_OBSERVED_TS_SOFT_CAP_MULT);

    // Warn + rate-limit: log once every `N` samples when inbox is deep.
    const COORD_INBOX_WARN_THRESHOLD: usize = 1000;
    const COORD_INBOX_WARN_EVERY: u64 = 500;
    let mut coord_loop_iter: u64 = 0;
    let mut coord_inbox_high_water: usize = 0;

    while let Some(msg) = coord_rx.recv().await {
        let depth = coord_rx.len();
        metrics.coord_inbox_depth.set(depth as i64);
        if depth > coord_inbox_high_water {
            coord_inbox_high_water = depth;
            metrics.coord_inbox_depth_max.set(depth as i64);
        }
        coord_loop_iter += 1;
        if depth > COORD_INBOX_WARN_THRESHOLD
            && coord_loop_iter.is_multiple_of(COORD_INBOX_WARN_EVERY)
        {
            warn!(
                pipeline,
                coord_inbox_depth = depth,
                coord_inbox_high_water,
                "coord inbox backing up; possible set_pipeline_watermark stall \
                 or WriteDone flood"
            );
        }
        match msg {
            CoordMsg::CommitBegin {
                commit_gen,
                watermark,
                commit_observed_at,
            } => {
                commits.insert(
                    commit_gen,
                    CommitAggregator {
                        watermark,
                        commit_observed_at,
                        shards_remaining: NUM_SHARDS as u32,
                        all_suppressed: true,
                    },
                );
                let cp = watermark.checkpoint_hi_inclusive;
                commit_observed_ts.insert(cp, commit_observed_at);
                if commit_observed_ts.len() > soft_cap {
                    warn!(
                        pipeline,
                        size = commit_observed_ts.len(),
                        soft_cap,
                        "commit_observed_ts grew past soft cap; evicting oldest"
                    );
                    if let Some((&oldest, _)) = commit_observed_ts.iter().min_by_key(|(_, v)| **v) {
                        commit_observed_ts.remove(&oldest);
                    }
                }
            }
            CoordMsg::ShardCommitDone {
                commit_gen,
                shard_id,
                emitted_oldest_cps,
                suppress,
            } => {
                // Increment inflight entries HERE, not at graduation.
                //
                // Ordering invariant: the shard serializes its sends —
                // ShardCommitDone precedes any InflightCleared it will
                // emit for rows touched in this commit (the shard's
                // select loop can't process `Durable` feedback until
                // `merge_shard` returns, and `merge_shard` sends
                // `ShardCommitDone` as its last act). With this +1
                // applied on ShardCommitDone receipt, the matching
                // InflightCleared always finds a live entry — even if
                // another shard's ShardCommitDone is delayed and
                // graduation hasn't fired yet.
                //
                // Graduation-time increments (the previous design)
                // raced: a fast shard could complete its write round
                // trip before the slowest shard's ShardCommitDone
                // arrived, leaving InflightCleared with no entry to
                // decrement; the subsequent graduation-time +1 then
                // became permanently stuck, pinning buffer_min and
                // blocking watermark promotion indefinitely.
                for cp in &emitted_oldest_cps {
                    *inflight_by_oldest_cp.entry(*cp).or_insert(0) += 1;
                }
                if let Some(agg) = commits.get_mut(commit_gen) {
                    agg.shards_remaining = agg.shards_remaining.saturating_sub(1);
                    if !suppress {
                        agg.all_suppressed = false;
                    }
                } else {
                    warn!(
                        pipeline,
                        commit_gen, shard_id, "ShardCommitDone for unknown commit"
                    );
                }
                try_graduate_commits(
                    pipeline,
                    &mut commits,
                    &mut pending_watermarks,
                    &metrics,
                    seal_fn,
                    &mut current_bucket_id,
                    &mut current_bucket_start_cp,
                );
                try_promote(
                    pipeline,
                    &mut pending_watermarks,
                    &inflight_by_oldest_cp,
                    &mut in_flight_watermark_cp,
                    &watermark_req_tx,
                    &metrics,
                )
                .await;
            }
            CoordMsg::WriteDone { rows, result } => {
                assert_eq!(
                    rows.len(),
                    result.len(),
                    "write loop must produce one result per row"
                );
                // Route per-row to the owning shard's feedback channel.
                let mut by_shard_durable: Vec<Vec<DurableRow>> =
                    (0..NUM_SHARDS).map(|_| Vec::new()).collect();
                let mut by_shard_remerge: Vec<Vec<Bytes>> =
                    (0..NUM_SHARDS).map(|_| Vec::new()).collect();
                let mut remerge_total = 0usize;
                for (i, row) in rows.into_iter().enumerate() {
                    let s = shard_for(&row.row_key);
                    if result[i] {
                        by_shard_durable[s].push(DurableRow {
                            row_key: row.row_key,
                            emit_version: row.emit_version,
                            oldest_unwritten_cp: row.oldest_unwritten_cp,
                        });
                    } else {
                        by_shard_remerge[s].push(row.row_key);
                        remerge_total += 1;
                    }
                }
                for s in 0..NUM_SHARDS {
                    let durable = std::mem::take(&mut by_shard_durable[s]);
                    if !durable.is_empty()
                        && shard_feedback_senders[s]
                            .send(ShardFeedbackMsg::Durable { rows: durable })
                            .is_err()
                    {
                        warn!(pipeline, shard_id = s, "Shard closed; cannot route Durable");
                    }
                    let remerge = std::mem::take(&mut by_shard_remerge[s]);
                    if !remerge.is_empty()
                        && shard_feedback_senders[s]
                            .send(ShardFeedbackMsg::Remerge { row_keys: remerge })
                            .is_err()
                    {
                        warn!(pipeline, shard_id = s, "Shard closed; cannot route Remerge");
                    }
                }
                if remerge_total > 0 {
                    metrics.remerge_count.inc_by(remerge_total as u64);
                }
            }
            CoordMsg::InflightMoved { old_cp, new_cp } => {
                decrement_inflight(&mut inflight_by_oldest_cp, old_cp);
                *inflight_by_oldest_cp.entry(new_cp).or_insert(0) += 1;
                try_promote(
                    pipeline,
                    &mut pending_watermarks,
                    &inflight_by_oldest_cp,
                    &mut in_flight_watermark_cp,
                    &watermark_req_tx,
                    &metrics,
                )
                .await;
            }
            CoordMsg::InflightCleared { old_cp } => {
                decrement_inflight(&mut inflight_by_oldest_cp, old_cp);
                try_promote(
                    pipeline,
                    &mut pending_watermarks,
                    &inflight_by_oldest_cp,
                    &mut in_flight_watermark_cp,
                    &watermark_req_tx,
                    &metrics,
                )
                .await;
            }
            CoordMsg::WatermarkDone {
                top_tx_hi,
                drained_count,
                expected_cp,
            } => {
                debug_assert_eq!(
                    in_flight_watermark_cp,
                    Some(expected_cp),
                    "WatermarkDone for cp {expected_cp} but in-flight slot is {in_flight_watermark_cp:?}",
                );
                in_flight_watermark_cp = None;

                for _ in 0..drained_count {
                    let Some((w, _)) = pending_watermarks.pop_front() else {
                        break;
                    };
                    let cp = w.checkpoint_hi_inclusive;
                    if let Some(start) = commit_observed_ts.remove(&cp) {
                        metrics
                            .watermark_lag_ms
                            .observe(start.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                metrics
                    .pending_watermarks_depth
                    .set(pending_watermarks.len() as i64);

                let prev = latest_persisted_tx_hi.fetch_max(top_tx_hi, Ordering::Relaxed);
                if prev < top_tx_hi {
                    // Drains are idempotent; broadcast unconditionally.
                    for tx in shard_feedback_senders.iter() {
                        let _ = tx.send(ShardFeedbackMsg::SweepEviction);
                    }
                }

                // More promotes may have accumulated while this RPC was
                // in flight. Coalesce them in the next dispatch.
                try_promote(
                    pipeline,
                    &mut pending_watermarks,
                    &inflight_by_oldest_cp,
                    &mut in_flight_watermark_cp,
                    &watermark_req_tx,
                    &metrics,
                )
                .await;
            }
            #[cfg(test)]
            CoordMsg::BarrierBegin { barrier_id, ack } => {
                pending_barriers.insert(barrier_id, (ack, 0));
            }
            #[cfg(test)]
            CoordMsg::BarrierShardAck {
                barrier_id,
                shard_id,
            } => {
                if let Some((_ack, count)) = pending_barriers.get_mut(&barrier_id) {
                    *count += 1;
                    debug!(pipeline, barrier_id, shard_id, "BarrierShardAck received");
                }
            }
        }
        #[cfg(test)]
        {
            // Resolve barriers whose shard count has reached NUM_SHARDS
            // AND the pipeline is fully quiesced.
            if inflight_by_oldest_cp.is_empty()
                && pending_watermarks.is_empty()
                && commits.is_empty()
                && in_flight_watermark_cp.is_none()
            {
                let ready_ids: Vec<u64> = pending_barriers
                    .iter()
                    .filter_map(|(id, (_, count))| {
                        if *count as usize >= NUM_SHARDS {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect();
                for id in ready_ids {
                    if let Some((ack, _)) = pending_barriers.remove(&id) {
                        let _ = ack.send(());
                    }
                }
            }
        }
    }

    info!(pipeline, "Bitmap coordinator exiting");
}

#[allow(clippy::too_many_arguments)]
fn try_graduate_commits(
    pipeline: &'static str,
    commits: &mut ReorderBuffer<CommitAggregator>,
    pending_watermarks: &mut VecDeque<(CommitterWatermark, u64)>,
    metrics: &BitmapIndexMetrics,
    seal_fn: fn(u64) -> u64,
    current_bucket_id: &mut u64,
    current_bucket_start_cp: &mut u64,
) {
    while let Some((commit_gen, agg)) = commits.pop_front_if(|a| a.shards_remaining == 0) {
        metrics
            .merge_latency
            .observe(agg.commit_observed_at.elapsed().as_secs_f64());
        if !agg.all_suppressed {
            // Detect bucket transitions on each graduation so the
            // `bucket_start_cp` persisted alongside future watermarks
            // reflects the commit that actually crossed into the
            // currently-active bucket. Graduations happen in `commit_gen`
            // order, so `agg.watermark.tx_hi` is monotonic; we walk the
            // cursor forward incrementally from `current_bucket_id`
            // rather than re-scanning from 0. Amortized O(1) across the
            // pipeline lifetime vs the previous O(bucket_id) per call.
            // If a single graduation crosses multiple buckets (rare but
            // legal), the loop body updates `current_bucket_start_cp`
            // for each, keeping the invariant that it records the
            // earliest checkpoint to hit any still-active bucket.
            while seal_fn(*current_bucket_id) <= agg.watermark.tx_hi {
                *current_bucket_id += 1;
                *current_bucket_start_cp = agg.watermark.checkpoint_hi_inclusive;
            }
            pending_watermarks.push_back((agg.watermark, *current_bucket_start_cp));
            metrics
                .pending_watermarks_depth
                .set(pending_watermarks.len() as i64);
        } else {
            debug!(
                pipeline,
                commit_gen, "Commit suppressed — nothing sealed; watermark not promoted",
            );
        }
    }
}

fn decrement_inflight(inflight: &mut BTreeMap<u64, usize>, cp: u64) {
    if let Some(e) = inflight.get_mut(&cp) {
        *e = e.saturating_sub(1);
        if *e == 0 {
            inflight.remove(&cp);
        }
    } else {
        error!(cp, "InflightCleared/Moved for unknown oldest_cp");
    }
}

/// Non-blocking: computes the highest promotable watermark (coalescing
/// any intermediate entries in `pending` — only the top tx_hi is
/// actually written to BigTable; the rest are popped in bulk on
/// `WatermarkDone`) and dispatches a single request to the dedicated
/// writer. The RPC runs off the coord's critical path; bookkeeping
/// (pop, lag-observe, mirror advance, SweepEviction broadcast) lives
/// in the `WatermarkDone` handler.
///
/// The `in_flight_watermark_cp` single-slot gate ensures at most one
/// request in flight at a time, which preserves monotonicity of
/// BigTable writes. Under a slow-BigTable scenario, more promotes
/// accumulate in `pending` while waiting; the next dispatch coalesces
/// them into one larger batch. Fewer RPCs under load is a free
/// improvement.
async fn try_promote(
    pipeline: &'static str,
    pending: &mut VecDeque<(CommitterWatermark, u64)>,
    inflight: &BTreeMap<u64, usize>,
    in_flight_watermark_cp: &mut Option<u64>,
    watermark_req_tx: &mpsc::Sender<WatermarkReq>,
    metrics: &BitmapIndexMetrics,
) {
    // One request in flight at a time — preserves monotonic ordering.
    if in_flight_watermark_cp.is_some() {
        return;
    }

    let buffer_min = inflight.keys().next().copied();

    let mut promote_count = 0usize;
    let mut highest: Option<(CommitterWatermark, u64)> = None;
    for (w, bstart) in pending.iter() {
        if let Some(min) = buffer_min
            && w.checkpoint_hi_inclusive >= min
        {
            break;
        }
        highest = Some((*w, *bstart));
        promote_count += 1;
    }

    let Some((top, bucket_start_cp)) = highest else {
        metrics.pending_watermarks_depth.set(pending.len() as i64);
        return;
    };

    let req = WatermarkReq {
        watermark: top,
        drained_count: promote_count,
        top_tx_hi: top.tx_hi,
        bucket_start_cp,
    };
    if watermark_req_tx.send(req).await.is_err() {
        warn!(pipeline, "Watermark writer closed; cannot promote");
        return;
    }
    *in_flight_watermark_cp = Some(top.checkpoint_hi_inclusive);
    debug!(
        pipeline,
        cp = top.checkpoint_hi_inclusive,
        tx_hi = top.tx_hi,
        bucket_start_cp,
        coalesced = promote_count,
        "Watermark batch dispatched to writer"
    );
    metrics.pending_watermarks_depth.set(pending.len() as i64);
}

/// Send a test-only barrier through the pipeline and wait for all shards
/// to ack AND for the pipeline to quiesce.
#[cfg(test)]
pub(crate) async fn barrier_wait(commit_tx: &mpsc::Sender<MergeMsg>) {
    let (tx, rx) = oneshot::channel();
    if commit_tx.send(MergeMsg::Barrier { ack: tx }).await.is_err() {
        return;
    }
    let _ = rx.await;
}

#[cfg(not(test))]
#[allow(dead_code)]
fn _unused_oneshot(_: oneshot::Sender<()>) {}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bitmap index committer.
//!
//! The bitmap pipelines use the framework's sequential mode: `commit()` calls
//! arrive in checkpoint order, and commit `N + 1` is not delivered before
//! commit `N` returns. This module depends on that ordering. At commit `N`, the
//! process has seen every bitmap input through checkpoint `N`, so the watermark
//! for `N` can be published once this module has durably written the row
//! snapshots it scheduled for `N`.
//!
//! This committer exists because the generic sequential store path does not
//! expose the controls bitmap indexing needs:
//!
//! - bitmap rows are long-lived accumulated buckets with deterministic boundaries,
//!   not one framework commit's rows;
//! - merge/serialize work must be parallelized across CPUs, while
//!   the framework's `handler::batch()` is single-threaded.
//!
//! A generation is one framework commit, keyed by `checkpoint_hi_inclusive`.
//! The generation task may send a watermark for checkpoint `N` only after all
//! shards have reported their scheduled row count for `N` and the writer has
//! reported that many durable row writes for `N`. Empty shards still report
//! zero scheduled rows; otherwise a sparse commit could never advance.
//!
//! Actor/message flow:
//!
//! ```text
//! framework commit
//!        │
//!        ▼
//! ┌────────────────┐
//! │ Handle::commit │
//! └──┬──────────┬──┘
//!    │          │
//!    │          └─ shard::Merge x NUM_SHARDS ──▶ ┌────────────────────────────┐
//!    │                                           │ ShardWorker x NUM_SHARDS   │
//!    │                                           └──┬──────────────▲────┬─────┘
//!    │                                              │              │    │
//!    │                                 ShardFlushesScheduled       │    │writer::Batch
//!    │                                              │              │    ▼
//!    │                                              │              │ ┌───────────┐
//!    │                                              │              │ │ Writer    │ ──▶ BigTable rows
//!    │                                              │              │ └──────┬────┘     (retry/backoff)
//!    │                                              │              │        │
//!    │                                              │         shard::Seal   │RowsFlushed
//!    │GenerationStarted                             │          (evict)      │
//!    ▼                                              ▼              │        ▼
//! ┌──────────────────────────────────────────────────────────────────────────┐
//! │                            GenerationWorker                              │
//! └────────────────────────────────────┬─────────────────────────────────────┘
//!                                      │
//!                         watermark::Commit
//!                                      ▼
//!                              ┌─────────────────┐
//!                              │ WatermarkWriter │ ──▶ BigTable watermark
//!                              └─────────────────┘       (retry/backoff)
//! ```
//!
//! Bounded channels provide backpressure to the framework. Per-shard merge
//! channels have capacity `1`; row batches, generation events, and watermark
//! commits use the small capacities below. The only unbounded channel is
//! generation-to-shard [`shard::Seal`]: seal messages are tiny and bucket-rate,
//! and eviction is cleanup, while `GenerationWorker` is the accounting hub.
//! Awaiting shard cleanup capacity there could block it from draining
//! writer/shard progress.
//!
//! Restart safety comes from two pieces of state in the watermark row. The
//! normal `tx_hi` tells the committer which buckets were already sealed before
//! startup, so replayed rows for older buckets are ignored. The bitmap-only
//! `bucket_start_cp` tells `init_watermark` how far to rewind the framework so
//! the active bucket is rebuilt from replay.

use std::hash::Hasher;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use futures::future::try_join_all;
use mysten_common::zip_debug_eq::ZipDebugEqIteratorExt;
use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tracing::warn;

use crate::bigtable::client::BigTableClient;
use crate::handlers::BitmapIndexValue;
use crate::rate_limiter::CompositeRateLimiter;
use crate::store::BitmapInitialWatermarks;

pub(crate) use metrics::BitmapIndexMetrics;

use generation::GenerationWorker;
use shard::Shard;
use shard::ShardWorker;
use watermark::WatermarkWriter;
use writer::Writer;

mod generation;
mod metrics;
mod shard;
#[cfg(test)]
mod tests;
mod watermark;
mod writer;

const COMMIT_RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// Number of hash shards across which accumulated rows are partitioned.
/// Power of two for cheap masking. One tokio task owns each shard for the
/// life of the process. It should be at least `num_cpus`, but making it larger
/// can reduce each shard's active working set and improve cache affinity.
pub(crate) const NUM_SHARDS: usize = 64;
const SHARD_MASK: usize = NUM_SHARDS - 1;

// Compile-time guarantee that `BitmapIndexValue::shard_id` (u8) can hold
// every shard index.
const _: () = assert!(NUM_SHARDS <= u8::MAX as usize + 1);

/// Bounded capacity of each per-shard merge channel. Small — `Handle::commit`
/// fills fast. When full, `Handle::commit` blocks on `.send()` →
/// `Handler::commit` blocks → framework throttles.
const SHARD_MERGE_CHANNEL_CAPACITY: usize = 1;

/// Generation receives `1 + NUM_SHARDS + row-completion-batches` messages per
/// framework commit. Keep enough room for a few commits of bookkeeping without
/// letting this become an unbounded proxy for stalled watermark/row accounting.
const GENERATION_CHANNEL_CAPACITY: usize = NUM_SHARDS * 8;

/// The store connection's view of the committer after [`BitmapCommitter::spawn`]
/// returns: the per-shard work senders. Task `JoinHandle`s live in the
/// returned [`Service`], not here.
#[derive(Clone)]
pub(crate) struct Handle {
    pipeline: &'static str,
    shard_merge_senders: Vec<mpsc::Sender<shard::Merge>>,
    generation_tx: mpsc::Sender<generation::Event>,
    rows_written: Arc<AtomicU64>,
}

impl Handle {
    pub(crate) fn take_rows_written(&self) -> usize {
        self.rows_written.swap(0, Ordering::Relaxed) as usize
    }

    /// Fan out per-shard merge requests. Returns `Err` if any downstream
    /// task has exited.
    ///
    /// `batch` is pre-partitioned by `shard_id` in `Handler::batch`: slot
    /// `i` holds an `Arc` to shard `i`'s values. We clone (refcount bump)
    /// and ship.
    ///
    /// Empty shards still receive a merge — every shard owns one
    /// flush-scheduling unit per commit and must finish it. Skipping empty
    /// shards would leave sparse-commit watermarks unable to promote.
    ///
    /// Backpressure: waits until every shard accepts its merge. A full shard
    /// channel blocks this commit, which cascades up to the framework's writer.
    pub(crate) async fn commit(
        &self,
        batch: Vec<Arc<Vec<BitmapIndexValue>>>,
        watermark: CommitterWatermark,
    ) -> Result<(), ()> {
        debug_assert_eq!(
            batch.len(),
            NUM_SHARDS,
            "Handler::batch must initialize all shard slots",
        );

        let framework_commit_time = Instant::now();
        let checkpoint = watermark.checkpoint_hi_inclusive;
        // This must precede shard fanout: generation state observes commits in
        // the same cp order as the sequential framework calls `commit()`.
        if self
            .generation_tx
            .send(generation::Event::GenerationStarted {
                watermark,
                framework_commit_time,
            })
            .await
            .is_err()
        {
            warn!(
                self.pipeline,
                "Generation task closed during generation start"
            );
            return Err(());
        }
        let sends = self
            .shard_merge_senders
            .iter()
            .zip_debug_eq(batch.into_iter())
            .enumerate()
            .map(|(shard_id, (tx, values))| async move {
                let merge = shard::Merge { checkpoint, values };
                tx.send(merge).await.map_err(|_| shard_id)
            });
        if let Err(shard_id) = try_join_all(sends).await {
            warn!(
                self.pipeline,
                shard_id, "Shard closed during merge request send"
            );
            return Err(());
        }

        Ok(())
    }
}

/// Configuration and dependencies for the async bitmap committer.
/// Populate the fields and call [`Self::spawn`] to wire the background
/// tasks.
pub(crate) struct BitmapCommitter {
    pub pipeline: &'static str,
    pub table: &'static str,
    pub column: &'static str,
    pub is_sealed: fn(u64, CommitterWatermark) -> bool,
    pub initial_watermarks: BitmapInitialWatermarks,
    pub client: BigTableClient,
    pub rate_limiter: Arc<CompositeRateLimiter>,
    pub write_chunk_size: usize,
    pub write_concurrency: usize,
    pub metrics: Arc<BitmapIndexMetrics>,
}

pub(crate) type BitmapCommitterHandle = Handle;
pub(crate) type BucketId = u64;
pub(crate) type ShardId = u16;

impl BitmapCommitter {
    /// Wire the committer. Returns `(Handle, Service)`: the handle is the
    /// store connection's send side, and the `Service` owns
    /// every background task as a primary. The caller is expected to
    /// `merge` the `Service` into the framework's indexer service so
    /// panics propagate and shutdown is coordinated.
    pub(crate) fn spawn(self) -> (Handle, Service) {
        let Self {
            pipeline,
            table,
            column,
            is_sealed,
            initial_watermarks,
            client,
            rate_limiter,
            write_chunk_size,
            write_concurrency,
            metrics,
        } = self;

        // The generation task reads its initial watermark (`tx_hi` and
        // `bucket_start_cp`) from store init state when it receives the first
        // `GenerationStarted`. By then the framework's `init_watermark` call
        // has returned and populated the entry for `pipeline`.
        let (write_tx, write_rx) = mpsc::channel::<writer::Batch>(bounded_channel_capacity());
        // Bounded so a slow generation task applies backpressure to its producers.
        let (generation_tx, generation_rx) =
            mpsc::channel::<generation::Event>(GENERATION_CHANNEL_CAPACITY);
        // Generation task → dedicated BigTable watermark committer. Bounded so
        // a wedged watermark writer eventually backpressures generation.
        let (watermark_commit_tx, watermark_commit_rx) =
            mpsc::channel::<watermark::Commit>(bounded_channel_capacity());
        let rows_written = Arc::new(AtomicU64::new(0));

        // Per-shard channels. Merge inputs are bounded so shard backlog
        // backpressures framework ingestion. Eviction commands are unbounded:
        // they are tiny bucket-rate messages, and eviction is cleanup, while
        // GenerationWorker is the accounting hub. Awaiting shard cleanup
        // capacity there could block it from draining writer/shard progress.
        let mut shard_merge_senders: Vec<mpsc::Sender<shard::Merge>> =
            Vec::with_capacity(NUM_SHARDS);
        let mut shard_merge_receivers: Vec<mpsc::Receiver<shard::Merge>> =
            Vec::with_capacity(NUM_SHARDS);
        let mut shard_seal_senders: Vec<mpsc::UnboundedSender<shard::Seal>> =
            Vec::with_capacity(NUM_SHARDS);
        let mut shard_seal_receivers: Vec<mpsc::UnboundedReceiver<shard::Seal>> =
            Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            let (itx, irx) = mpsc::channel::<shard::Merge>(SHARD_MERGE_CHANNEL_CAPACITY);
            #[allow(clippy::disallowed_methods)]
            let (etx, erx) = mpsc::unbounded_channel::<shard::Seal>();
            shard_merge_senders.push(itx);
            shard_merge_receivers.push(irx);
            shard_seal_senders.push(etx);
            shard_seal_receivers.push(erx);
        }

        // All tasks go into a single `Service` as primary tasks. Task errors
        // and panics surface via tokio's `JoinSet` and are re-raised by the
        // Service's `main()`.
        let mut service = Service::new();
        for (i, (merge_rx, seal_rx)) in shard_merge_receivers
            .into_iter()
            .zip_debug_eq(shard_seal_receivers)
            .enumerate()
        {
            let shard_write_tx = write_tx.clone();
            let generation_tx = generation_tx.clone();
            let metrics = metrics.clone();
            let initial_watermarks = initial_watermarks.clone();
            let state = Shard::new(i as ShardId, table);
            let worker = ShardWorker {
                pipeline,
                shard: state,
                merge_rx,
                seal_rx,
                write_tx: shard_write_tx,
                generation_tx,
                initial_watermarks,
                is_sealed,
                startup_min_unsealed_bucket: Default::default(),
                metrics,
            };
            service = service.spawn(async move { worker.run().await });
        }

        drop(write_tx);

        service = service.spawn({
            let client = client.clone();
            let generation_tx = generation_tx.clone();
            let metrics = metrics.clone();
            let rows_written = rows_written.clone();
            let worker = Writer {
                pipeline,
                table,
                column,
                client,
                rate_limiter,
                write_rx,
                generation_tx,
                write_chunk_size,
                write_concurrency,
                rows_written,
                metrics,
            };
            async move {
                worker.run().await;
                Ok(())
            }
        });

        service = service.spawn({
            let metrics = metrics.clone();
            let worker = WatermarkWriter {
                pipeline,
                client,
                commit_rx: watermark_commit_rx,
                metrics,
            };
            async move {
                worker.run().await;
                Ok(())
            }
        });

        let handle_generation_tx = generation_tx.clone();
        drop(generation_tx);

        service = service.spawn({
            let worker = GenerationWorker {
                pipeline,
                generation_rx,
                shard_seal_senders,
                watermark_commit_tx,
                is_sealed,
                initial_watermarks: initial_watermarks.clone(),
            };
            async move { worker.run().await }
        });

        let handle = Handle {
            pipeline,
            shard_merge_senders,
            generation_tx: handle_generation_tx,
            rows_written,
        };
        (handle, service)
    }
}

/// Default capacity for ordinary bounded control/data channels. Small queues
/// keep backpressure close to the stalled subsystem instead of retaining lots
/// of already-produced work in memory.
fn bounded_channel_capacity() -> usize {
    num_cpus::get().saturating_div(2).max(4)
}

/// Smallest bucket not sealed by `watermark`. The processor's `is_sealed`
/// closure decides which watermark dimension matters; this loop walks until
/// the first unsealed bucket so the next bucket is the only one that may need
/// replay.
fn bucket_of(
    watermark: CommitterWatermark,
    is_sealed: fn(u64, CommitterWatermark) -> bool,
) -> BucketId {
    let mut b: BucketId = 0;
    while is_sealed(b, watermark) {
        b += 1;
    }
    b
}

/// Shard index for a row key. Needs to be deterministic within a single
/// process (handler and shards must agree on routing); cross-process
/// stability isn't required — shard state is entirely in-memory and
/// rebuilt on restart from bucket-start replay.
pub(crate) fn shard_for(key: &[u8]) -> usize {
    let mut h = rustc_hash::FxHasher::default();
    h.write(key);
    (h.finish() as usize) & SHARD_MASK
}

#[cfg(test)]
mod bucket_of_tests {
    use super::*;

    fn is_sealed(b: u64, watermark: CommitterWatermark) -> bool {
        watermark.tx_hi >= (b + 1) * 10
    }

    fn wm(tx_hi: u64) -> CommitterWatermark {
        CommitterWatermark {
            tx_hi,
            ..Default::default()
        }
    }

    /// `is_sealed(b, wm)` true means bucket `b` is sealed, so `bucket_of`
    /// returns the next bucket. Pin the boundary semantics: at `tx_hi`
    /// exactly equal to `(b + 1) * 10`, bucket `b` is sealed.
    #[test]
    fn bucket_of_handles_boundary() {
        assert_eq!(bucket_of(wm(0), is_sealed), 0);
        assert_eq!(bucket_of(wm(9), is_sealed), 0);
        assert_eq!(bucket_of(wm(10), is_sealed), 1, "tx_hi == 10 seals 0");
        assert_eq!(bucket_of(wm(11), is_sealed), 1);
        assert_eq!(bucket_of(wm(20), is_sealed), 2, "tx_hi == 20 seals 1");
        assert_eq!(bucket_of(wm(25), is_sealed), 2);
    }
}

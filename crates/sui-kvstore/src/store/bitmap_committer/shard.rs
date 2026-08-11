// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-shard accumulated bitmap state, owned exclusively by a single
//! bitmap committer shard task running on the tokio runtime.
//!
//! BigTable cells for the bitmap tables are written with `maxversions=1`, so
//! each flush writes the full accumulated bitmap (not a delta). That requires
//! keeping the bits OR'd in over the lifetime of this process in memory.
//!
//! Each `Shard` owns one of `NUM_SHARDS` disjoint slices of the full
//! row space, keyed by `shard_for(row_key)`. Each shard is
//! driven by a single tokio task that holds its `Shard` by value — no
//! `Arc`, no `Mutex`. Share-nothing is preserved by single-consumer channel
//! discipline (exactly one task drains the shard's work channel).
//!
//! On restart, `init_watermark` clamps the resumed checkpoint back to the
//! active bucket's replay floor. Shards ignore replayed rows for buckets
//! already sealed by the persisted `tx_hi`; those persisted rows are
//! authoritative, while the active bucket is rebuilt from normal commit flow.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Result;
use bytes::Bytes;
use roaring::RoaringBitmap;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::handlers::BitmapIndexValue;
use crate::store::BitmapInitialWatermarks;

use super::BitmapIndexMetrics;
use super::BucketId;
use super::ShardId;
use super::generation;
use super::writer;

/// Number of serialized row writes a shard queues per message to the writer.
/// This only amortizes channel overhead; `writer.rs` still owns BigTable
/// request chunking and retry/backoff.
const SHARD_ROW_COMMIT_BATCH_SIZE: usize = 1_000;

thread_local! {
    /// Scratch buffer reused across bitmap serializations on this worker
    /// thread. Queued row writes still copy into `Bytes` so they own their
    /// payload, but reusing this Vec avoids one allocation per serialized row.
    ///
    /// Tokio tasks can migrate between worker threads at await points, so
    /// nothing in this buffer is retained across `.await`s. Every use must
    /// be self-contained within a single `with()` closure: clear, write,
    /// copy out, drop the borrow.
    static SER_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(Vec::with_capacity(32 * 1024));
}

/// Handle → shard merge work on a bounded per-shard channel. The shard merges
/// its slice of one commit's values into accumulated row state, queues
/// row-write batches for changed rows, then marks flush scheduling complete
/// for this shard.
pub(super) struct Merge {
    pub(super) checkpoint: u64,
    /// This shard's slice of the commit's values. Pre-partitioned in
    /// `Handler::batch`, so the shard can iterate directly — no per-value
    /// shard check, no cross-shard indirection. `Arc` keeps the outer
    /// `commit()` fan-out cheap (one refcount bump per shard instead of
    /// cloning `BitmapIndexValue`s).
    pub(super) values: Arc<Vec<BitmapIndexValue>>,
}

/// Generation task → shard (UNBOUNDED per-shard channel).
pub(super) struct Seal {
    /// Every generation that could add bits to buckets below this exclusive
    /// bound is durable, so the shard can drop those bucket maps.
    pub(super) bucket_id_exclusive: BucketId,
}

pub(super) struct ShardWorker {
    pub(super) pipeline: &'static str,
    pub(super) shard: Shard,
    // Bounded ingress: merge backlog should throttle upstream commits.
    pub(super) merge_rx: mpsc::Receiver<Merge>,
    // Unbounded control: seal commands are small and bucket-rate. Eviction is
    // cleanup, but GenerationWorker is the accounting hub; awaiting shard
    // cleanup capacity here could block it from draining writer/shard progress.
    // and deadlock.
    pub(super) seal_rx: mpsc::UnboundedReceiver<Seal>,
    pub(super) write_tx: mpsc::Sender<writer::Batch>,
    pub(super) generation_tx: mpsc::Sender<generation::Event>,
    pub(super) initial_watermarks: BitmapInitialWatermarks,
    pub(super) is_sealed: fn(u64, CommitterWatermark) -> bool,
    pub(super) startup_min_unsealed_bucket: OnceLock<BucketId>,
    pub(super) metrics: Arc<BitmapIndexMetrics>,
}

impl ShardWorker {
    pub(super) async fn run(mut self) -> Result<()> {
        let shard_id = self.shard.shard_id;
        let table = self.shard.table;
        info!(table, shard_id, "Bitmap shard task started");

        loop {
            // Prefer shard seals when both channels are ready. Bucket eviction
            // keeps memory bounded once sealed buckets are fully durable.
            tokio::select! {
                biased;
                Some(seal) = self.seal_rx.recv() => {
                    self.handle_shard_seal(seal).await;
                }
                Some(merge) = self.merge_rx.recv() => {
                    self.handle_shard_merge(merge).await?;
                }
                else => break,
            }
        }

        info!(table, shard_id, "Bitmap shard task exiting");
        Ok(())
    }

    async fn handle_shard_merge(&mut self, merge: Merge) -> Result<()> {
        let checkpoint = merge.checkpoint;
        let min_bucket_to_accumulate = self.min_bucket_to_accumulate()?;
        let mut rows_scheduled = 0u64;
        {
            let mut rows_to_flush = self.shard.merge_in_bitmaps(merge, min_bucket_to_accumulate);
            loop {
                let batch_rows: Vec<_> = rows_to_flush
                    .by_ref()
                    .take(SHARD_ROW_COMMIT_BATCH_SIZE)
                    .inspect(|row| {
                        self.metrics
                            .row_key_size_bytes
                            .observe(row.row_key.len() as f64);
                        self.metrics
                            .serialized_bitmap_size_bytes
                            .observe(row.serialized.len() as f64);
                    })
                    .collect();
                if batch_rows.is_empty() {
                    break;
                }
                rows_scheduled += batch_rows.len() as u64;
                let batch = writer::Batch { rows: batch_rows };
                if self.write_tx.send(batch).await.is_err() {
                    warn!("Writer closed while enqueueing row flushes");
                    return Ok(());
                }
            }
        }
        self.send_shard_flushes_scheduled(checkpoint, rows_scheduled)
            .await;
        Ok(())
    }

    fn min_bucket_to_accumulate(&self) -> Result<BucketId> {
        if let Some(bucket) = self.startup_min_unsealed_bucket.get() {
            return Ok(*bucket);
        }

        let startup = self.initial_watermarks.get(self.pipeline)?;
        let bucket = super::bucket_of(startup.watermark, self.is_sealed);
        let _ = self.startup_min_unsealed_bucket.set(bucket);
        Ok(bucket)
    }

    async fn handle_shard_seal(&mut self, seal: Seal) {
        self.shard.evict_buckets_before(seal.bucket_id_exclusive);
    }

    async fn send_shard_flushes_scheduled(&self, checkpoint: u64, rows_scheduled: u64) {
        if self
            .generation_tx
            .send(generation::Event::ShardFlushesScheduled {
                checkpoint,
                rows_scheduled,
            })
            .await
            .is_err()
        {
            warn!("Generation task closed during ShardFlushesScheduled send");
        }
    }
}

/// Per-row state in the accumulated map. Distinct from the processor's
/// output [`BitmapIndexValue`] so tracking fields stay internal to the
/// committer.
pub(super) struct AccumulatedRow {
    pub bitmap: RoaringBitmap,
    pub max_cp: u64,
    pub max_ts_ms: u64,
}

impl AccumulatedRow {
    /// Fresh row owning no bits. Use with `or_in` to accumulate the
    /// first `BitmapIndexValue`: the or_in path handles both the
    /// first-insert and subsequent-OR cases uniformly.
    fn empty() -> Self {
        Self {
            bitmap: RoaringBitmap::new(),
            max_cp: 0,
            max_ts_ms: 0,
        }
    }

    /// OR `v.bitmap` into this row and update tracking fields. Returns
    /// `true` iff any new bits were added.
    fn or_in(&mut self, v: &BitmapIndexValue) -> bool {
        let before = self.bitmap.len();
        self.bitmap |= &v.bitmap;
        let grew = self.bitmap.len() > before;
        if v.max_cp > self.max_cp {
            self.max_cp = v.max_cp;
            self.max_ts_ms = v.max_ts_ms;
        }
        grew
    }

    /// Serialize the current bitmap into a fresh row write.
    fn make_row_write(&mut self, row_key: Bytes, generation_cp: u64) -> writer::Row {
        self.bitmap.optimize();
        let needed = self.bitmap.serialized_size();
        let serialized = SER_BUF.with(|cell| {
            let mut buf = cell.borrow_mut();
            buf.clear();
            buf.reserve(needed);
            self.bitmap
                .serialize_into(&mut *buf)
                .expect("serialize into Vec is infallible");
            Bytes::copy_from_slice(&buf)
        });
        writer::Row {
            row_key,
            serialized,
            max_ts_ms: self.max_ts_ms,
            generation_cp,
        }
    }
}

/// Per-shard accumulated state. Owned exclusively by a single shard task;
/// no interior locking.
pub(super) struct Shard {
    pub shard_id: ShardId,
    pub table: &'static str,

    /// This shard's slice of the accumulated row map, grouped by bitmap
    /// bucket so sealed buckets can be dropped wholesale.
    pub rows: BTreeMap<BucketId, FxHashMap<Bytes, AccumulatedRow>>,
}

impl Shard {
    pub(super) fn new(shard_id: ShardId, table: &'static str) -> Self {
        Self {
            shard_id,
            table,
            rows: BTreeMap::new(),
        }
    }

    /// Merge this shard's values for one commit into accumulated row state.
    pub(super) fn merge_in_bitmaps<'a>(
        &'a mut self,
        merge: Merge,
        min_bucket_to_accumulate: BucketId,
    ) -> impl Iterator<Item = writer::Row> + 'a {
        let Merge { checkpoint, values } = merge;
        let mut changed_rows = FxHashSet::default();
        for v in values.iter() {
            if v.bucket_id < min_bucket_to_accumulate {
                continue;
            }

            let grew = {
                let row = self
                    .rows
                    .entry(v.bucket_id)
                    .or_default()
                    .entry(v.row_key.clone())
                    .or_insert_with(AccumulatedRow::empty);
                row.or_in(v)
            };

            if grew {
                changed_rows.insert((v.bucket_id, v.row_key.clone()));
            }
        }
        drop(values);

        changed_rows
            .into_iter()
            .filter_map(move |(bucket_id, key)| {
                let row = self
                    .rows
                    .get_mut(&bucket_id)
                    .and_then(|rows| rows.get_mut(&key))?;

                Some(row.make_row_write(key, checkpoint))
            })
    }

    pub(super) fn evict_buckets_before(&mut self, bucket_id_exclusive: BucketId) {
        let mut buckets = 0usize;
        let mut rows = 0usize;
        while let Some((&bucket_id, _)) = self.rows.first_key_value() {
            if bucket_id >= bucket_id_exclusive {
                break;
            }
            let (_, bucket_rows) = self.rows.pop_first().unwrap();
            buckets += 1;
            rows += bucket_rows.len();
        }

        if buckets > 0 {
            debug!(
                table = self.table,
                buckets, rows, bucket_id_exclusive, "Evicted sealed bitmap buckets"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use roaring::RoaringBitmap;

    use super::*;

    const TABLE: &str = "test_bitmap_table";
    fn value(row_key: &[u8], bucket_id: u64, bits: &[u32], max_cp: u64) -> BitmapIndexValue {
        let mut bitmap = RoaringBitmap::new();
        for bit in bits {
            bitmap.insert(*bit);
        }
        BitmapIndexValue {
            row_key: Bytes::copy_from_slice(row_key),
            bucket_id,
            bitmap,
            max_cp,
            max_ts_ms: max_cp * 1000,
            shard_id: 0,
        }
    }

    fn shard() -> Shard {
        Shard::new(0, TABLE)
    }

    fn row_count(shard: &Shard) -> usize {
        shard.rows.values().map(|rows| rows.len()).sum()
    }

    fn merge_and_next_row_write(
        shard: &mut Shard,
        values: Vec<BitmapIndexValue>,
        checkpoint: u64,
    ) -> writer::Row {
        let mut rows = shard.merge_in_bitmaps(
            Merge {
                checkpoint,
                values: Arc::new(values),
            },
            0,
        );
        let mut batch: Vec<_> = rows.by_ref().take(1).collect();
        assert!(rows.next().is_none());
        assert_eq!(batch.len(), 1);
        batch.pop().unwrap()
    }

    fn merge_rows(
        shard: &mut Shard,
        values: Vec<BitmapIndexValue>,
        checkpoint: u64,
        min_bucket_to_accumulate: BucketId,
    ) -> Vec<writer::Row> {
        shard
            .merge_in_bitmaps(
                Merge {
                    checkpoint,
                    values: Arc::new(values),
                },
                min_bucket_to_accumulate,
            )
            .collect()
    }

    #[tokio::test]
    async fn keeps_active_bucket_row_resident_across_merges() {
        let mut shard = shard();
        let row_key = b"row-a";

        let commit1 = merge_and_next_row_write(&mut shard, vec![value(row_key, 0, &[1], 1)], 1);

        assert_eq!(row_count(&shard), 1, "active bucket row stays resident");
        assert_eq!(commit1.generation_cp, 1);

        let commit2 = merge_and_next_row_write(&mut shard, vec![value(row_key, 0, &[2], 2)], 2);

        let row = shard.rows.get(&0).unwrap().get(row_key.as_slice()).unwrap();
        assert!(row.bitmap.contains(1));
        assert!(row.bitmap.contains(2));
        assert_eq!(commit2.generation_cp, 2);
    }

    #[tokio::test]
    async fn duplicate_bits_do_not_schedule_row_write() {
        let mut shard = shard();
        let row_key = b"row-a";

        let first = merge_rows(&mut shard, vec![value(row_key, 0, &[1], 1)], 1, 0);
        let second = merge_rows(&mut shard, vec![value(row_key, 0, &[1], 2)], 2, 0);

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
        assert_eq!(row_count(&shard), 1);
    }

    #[tokio::test]
    async fn merge_ignores_buckets_below_accumulation_floor() {
        let mut shard = shard();
        let sealed_row = b"sealed-row";
        let active_row = b"active-row";

        let writes = merge_rows(
            &mut shard,
            vec![value(sealed_row, 0, &[1], 4), value(active_row, 1, &[2], 4)],
            4,
            1,
        );

        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].row_key, Bytes::copy_from_slice(active_row));
        assert!(!shard.rows.contains_key(&0));
        assert!(shard.rows.contains_key(&1));
    }

    #[tokio::test]
    async fn sealed_bucket_evicted_on_generation_command() {
        let mut shard = shard();
        let row_key = b"row-a";
        merge_and_next_row_write(&mut shard, vec![value(row_key, 0, &[99], 1)], 1);
        assert_eq!(row_count(&shard), 1);

        shard.evict_buckets_before(1);
        assert!(shard.rows.is_empty());
    }
}

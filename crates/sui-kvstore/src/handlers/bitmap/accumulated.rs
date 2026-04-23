// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-shard accumulated bitmap state, owned exclusively by a single
//! bitmap-handler **shard task** running on the tokio runtime.
//!
//! BigTable cells for the bitmap tables are written with `maxversions=1`, so
//! each flush writes the full accumulated bitmap (not a delta). That requires
//! keeping the bits OR'd in over the lifetime of this process in memory.
//!
//! Per-row invariants — the central correctness story of the async pipeline:
//!
//! - `oldest_unwritten_cp: Option<u64>` — `cp_hi` of the earliest commit
//!   whose bits are in `bitmap` but have not yet been durably written.
//!   Set when a row goes from clean to dirty; cleared when a durable write
//!   arrives and no new bits accumulated since emission.
//! - `version: u64` — bumped on every merge that adds bits. Captured at
//!   emit time into the WriteRow's `emit_version`; compared in
//!   `handle_durable` to decide whether new bits arrived since emission.
//! - `emit_pending: bool` — true iff a WriteRow is currently in flight for
//!   this row. Prevents double-emission during further merges.
//! - `next_oldest_unwritten_cp: Option<u64>` — set when new bits arrive
//!   *while* `emit_pending` is true. Becomes the new `oldest_unwritten_cp`
//!   after the in-flight write lands, at which point the row is re-emitted
//!   **inline** from `handle_durable` (critical for sealed-bucket rows that
//!   no future `Commit` will touch).
//!
//! Each `ShardState` owns one of `NUM_SHARDS` disjoint slices of the full
//! row space, keyed by `shard_for(row_key) & SHARD_MASK`. Each shard is
//! driven by a single tokio task that holds its `ShardState` by value — no
//! `Arc`, no `Mutex`. Share-nothing is preserved by single-consumer channel
//! discipline (exactly one task drains the shard's work channel).
//!
//! On restart, `init_watermark` clamps the resumed checkpoint back to the
//! start of the currently-active bucket (via the persisted
//! `bitmap_bucket_start_cp` column), so the partial bucket is re-ingested
//! from scratch. The shard state therefore always starts empty and
//! rebuilds cumulative bitmaps through normal `Commit` flow.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use bytes::Bytes;
use roaring::RoaringBitmap;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::warn;

use crate::handlers::bitmap::BitmapIndexValue;
use crate::handlers::bitmap::async_pipeline::CoordMsg;
use crate::handlers::bitmap::async_pipeline::DurableRow;
use crate::handlers::bitmap::async_pipeline::ShardId;
use crate::handlers::bitmap::async_pipeline::WriteRow;

thread_local! {
    /// Reusable serialization scratch buffer. Thread-local here is
    /// per-tokio-worker (shard tasks migrate between workers), so each
    /// worker amortizes the Vec allocation across every bitmap it
    /// serializes. The final `Bytes::copy_from_slice` copies the
    /// payload into a fresh `Bytes` — the scratch stays hot in the
    /// worker's cache.
    static SER_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(Vec::with_capacity(32 * 1024));
}

/// Number of hash shards across which accumulated rows are partitioned.
/// Power of two for cheap masking. One tokio task owns each shard for the
/// life of the process.
pub(crate) const NUM_SHARDS: usize = 64;
const SHARD_MASK: usize = NUM_SHARDS - 1;

/// Deterministic shard index for a row key. Same key must map to the same
/// shard across all calls in the process — FNV-1a with a finalizer rather
/// than `foldhash::RandomState` (random-seeded by design).
#[inline]
pub(crate) fn shard_for(key: &[u8]) -> usize {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    for &b in key {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    (h as usize) & SHARD_MASK
}

/// Per-row state in the accumulated map. Distinct from the processor's
/// output [`BitmapIndexValue`] so tracking fields stay internal to the
/// handler.
pub(super) struct AccumulatedRow {
    pub bucket_id: u64,
    pub bitmap: RoaringBitmap,
    pub max_cp: u64,
    pub max_ts_ms: u64,
    /// `cp_hi` of the earliest commit whose bits are in `bitmap` but not yet
    /// durable. `None` when the row is clean.
    pub oldest_unwritten_cp: Option<u64>,
    /// Bumped on every merge that ORs new bits.
    pub version: u64,
    /// A WriteRow is in flight for this row — don't emit again until it
    /// lands.
    pub emit_pending: bool,
    /// `cp_hi` of the earliest commit that added bits *while* `emit_pending`
    /// was true. Moves into `oldest_unwritten_cp` in `handle_durable` after
    /// the in-flight write lands.
    pub next_oldest_unwritten_cp: Option<u64>,
}

impl AccumulatedRow {
    fn new(v: &BitmapIndexValue, commit_cp: u64) -> Self {
        Self {
            bucket_id: v.bucket_id,
            bitmap: v.bitmap.clone(),
            max_cp: v.max_cp,
            max_ts_ms: v.max_ts_ms,
            oldest_unwritten_cp: if v.bitmap.is_empty() {
                None
            } else {
                Some(commit_cp)
            },
            version: if v.bitmap.is_empty() { 0 } else { 1 },
            emit_pending: false,
            next_oldest_unwritten_cp: None,
        }
    }

    /// OR `v.bitmap` into this row and update tracking fields. Returns
    /// `true` iff any new bits were added.
    fn or_in(&mut self, v: &BitmapIndexValue, commit_cp: u64) -> bool {
        let before = self.bitmap.len();
        self.bitmap |= &v.bitmap;
        let grew = self.bitmap.len() > before;
        if v.max_cp > self.max_cp {
            self.max_cp = v.max_cp;
            self.max_ts_ms = v.max_ts_ms;
        }
        if grew {
            self.version += 1;
            if self.emit_pending {
                if self.next_oldest_unwritten_cp.is_none() {
                    self.next_oldest_unwritten_cp = Some(commit_cp);
                }
            } else if self.oldest_unwritten_cp.is_none() {
                self.oldest_unwritten_cp = Some(commit_cp);
            }
        }
        grew
    }

    /// Serialize the current bitmap into a fresh WriteRow, setting
    /// `emit_pending = true`. Caller must pass the row_key, the
    /// `oldest_unwritten_cp` to tag (for inline re-emit cases this may
    /// differ from the current field), and whether the bucket is sealed
    /// at emit time.
    fn emit_into(&mut self, row_key: Bytes, oldest_cp: u64, sealed: bool) -> WriteRow {
        let mut bm = std::mem::take(&mut self.bitmap);
        bm.optimize();
        let needed = bm.serialized_size();
        let serialized = SER_BUF.with(|cell| {
            let mut buf = cell.borrow_mut();
            buf.clear();
            buf.reserve(needed);
            bm.serialize_into(&mut *buf)
                .expect("serialize into Vec is infallible");
            Bytes::copy_from_slice(&buf)
        });
        self.bitmap = bm;
        self.emit_pending = true;
        WriteRow {
            row_key,
            serialized,
            max_ts_ms: self.max_ts_ms,
            oldest_unwritten_cp: oldest_cp,
            emit_version: self.version,
            sealed,
        }
    }
}

/// Bucket-keyed eviction index for one shard.
///
/// `by_bucket[bucket_id - front_bucket]` is the set of row keys currently
/// "pending" in that bucket. A row is pending while it is either:
/// - dirty (`oldest_unwritten_cp.is_some()`), or
/// - clean-awaiting-eviction in backfill mode (the bucket's seal isn't yet
///   covered by the persisted watermark).
///
/// Front-empty slots are lazily popped so `front_bucket` reflects the
/// smallest pending bucket_id, giving O(1) `min_seal_tx_hi` reads.
/// `drain_up_to` pops slots once their seal is covered by the persisted
/// watermark and evicts any clean rows from `ShardState.rows`. Idempotent:
/// re-running the drain with the same `persisted` is a no-op once the
/// front has advanced.
pub(super) struct EvictionIndex {
    by_bucket: VecDeque<FxHashSet<Bytes>>,
    front_bucket: u64,
}

impl EvictionIndex {
    fn new() -> Self {
        Self {
            by_bucket: VecDeque::new(),
            front_bucket: 0,
        }
    }

    /// Add `key` to bucket `bucket_id`'s pending set. No-op if the bucket
    /// has already been drained past.
    pub(super) fn insert(&mut self, bucket_id: u64, key: Bytes) {
        if self.by_bucket.is_empty() {
            self.front_bucket = bucket_id;
        }
        if bucket_id < self.front_bucket {
            return;
        }
        let needed = (bucket_id - self.front_bucket) as usize + 1;
        while self.by_bucket.len() < needed {
            self.by_bucket.push_back(FxHashSet::default());
        }
        let idx = (bucket_id - self.front_bucket) as usize;
        self.by_bucket[idx].insert(key);
    }

    /// Remove `key` from bucket `bucket_id`'s pending set. Compacts
    /// front-empty slots.
    ///
    /// Idempotent: calling with a `(bucket_id, key)` pair that isn't
    /// tracked — because the bucket was drained past, the slot was
    /// never grown, or the key was already removed — is a silent no-op.
    /// Callers rely on this: `handle_durable` in non-backfill mode
    /// calls `remove` on every clean-transition without first checking
    /// whether the row was tracked. Do not "tighten" this to a bool
    /// return or debug_assert without auditing those call sites.
    pub(super) fn remove(&mut self, bucket_id: u64, key: &Bytes) {
        if bucket_id < self.front_bucket {
            return;
        }
        let idx = (bucket_id - self.front_bucket) as usize;
        if let Some(slot) = self.by_bucket.get_mut(idx) {
            slot.remove(key);
        }
        self.compact_front();
    }

    fn compact_front(&mut self) {
        while let Some(front) = self.by_bucket.front()
            && front.is_empty()
        {
            self.by_bucket.pop_front();
            self.front_bucket += 1;
        }
    }

    /// `seal_fn(front_bucket)` if any bucket has pending rows, else None.
    pub(super) fn front_seal_tx_hi(&self, seal_fn: fn(u64) -> u64) -> Option<u64> {
        if self.by_bucket.is_empty() {
            None
        } else {
            Some(seal_fn(self.front_bucket))
        }
    }

    /// Drain buckets whose seal is covered by `persisted`. Evicts any
    /// clean rows in popped slots from `rows`.
    ///
    /// Invariant: every key reached here is either clean or missing
    /// from `rows`. Watermark promotion gates on `inflight_by_oldest_cp`:
    /// any dirty row holds a slot there, which keeps `persisted` below
    /// the row's commit_cp, which keeps the row's bucket seal NOT
    /// covered by `persisted`, which keeps the row out of this drain.
    /// The `debug_assert!` catches a break in that invariant at its
    /// source; the release-build `is_none()` check is the defensive
    /// fallback.
    #[cfg(debug_assertions)]
    fn assert_post_sweep_consistency(
        &self,
        seal_fn: fn(u64) -> u64,
        persisted: u64,
        rows: &FxHashMap<Bytes, AccumulatedRow>,
        table: &'static str,
    ) {
        for (offset, slot) in self.by_bucket.iter().enumerate() {
            let bucket_id = self.front_bucket + offset as u64;
            for key in slot {
                let Some(row) = rows.get(key) else { continue };
                debug_assert!(
                    row.oldest_unwritten_cp.is_some() || seal_fn(bucket_id) > persisted,
                    "{table}: eviction index holds clean row key={:?} bucket={} \
                     whose seal {} <= persisted {}; should have been drained",
                    key,
                    bucket_id,
                    seal_fn(bucket_id),
                    persisted,
                );
            }
        }
    }

    pub(super) fn drain_up_to(
        &mut self,
        persisted: u64,
        seal_fn: fn(u64) -> u64,
        rows: &mut FxHashMap<Bytes, AccumulatedRow>,
    ) -> usize {
        let mut evicted = 0usize;
        while !self.by_bucket.is_empty() && seal_fn(self.front_bucket) <= persisted {
            let slot = self.by_bucket.pop_front().unwrap();
            self.front_bucket += 1;
            for key in slot {
                if let Some(row) = rows.get(&key) {
                    debug_assert!(
                        row.oldest_unwritten_cp.is_none(),
                        "drain_up_to encountered dirty row key={:?} in bucket {}; \
                         violates watermark-gating invariant (a dirty row holds an \
                         entry in inflight_by_oldest_cp, which must block watermark \
                         promotion past this bucket's seal)",
                        key,
                        row.bucket_id,
                    );
                    if row.oldest_unwritten_cp.is_none() {
                        rows.remove(&key);
                        evicted += 1;
                    }
                }
            }
        }
        evicted
    }
}

/// Per-shard accumulated state. Owned exclusively by a single shard task;
/// no interior locking.
pub(super) struct ShardState {
    pub shard_id: ShardId,
    pub table: &'static str,
    pub seal_fn: fn(u64) -> u64,
    pub flush_only_when_sealed: bool,

    /// This shard's slice of the accumulated row map.
    pub rows: FxHashMap<Bytes, AccumulatedRow>,
    /// Row keys currently dirty (bitmap holds bits not yet durably
    /// written) in this shard.
    pub dirty: FxHashSet<Bytes>,
    /// Bucket-keyed index of rows awaiting action:
    /// - dirty rows (supports min-seal queries via `front_seal_tx_hi`)
    /// - in backfill mode, clean rows whose bucket hasn't yet been
    ///   covered by the persisted watermark (cleaned up on the next
    ///   SweepEviction).
    ///
    /// Invariant: every key in `dirty` is present in `eviction`, but
    /// not every key in `eviction` is in `dirty` — backfill-mode
    /// clean-pending-eviction rows are tracked only in `eviction`.
    pub eviction: EvictionIndex,

    /// Shared slice of all shards' min-seal mirrors; shard writes only
    /// its own slot (`min_seal_mirrors[shard_id]`). The handler reads
    /// the min across all slots to gate backfill-mode fast paths.
    pub min_seal_mirrors: Arc<[AtomicU64]>,
    /// Monotonic mirror of the coord's last successfully-persisted
    /// watermark `tx_hi`. Shared across all shards; written by the coord.
    pub latest_persisted_tx_hi: Arc<AtomicU64>,
    /// Test-only shared slice of each shard's `rows.len()` mirror.
    #[cfg(test)]
    pub rows_len_mirrors: Arc<[AtomicUsize]>,
}

impl ShardState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        shard_id: ShardId,
        table: &'static str,
        seal_fn: fn(u64) -> u64,
        flush_only_when_sealed: bool,
        min_seal_mirrors: Arc<[AtomicU64]>,
        latest_persisted_tx_hi: Arc<AtomicU64>,
        #[cfg(test)] rows_len_mirrors: Arc<[AtomicUsize]>,
    ) -> Self {
        Self {
            shard_id,
            table,
            seal_fn,
            flush_only_when_sealed,
            rows: FxHashMap::default(),
            dirty: FxHashSet::default(),
            eviction: EvictionIndex::new(),
            min_seal_mirrors,
            latest_persisted_tx_hi,
            #[cfg(test)]
            rows_len_mirrors,
        }
    }

    #[inline]
    fn update_min_seal_mirror(&self) {
        self.min_seal_mirrors[self.shard_id as usize].store(
            self.eviction
                .front_seal_tx_hi(self.seal_fn)
                .unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    #[cfg(test)]
    #[inline]
    fn update_rows_len_mirror(&self) {
        self.rows_len_mirrors[self.shard_id as usize].store(self.rows.len(), Ordering::Relaxed);
    }

    #[cfg(not(test))]
    #[inline]
    fn update_rows_len_mirror(&self) {}

    /// Merge this shard's values for one commit and emit eligible rows.
    pub(super) async fn merge_shard(
        &mut self,
        values: &[&BitmapIndexValue],
        tx_hi: u64,
        commit_cp: u64,
        pre_merge_any_sealed: bool,
        rows_tx: &mpsc::Sender<WriteRow>,
        emitted_oldest_cps: &mut Vec<u64>,
    ) {
        let seal_fn = self.seal_fn;
        let flush_only_when_sealed = self.flush_only_when_sealed;

        for v in values {
            let key = &v.row_key;
            let (bucket_id, grew) = match self.rows.get_mut(key) {
                Some(existing) => (existing.bucket_id, existing.or_in(v, commit_cp)),
                None => {
                    let mut new_row = AccumulatedRow::new(v, commit_cp);
                    let grew = !v.bitmap.is_empty();
                    new_row.oldest_unwritten_cp = if grew { Some(commit_cp) } else { None };
                    new_row.version = if grew { 1 } else { 0 };
                    new_row.emit_pending = false;
                    new_row.next_oldest_unwritten_cp = None;
                    self.rows.insert(key.clone(), new_row);
                    (v.bucket_id, grew)
                }
            };

            if grew && self.dirty.insert(key.clone()) {
                self.eviction.insert(bucket_id, key.clone());
            }

            let Some(row) = self.rows.get_mut(key) else {
                continue;
            };
            if row.emit_pending {
                continue;
            }
            let is_eligible = if flush_only_when_sealed {
                seal_fn(row.bucket_id) <= tx_hi && row.oldest_unwritten_cp.is_some()
            } else {
                row.oldest_unwritten_cp.is_some()
            };
            if !is_eligible {
                continue;
            }

            let oldest_cp = row.oldest_unwritten_cp.expect("eligibility implies Some");
            let sealed = seal_fn(row.bucket_id) <= tx_hi;
            let wr = row.emit_into(key.clone(), oldest_cp, sealed);
            emitted_oldest_cps.push(oldest_cp);
            if rows_tx.send(wr).await.is_err() {
                warn!(table = self.table, "Write loop closed during shard emit");
                break;
            }
        }

        // Backfill-mode post-value sweep over the dirty set: emit rows
        // whose bucket NOW seals (tx_hi crossed `seal_fn(bucket_id)`) but
        // which did NOT receive an incoming value this commit — the
        // per-value loop above only touches incoming keys.
        //
        // `pre_merge_any_sealed` short-circuits the sweep on commits
        // where no prior-pending bucket is sealed by the current tx_hi.
        if flush_only_when_sealed && pre_merge_any_sealed {
            let sweep_keys: Vec<Bytes> = self.dirty.iter().cloned().collect();
            for key in sweep_keys {
                let Some(row) = self.rows.get_mut(&key) else {
                    continue;
                };
                if row.emit_pending {
                    continue;
                }
                let Some(oldest_cp) = row.oldest_unwritten_cp else {
                    continue;
                };
                if seal_fn(row.bucket_id) > tx_hi {
                    continue;
                }
                // Guarded by `seal_fn(row.bucket_id) <= tx_hi` above.
                let wr = row.emit_into(key, oldest_cp, true);
                emitted_oldest_cps.push(oldest_cp);
                if rows_tx.send(wr).await.is_err() {
                    warn!(
                        table = self.table,
                        "Write loop closed during dirty-sweep emit"
                    );
                    break;
                }
            }
        }

        self.update_min_seal_mirror();
        self.update_rows_len_mirror();
    }

    /// Re-emit rows whose earlier WriteRow failed. The row's
    /// `oldest_unwritten_cp` is preserved across retries; only the bitmap
    /// snapshot and `emit_version` change. `inflight_by_oldest_cp`
    /// accounting is unchanged (the slot was taken at the original emit
    /// and is still held).
    pub(super) async fn handle_remerge(
        &mut self,
        row_keys: &[Bytes],
        rows_tx: &mpsc::Sender<WriteRow>,
    ) {
        let mut retried = 0usize;
        for key in row_keys {
            let Some(row) = self.rows.get_mut(key) else {
                warn!(table = self.table, "Remerge: row not in state; skipping");
                continue;
            };
            let Some(oldest_cp) = row.oldest_unwritten_cp else {
                warn!(
                    table = self.table,
                    "Remerge: row has no oldest_unwritten_cp; skipping"
                );
                continue;
            };
            // Re-emit after a failed write: use the latest persisted tx_hi
            // as the sealed-ness reference. Once sealed, always sealed;
            // using the persisted snapshot avoids stale false-negatives.
            let sealed = (self.seal_fn)(row.bucket_id)
                <= self.latest_persisted_tx_hi.load(Ordering::Relaxed);
            let write_row = row.emit_into(key.clone(), oldest_cp, sealed);
            if rows_tx.send(write_row).await.is_err() {
                warn!(table = self.table, "Write loop closed during Remerge send");
                return;
            }
            retried += 1;
        }
        debug!(
            table = self.table,
            retried, "handle_remerge re-emitted rows"
        );
    }

    /// Update row bookkeeping after successful BigTable writes. For each
    /// durable row:
    /// - Clear `emit_pending`.
    /// - If `next_oldest_unwritten_cp.is_some()`, inline re-emit and post
    ///   `CoordMsg::InflightMoved`.
    /// - Else if no new bits arrived, mark row clean and post
    ///   `CoordMsg::InflightCleared`. Evict if sealed (backfill mode);
    ///   else leave in the eviction index so a future sweep can reclaim
    ///   it once the watermark covers the bucket.
    pub(super) async fn handle_durable(
        &mut self,
        durable_rows: Vec<DurableRow>,
        rows_tx: &mpsc::Sender<WriteRow>,
        coord_tx: &mpsc::Sender<CoordMsg>,
    ) {
        // Opportunistic sweep: if the coord promoted a watermark since our
        // last invocation, evict any rows whose bucket is now covered.
        let persisted_snapshot = self.latest_persisted_tx_hi.load(Ordering::Relaxed);
        self.eviction
            .drain_up_to(persisted_snapshot, self.seal_fn, &mut self.rows);

        let table = self.table;
        let seal_fn = self.seal_fn;
        let flush_only_when_sealed = self.flush_only_when_sealed;
        let mut moved = 0usize;
        let mut cleared = 0usize;
        let mut evicted = 0usize;

        for d in durable_rows {
            let Some(row) = self.rows.get_mut(&d.row_key) else {
                warn!(table, "handle_durable: row missing from state");
                continue;
            };

            match row.oldest_unwritten_cp {
                Some(current) if current != d.oldest_unwritten_cp => {
                    debug!(
                        table,
                        durable_cp = d.oldest_unwritten_cp,
                        current_cp = current,
                        "handle_durable: row advanced past emitted cp; skipping"
                    );
                    let _ = coord_tx
                        .send(CoordMsg::InflightCleared {
                            old_cp: d.oldest_unwritten_cp,
                        })
                        .await;
                    continue;
                }
                None => {
                    debug!(
                        table,
                        "handle_durable: row already clean; decrementing inflight"
                    );
                    let _ = coord_tx
                        .send(CoordMsg::InflightCleared {
                            old_cp: d.oldest_unwritten_cp,
                        })
                        .await;
                    continue;
                }
                _ => {}
            }

            row.emit_pending = false;

            if let Some(new_cp) = row.next_oldest_unwritten_cp.take() {
                let old_cp = d.oldest_unwritten_cp;
                row.oldest_unwritten_cp = Some(new_cp);
                let sealed = seal_fn(row.bucket_id) <= persisted_snapshot;
                let wr = row.emit_into(d.row_key.clone(), new_cp, sealed);
                if rows_tx.send(wr).await.is_err() {
                    warn!(table, "Write loop closed during inline re-emit");
                    return;
                }
                if coord_tx
                    .send(CoordMsg::InflightMoved { old_cp, new_cp })
                    .await
                    .is_err()
                {
                    warn!(table, "Coord closed during InflightMoved send");
                    return;
                }
                moved += 1;
            } else if row.version == d.emit_version {
                let bucket_id = row.bucket_id;
                row.oldest_unwritten_cp = None;
                self.dirty.remove(&d.row_key);
                if coord_tx
                    .send(CoordMsg::InflightCleared {
                        old_cp: d.oldest_unwritten_cp,
                    })
                    .await
                    .is_err()
                {
                    warn!(table, "Coord closed during InflightCleared send");
                    return;
                }
                cleared += 1;
                if flush_only_when_sealed {
                    let persisted = self.latest_persisted_tx_hi.load(Ordering::Relaxed);
                    if seal_fn(bucket_id) <= persisted {
                        self.eviction.remove(bucket_id, &d.row_key);
                        self.rows.remove(&d.row_key);
                        evicted += 1;
                    }
                    // else: row stays in `rows` and remains in
                    // `self.eviction` (now as a clean-pending-eviction
                    // entry). A future `drain_up_to` will evict it.
                } else {
                    // Non-backfill mode: row stays resident indefinitely
                    // (for future OR accumulation). Still remove it from
                    // the eviction index so it no longer pins min-seal.
                    self.eviction.remove(bucket_id, &d.row_key);
                }
            } else {
                let old_cp = d.oldest_unwritten_cp;
                let sealed = seal_fn(row.bucket_id) <= persisted_snapshot;
                let wr = row.emit_into(d.row_key.clone(), old_cp, sealed);
                if rows_tx.send(wr).await.is_err() {
                    warn!(table, "Write loop closed during version-bump re-emit");
                    return;
                }
            }
        }

        self.update_min_seal_mirror();
        self.update_rows_len_mirror();

        if evicted > 0 {
            debug!(table, evicted, "Evicted sealed+clean rows after durable");
        }
        debug!(table, moved, cleared, evicted, "handle_durable done");
    }

    /// Handle `ShardFeedbackMsg::SweepEviction`. Idempotent: drains any
    /// buckets whose seal is now covered by `latest_persisted_tx_hi`.
    pub(super) fn handle_sweep_eviction(&mut self) {
        let persisted = self.latest_persisted_tx_hi.load(Ordering::Relaxed);
        let evicted = self
            .eviction
            .drain_up_to(persisted, self.seal_fn, &mut self.rows);
        if evicted > 0 {
            debug!(
                table = self.table,
                evicted,
                persisted_tx_hi = persisted,
                "Evicted sealed+clean rows in sweep"
            );
        }
        self.update_min_seal_mirror();
        self.update_rows_len_mirror();

        #[cfg(debug_assertions)]
        self.assert_post_sweep_consistency(persisted);
    }

    pub(super) fn current_min_dirty_seal_tx_hi(&self) -> Option<u64> {
        self.eviction.front_seal_tx_hi(self.seal_fn)
    }

    /// After a sweep, every remaining eviction-slot key must either be
    /// dirty or sit in a bucket whose seal exceeds `persisted` (so the
    /// drain rightly left it alone). Catches bugs in the clean-rows-
    /// pending-eviction machinery that behavioral tests might miss.
    #[cfg(debug_assertions)]
    fn assert_post_sweep_consistency(&self, persisted: u64) {
        self.eviction.assert_post_sweep_consistency(
            self.seal_fn,
            persisted,
            &self.rows,
            self.table,
        );
    }
}

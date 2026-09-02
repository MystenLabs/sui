// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Background pruner for the historical column families.
//!
//! Pruning is a standalone [`Service`] rather than a framework
//! pipeline: it does not consume checkpoints from ingestion, it reads
//! the already-committed state and deletes data below a retention
//! floor. The shape mirrors the validator's perpetual-store pruner
//! (a periodic background task) more than the indexer framework's
//! per-pipeline `prune` hook — the deletions are data-driven (we walk
//! transaction effects to retract superseded object versions) and the
//! floor is a single value shared across every historical CF.
//!
//! # What gets pruned
//!
//! - **Per-transaction CFs** (`transactions`, `effects`, `events`,
//!   `tx_metadata_by_seq`) — range-deleted over the pruned `tx_seq`
//!   range; the keys are contiguous big-endian `tx_seq`, so one range
//!   tombstone per CF clears the chunk.
//! - **Per-checkpoint CFs** (`checkpoint_summary`,
//!   `checkpoint_contents`) — range-deleted over the pruned
//!   checkpoint range.
//! - **Digest reverse indexes** (`tx_seq_by_digest`,
//!   `checkpoint_seq_by_digest`) — point-deleted; their keys are
//!   digests, so we collect them from the data being pruned (tx
//!   digests from each effects row, checkpoint digests from each
//!   summary) before deleting.
//! - **`objects` history** — point-deleted, effects-driven: each
//!   pruned transaction's `modified_at_versions` (superseded input
//!   versions) and `all_tombstones` (deleted / wrapped markers) are
//!   the exact `(ObjectID, version)` rows that are now dead. The
//!   latest live version is never an input to a pruned transaction,
//!   so it — and the greatest `object_version_by_checkpoint` entry
//!   that resolves to it — is preserved.
//! - **`object_version_by_checkpoint`** — retracted in lockstep with
//!   `objects` history: the same effects-driven walk records per-object
//!   retractions, then each prune batch coalesces them per object to the
//!   latest superseding checkpoint and point-deletes that object's
//!   checkpoint-pinned entries below it (plus the tombstone entry itself
//!   when the object was removed there). A best-effort per-object cursor
//!   starts each scan at the object's previous retraction checkpoint; on
//!   cache miss, eviction, or restart the scan falls back to checkpoint 0,
//!   slower but deleting the same rows. Retractions use point deletes
//!   because a fallback range delete would restart at checkpoint 0:
//!   repeated same-start ranges nest, and RocksDB fragments `K` nested
//!   range tombstones into `O(K^2)` pieces — which OOMed mainnet
//!   fullnodes (see `retract_object_version_by_checkpoint`).
//!   The retained set mirrors the `objects` versions kept, so the index
//!   never points at a pruned version.
//! - **Ledger-history bitmaps** (`transaction_bitmap`,
//!   `event_bitmap`) — not deleted directly; advancing the
//!   database-local pruning floor lets their compaction filters drop
//!   fully-pruned buckets. Merge operands can require one covering
//!   compaction to materialize and a later compaction to filter; the
//!   forced catch-up pass and periodic compaction provide those sweeps.
//!
//! The live-set-bounded indexes (`object_by_owner`, `object_by_type`,
//! `balance`, `package_versions`) and the tiny `epochs` CF are never
//! pruned.
//!
//! # Floor, retention, and safety
//!
//! Retention is epoch-based: the `retention_epochs` most-recent
//! epochs are retained, and the target floor is the start checkpoint
//! of the oldest retained epoch. The floor is then clamped so it
//! never advances past the oldest in-memory snapshot's checkpoint:
//! point and range deletes are already invisible to a snapshot
//! (RocksDB pins the data a live snapshot references), but the bitmap
//! compaction filter physically removes buckets irrespective of
//! snapshots, so the clamp keeps every live snapshot's advertised
//! available range valid even under an aggressively small retention.
//!
//! Each tick advances the floor toward that target by at most
//! `max_checkpoints_per_tick` checkpoints (in `max_chunk_checkpoints`
//! atomic chunks), so a large backlog — for example when pruning is
//! first enabled on an old database — drains across many ticks rather
//! than one long blocking pass. The floor converges to the target
//! over subsequent ticks.
//!
//! # Ordering and crash-safety
//!
//! Each chunk stages all of its deletes plus the new
//! `PruningWatermarks` row into one atomic batch, commits, and only
//! then advances the in-memory bitmap floor. Because the watermark
//! row lives in the same batch as the deletes, a crash either loses
//! the whole chunk (re-pruned next run) or commits it wholesale;
//! there is no partial-delete-without-watermark state. Range and
//! point deletes are idempotent, so a re-run is harmless.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::ops::Bound;
use std::sync::Arc;
use std::sync::Mutex;

use lru::LruCache;

use anyhow::Context as _;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;
use sui_consistent_store::Batch;
use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_indexer_alt_framework::service::Service;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::message_envelope::Message;
use tokio::time::MissedTickBehavior;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::RpcStoreSchema;
use crate::config::PrunerConfig;
use crate::indexer::Store;
use crate::indexer::restore::HISTORY_COHORT;
use crate::indexer::restore::LIVE_COHORT;
use crate::schema::checkpoint_seq_by_digest;
use crate::schema::event_bitmap;
use crate::schema::object_version_by_checkpoint;
use crate::schema::objects;
use crate::schema::primitives::U64Be;
use crate::schema::pruning_watermark;
use crate::schema::pruning_watermark::Watermarks;
use crate::schema::transaction_bitmap;
use crate::schema::tx_seq_by_digest;

/// Prometheus metrics for the pruner.
pub struct PrunerMetrics {
    /// Lowest still-available checkpoint sequence number — the
    /// persisted checkpoint floor.
    pub checkpoint_lo: IntGauge,
    /// Lowest still-available transaction sequence number — the
    /// persisted `tx_seq` floor.
    pub tx_seq_lo: IntGauge,
    /// Total pruning chunks committed.
    pub chunks_committed: IntCounter,
    /// Total superseded object versions and tombstones deleted.
    pub objects_deleted: IntCounter,
}

impl PrunerMetrics {
    pub fn new(prefix: Option<&str>, registry: &Registry) -> Arc<Self> {
        let prefix = prefix.unwrap_or("rpc_store_pruner");
        let name = |n| format!("{prefix}_{n}");

        Arc::new(Self {
            checkpoint_lo: register_int_gauge_with_registry!(
                name("checkpoint_lo"),
                "Lowest still-available checkpoint sequence number (pruning floor)",
                registry,
            )
            .unwrap(),
            tx_seq_lo: register_int_gauge_with_registry!(
                name("tx_seq_lo"),
                "Lowest still-available transaction sequence number (pruning floor)",
                registry,
            )
            .unwrap(),
            chunks_committed: register_int_counter_with_registry!(
                name("chunks_committed"),
                "Total pruning chunks committed",
                registry,
            )
            .unwrap(),
            objects_deleted: register_int_counter_with_registry!(
                name("objects_deleted"),
                "Total superseded object versions and tombstones deleted by the pruner",
                registry,
            )
            .unwrap(),
        })
    }
}

/// Default maximum number of per-object retraction cursors retained in memory.
pub const DEFAULT_RETRACTION_CURSORS_CAPACITY: usize = 200_000;

/// In-memory cache of the greatest committed retraction checkpoint per object.
///
/// Bounded by an LRU policy with capacity [`DEFAULT_RETRACTION_CURSORS_CAPACITY`].
/// A missing or evicted object falls back to a lower bound of `0`.
#[derive(Debug)]
pub struct RetractionCursors {
    cache: LruCache<ObjectID, u64>,
}

impl Default for RetractionCursors {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_RETRACTION_CURSORS_CAPACITY)
    }
}

impl RetractionCursors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1)).expect("capacity is at least 1");
        Self {
            cache: LruCache::new(capacity),
        }
    }

    /// Return the lower bound checkpoint for scanning an object's prefix.
    /// Returns 0 on miss or eviction.
    pub fn lower_bound(&self, id: &ObjectID) -> u64 {
        self.cache.peek(id).copied().unwrap_or(0)
    }

    /// Monotonically advance the retraction checkpoint for the given object.
    pub fn advance(&mut self, id: ObjectID, cp: u64) {
        if let Some(existing) = self.cache.get_mut(&id) {
            *existing = (*existing).max(cp);
        } else {
            self.cache.put(id, cp);
        }
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn cap(&self) -> usize {
        self.cache.cap().get()
    }
}

/// Collects `object_version_by_checkpoint` retractions for one prune batch,
/// coalescing every retraction for an object to a single entry.
///
/// Hot objects such as Clock and SuiSystemState can be superseded in every
/// checkpoint, so one prune batch retracts the same object many times. The
/// retraction point-deletes each checkpoint-pinned row below the superseding
/// checkpoint by walking the object's prefix once (see
/// [`retract_object_version_by_checkpoint`]); coalescing keeps a hot object's
/// prefix from being walked once per supersession.
///
/// The retraction uses point deletes rather than one
/// `delete_range [id||0, id||cp)` because cursor state is best-effort in
/// memory: on cache miss, eviction, or node restart, the scan falls back to
/// `lo_cp = 0`. If range deletes were used, successive retractions falling
/// back to `id||0` would share that lower bound, causing range tombstones to
/// nest and RocksDB's `FragmentedRangeTombstoneList` to fragment `K` of them
/// into `K^2 / 2` `(fragment, seqnum)` pairs -- which OOMed mainnet fullnodes
/// during memtable flush and WAL recovery. Point deletes are the only
/// fallback-safe delete shape.
/// Coalescing keeps only the greatest checkpoint per object: its rows below that
/// checkpoint are the union of every narrower retraction's rows, so one widest
/// retraction subsumes them all. On an equal checkpoint, the `removed` flags are
/// ORed: if any same-checkpoint retraction removed the object, the row at that
/// checkpoint must be dropped.
#[derive(Default)]
struct Retractions(HashMap<ObjectID, (u64, bool)>);

impl Retractions {
    fn record(&mut self, id: ObjectID, cp: u64, removed: bool) {
        self.0
            .entry(id)
            .and_modify(|(recorded_cp, recorded_removed)| {
                if cp > *recorded_cp {
                    *recorded_cp = cp;
                    *recorded_removed = removed;
                } else if cp == *recorded_cp {
                    *recorded_removed |= removed;
                }
            })
            .or_insert((cp, removed));
    }

    fn stage(
        &self,
        batch: &mut Batch,
        schema: &RpcStoreSchema,
        cursors: &RetractionCursors,
    ) -> anyhow::Result<()> {
        for (&id, &(cp, removed)) in &self.0 {
            let lo_cp = cursors.lower_bound(&id);
            retract_object_version_by_checkpoint(batch, schema, id, lo_cp, cp, removed)?;
        }
        Ok(())
    }

    fn commit_to_cursors(self, cursors: &mut RetractionCursors) {
        for (id, (cp, _removed)) in self.0 {
            cursors.advance(id, cp);
        }
    }
}

/// Start the background pruner as a [`Service`].
///
/// Errors if `config.retention_epochs` is `0` (which would prune the
/// current epoch). The returned service runs an infinite tick loop;
/// it is aborted on graceful shutdown (each chunk is atomic, so an
/// abort leaves the database consistent).
pub fn start_pruner(
    store: Store,
    config: PrunerConfig,
    metrics: Arc<PrunerMetrics>,
) -> anyhow::Result<Service> {
    anyhow::ensure!(
        config.retention_epochs >= 1,
        "PrunerConfig::retention_epochs must be >= 1; 0 would prune the current epoch",
    );
    anyhow::ensure!(
        config.max_checkpoints_per_tick >= 1,
        "PrunerConfig::max_checkpoints_per_tick must be >= 1; 0 would never make progress",
    );

    let cursors = Arc::new(Mutex::new(RetractionCursors::default()));

    let service = Service::new().spawn_aborting(async move {
        let mut ticker = tokio::time::interval(config.interval());
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            let store = store.clone();
            let config = config.clone();
            let metrics = metrics.clone();
            let cursors = cursors.clone();

            // The pruner does blocking RocksDB iteration and writes;
            // keep it off the async runtime threads.
            let res = tokio::task::spawn_blocking(move || {
                let mut guard = cursors.lock().unwrap_or_else(|p| p.into_inner());
                prune_once(store.db(), store.schema(), &mut guard, &config, &metrics)
            })
            .await;

            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!("rpc-store pruner pass failed (will retry next interval): {e:#}")
                }
                Err(e) => {
                    if e.is_panic() {
                        std::panic::resume_unwind(e.into_panic());
                    } else {
                        panic!("rpc-store pruner task failed: {e}");
                    }
                }
            }
        }
    });

    Ok(service)
}

/// Run a single pruning pass: recompute the target floor and advance
/// the persisted floor toward it one chunk at a time.
fn prune_once(
    db: &Db,
    schema: &RpcStoreSchema,
    retraction_cursors: &mut RetractionCursors,
    config: &PrunerConfig,
    metrics: &PrunerMetrics,
) -> anyhow::Result<()> {
    let Some(current_epoch) = current_committed_epoch(db)? else {
        debug!("rpc-store pruner: no committed watermark yet; nothing to prune");
        return Ok(());
    };

    let Some(retention_lo) =
        retention_checkpoint_floor(schema, current_epoch, config.retention_epochs)?
    else {
        debug!(
            current_epoch,
            "rpc-store pruner: retention floor not yet reached; nothing to prune"
        );
        return Ok(());
    };

    // Never advance the floor past the oldest live snapshot.
    let target_lo = clamp_to_snapshot(retention_lo, db.snapshot_range().map(|r| *r.start()));

    let mut cursor = schema.get_pruning_watermarks()?.unwrap_or_default();
    if target_lo <= cursor.checkpoint_lo {
        debug!(
            target_lo,
            current_lo = cursor.checkpoint_lo,
            "rpc-store pruner: floor already at or beyond target"
        );
        return Ok(());
    }

    // Bound the work done this tick: advance the floor by at most
    // `max_checkpoints_per_tick` checkpoints so a large backlog drains
    // across many ticks instead of one long blocking pass. The floor
    // converges to `target_lo` over subsequent ticks.
    let tick_target = target_lo.min(cursor.checkpoint_lo + config.max_checkpoints_per_tick);

    info!(
        from = cursor.checkpoint_lo,
        to = tick_target,
        target = target_lo,
        current_epoch,
        "rpc-store pruner: advancing floor"
    );

    while cursor.checkpoint_lo < tick_target {
        let chunk_ckpt_hi = (cursor.checkpoint_lo + config.max_chunk_checkpoints).min(tick_target);
        cursor = prune_chunk(
            db,
            schema,
            retraction_cursors,
            cursor,
            chunk_ckpt_hi,
            metrics,
        )?;
        metrics.checkpoint_lo.set(cursor.checkpoint_lo as i64);
        metrics.tx_seq_lo.set(cursor.tx_seq_lo as i64);
        metrics.chunks_committed.inc();
    }

    // A bitmap row written as a merge operand may need one covering
    // compaction to materialize and another to be filtered. Force a
    // pass after reaching the retention target; the bitmap CFs'
    // periodic compaction policy supplies subsequent passes. While a
    // backlog is draining, skip whole-CF compaction so it does not
    // become the per-tick long pole.
    if cursor.checkpoint_lo >= target_lo {
        db.compact_range_cf(transaction_bitmap::NAME, None, None)
            .context("Compacting transaction_bitmap after prune")?;
        db.compact_range_cf(event_bitmap::NAME, None, None)
            .context("Compacting event_bitmap after prune")?;
    }

    Ok(())
}

/// Prune one chunk of checkpoints `[cursor.checkpoint_lo,
/// chunk_ckpt_hi)` and their transactions, returning the new floor.
fn prune_chunk(
    db: &Db,
    schema: &RpcStoreSchema,
    retraction_cursors: &mut RetractionCursors,
    cursor: Watermarks,
    chunk_ckpt_hi: u64,
    metrics: &PrunerMetrics,
) -> anyhow::Result<Watermarks> {
    let ckpt_lo = cursor.checkpoint_lo;
    let tx_lo = cursor.tx_seq_lo;

    // The exclusive `tx_seq` upper bound for the chunk is the
    // cumulative network tx count after the chunk's highest
    // checkpoint, which is the first `tx_seq` of `chunk_ckpt_hi`.
    // `chunk_ckpt_hi >= 1` by the caller's loop invariant, and
    // `chunk_ckpt_hi - 1 >= ckpt_lo` is still retained (not yet
    // pruned), so its summary is present.
    let last_ckpt = chunk_ckpt_hi - 1;
    let tx_hi = schema
        .get_checkpoint_summary(last_ckpt)?
        .with_context(|| format!("checkpoint_summary missing for checkpoint {last_ckpt}"))?
        .data()
        .network_total_transactions;

    let mut batch = db.batch();
    let mut retractions = Retractions::default();
    let mut objects_deleted: u64 = 0;
    // Walk each pruned checkpoint and the transactions it contains.
    // Consecutive summaries' `network_total_transactions` partition
    // `[tx_lo, tx_hi)` into per-checkpoint tx ranges, so the containing
    // checkpoint of every transaction is known here -- it is exactly
    // the `seq` being walked -- without a per-transaction metadata
    // lookup. Each effects row yields the object versions to retract
    // and the transaction digest to unindex; a missing effects row
    // means that transaction was already pruned (idempotent re-run).
    let mut tx_cursor = tx_lo;
    for seq in ckpt_lo..chunk_ckpt_hi {
        // Every in-range summary is still present: the chunk has not
        // deleted any yet, and prior chunks committed atomically. A
        // miss is therefore corruption, not an expected re-run state,
        // so fail loudly rather than mis-partition the tx range.
        let summary = schema
            .get_checkpoint_summary(seq)?
            .with_context(|| format!("checkpoint_summary missing for checkpoint {seq}"))?;
        let ckpt_tx_hi = summary.data().network_total_transactions;

        for tx_seq in tx_cursor..ckpt_tx_hi {
            let Some((effects, _unchanged)) = schema.get_effects(tx_seq)? else {
                continue;
            };
            for (id, version) in effects.modified_at_versions() {
                batch.delete(&schema.objects, &objects::Key { id, version })?;
                // Record checkpoint-pinned entries older than this
                // supersession for per-batch retraction; the entry at
                // `seq` (the object's final version in this checkpoint)
                // is kept.
                retractions.record(id, seq, false);
                objects_deleted += 1;
            }
            for (id, version) in effects.all_tombstones() {
                batch.delete(&schema.objects, &objects::Key { id, version })?;
                // The object was removed in `seq`: record that its
                // tombstone entry at `seq` must be dropped too.
                retractions.record(id, seq, true);
                objects_deleted += 1;
            }
            batch.delete(
                &schema.tx_seq_by_digest,
                &tx_seq_by_digest::Key(*effects.transaction_digest()),
            )?;
        }
        tx_cursor = ckpt_tx_hi;

        // Unindex this checkpoint's digest reverse map.
        batch.delete(
            &schema.checkpoint_seq_by_digest,
            &checkpoint_seq_by_digest::Key(summary.data().digest()),
        )?;
    }

    // The `tx_seq`- and checkpoint-keyed CFs are contiguous, so one
    // range delete each clears the whole chunk regardless of how many
    // rows it spans.
    batch.delete_range(&schema.transactions, &U64Be(tx_lo), &U64Be(tx_hi))?;
    batch.delete_range(&schema.effects, &U64Be(tx_lo), &U64Be(tx_hi))?;
    batch.delete_range(&schema.events, &U64Be(tx_lo), &U64Be(tx_hi))?;
    batch.delete_range(&schema.tx_metadata_by_seq, &U64Be(tx_lo), &U64Be(tx_hi))?;
    batch.delete_range(
        &schema.checkpoint_summary,
        &U64Be(ckpt_lo),
        &U64Be(chunk_ckpt_hi),
    )?;
    batch.delete_range(
        &schema.checkpoint_contents,
        &U64Be(ckpt_lo),
        &U64Be(chunk_ckpt_hi),
    )?;

    retractions.stage(&mut batch, schema, retraction_cursors)?;

    // Advance the persisted floor atomically with the deletes.
    let new = Watermarks {
        tx_seq_lo: tx_hi,
        checkpoint_lo: chunk_ckpt_hi,
    };
    let (k, v) = pruning_watermark::store(&new);
    batch.put(&schema.pruning_watermark, &k, &v)?;

    batch.commit()?;

    retractions.commit_to_cursors(retraction_cursors);

    // The commit is durable; advance the in-memory bitmap floor so
    // the compaction filters drop buckets below `tx_hi`.
    schema.set_pruning_floor(new.tx_seq_lo);
    metrics.objects_deleted.inc_by(objects_deleted);

    Ok(new)
}

/// Retract `object_version_by_checkpoint` rows for one object, given the
/// greatest checkpoint in a prune batch that superseded or removed it, in
/// lockstep with the `objects` CF.
///
/// Point-deletes every checkpoint-pinned entry for `id` strictly older than
/// `cp` by walking the object's own prefix over `[id||lo_cp, id||cp)` and issuing a
/// targeted delete for each row present. When `lo_cp >= cp`, the scan range is
/// empty. The bounds stay within `id`'s prefix, so the scan never spills into the
/// neighboring object. Callers coalesce repeated supersessions for the same
/// object within a batch before calling this helper (see [`Retractions`]), so this
/// prefix is walked once, at the greatest `cp`, whose row set is the union of
/// every narrower retraction's -- including any removal below `cp`.
///
/// Invariant: after a retraction at checkpoint `lo_cp` commits, no live
/// `object_version_by_checkpoint` row for that object exists strictly below
/// `lo_cp` -- prior retractions visited and deleted every row below `lo_cp`,
/// leaving at most the kept anchor exactly at `lo_cp` (or no row if removed at
/// `lo_cp`). Therefore, a subsequent retraction at `cp > lo_cp` only needs to scan
/// `[id||lo_cp, id||cp)`. Seeking to `id||lo_cp` skips the dead range below `lo_cp`.
/// If `lo_cp` is 0 (cursor cache miss or eviction), the scan safely defaults to
/// `[id||0, id||cp)`.
///
/// Point deletes rather than `delete_range`: cursor state is best-effort in
/// memory, so on cache miss, eviction, or node restart, `lo_cp` falls back to
/// `0`. If range deletes were used, successive retractions falling back to
/// `id||0` would share that start and nest into `O(K^2)` `(fragment, seqnum)`
/// pairs at flush, compaction, and read. Point deletes are the only
/// fallback-safe delete shape: ordinary point tombstones carry no such
/// structure, and the cost is one entry per deleted row and a bounded prefix
/// scan on the delete path.
///
/// Once the floor advances past `cp`, the entry at `cp` (or a newer one) is the
/// floor a checkpoint-pinned read resolves to, so the older entries can never
/// be the answer again. Because the chunk only prunes checkpoints below the new
/// floor, `cp` is itself below the floor, so the kept entry is never the answer
/// to an in-range read either; it survives only until its own superseding
/// transaction is pruned in a later chunk.
///
/// The entry *at* `cp` is kept for a supersession (it is the object's final
/// live version in `cp`). When `removed` is set, the object was deleted or
/// wrapped in `cp`: its tombstone entry at `cp` is dropped too, since nothing
/// at or after the floor can reference a removed object.
fn retract_object_version_by_checkpoint(
    batch: &mut Batch,
    schema: &RpcStoreSchema,
    id: ObjectID,
    lo_cp: u64,
    cp: u64,
    removed: bool,
) -> anyhow::Result<()> {
    if lo_cp < cp {
        let lo = object_version_by_checkpoint::Key {
            id,
            checkpoint: lo_cp,
        };
        let hi = object_version_by_checkpoint::Key { id, checkpoint: cp };
        for entry in schema
            .object_version_by_checkpoint
            .iter((Bound::Included(lo), Bound::Excluded(hi)))?
        {
            let (key, _value) = entry?;
            batch.delete(&schema.object_version_by_checkpoint, &key)?;
        }
    }
    if removed {
        let hi = object_version_by_checkpoint::Key { id, checkpoint: cp };
        batch.delete(&schema.object_version_by_checkpoint, &hi)?;
    }
    Ok(())
}

/// Prune the embedded fullnode's history cohort up to a floor supplied
/// by the validator's perpetual-store pruner.
///
/// Unlike [`start_pruner`], this is not epoch-driven and not a
/// `Service`. The embedded deployment deactivates the raw chain-data
/// CFs (`transactions`, `effects`, `events`, `objects`,
/// `checkpoint_*`), so it cannot derive a retention floor or read the
/// raw effects itself. Instead the perpetual pruner — which owns the raw
/// data — supplies the floor and the pruned checkpoints' `effects`
/// directly, and this prunes exactly the history-cohort CFs that grow
/// without bound:
///
/// - `tx_metadata_by_seq` — range-deleted over
///   `[old_tx_lo, pruned_tx_seq_exclusive)`.
/// - `tx_seq_by_digest` — point-deleted; the digests are read from
///   `tx_metadata_by_seq` (the only history CF that still carries them)
///   over the pruned range, before that range is deleted.
/// - `object_version_by_checkpoint` — retracted effects-driven through the
///   same per-batch deduped retraction path as the standalone `prune_chunk`
///   (the paired `objects` delete lives in that caller, not the helper, and
///   the embedded store has no `objects` CF): each effect carries the
///   checkpoint it was pruned from, and repeated retractions for one object
///   are coalesced to the greatest checkpoint, so a superseded object keeps
///   only its supersession-checkpoint row — the anchor a point-in-time read at
///   the floor resolves to — and a removed object drops its rows (a later
///   wrap/unwrap re-creation at or above the floor survives).
/// - `transaction_bitmap` / `event_bitmap` — evicted by advancing the
///   database-local `tx_seq` floor so their compaction filters drop
///   fully-pruned buckets during periodic compaction.
///
/// The live cohort, `package_versions`, and the tiny `epochs` CF are
/// never pruned.
///
/// `pruned_checkpoint_watermark` is the highest checkpoint the
/// perpetual store has pruned (inclusive); `pruned_tx_seq_exclusive` is
/// the first still-retained `tx_seq`. The pruner consumes the same floor
/// the perpetual store prunes to, so the embedded rpc-store's history
/// cohort stays in lockstep with it. Idempotent: a re-run with the same
/// or a lower floor is a no-op.
///
/// Ordering contract: the caller must invoke this BEFORE durably
/// committing its own prune of the same checkpoints. The
/// `object_version_by_checkpoint` retraction is driven by the `effects`
/// passed in this call and is never re-derived; if the caller's floor
/// committed first, a crash between the two commits would skip these
/// effects forever and leak the rows they retract. Committing this side
/// first is safe precisely because a re-run is idempotent.
pub fn prune_history_cohort(
    db: &Db,
    schema: &RpcStoreSchema,
    retraction_cursors: &mut RetractionCursors,
    pruned_checkpoint_watermark: u64,
    pruned_tx_seq_exclusive: u64,
    effects: &[(u64, TransactionEffects)],
) -> anyhow::Result<()> {
    let cursor = schema.get_pruning_watermarks()?.unwrap_or_default();
    let tx_lo = cursor.tx_seq_lo;
    let tx_hi = pruned_tx_seq_exclusive;
    // Lowest still-available checkpoint after this prune: the perpetual
    // store has pruned through `pruned_checkpoint_watermark` inclusive.
    let checkpoint_lo = pruned_checkpoint_watermark.saturating_add(1);

    // No-op if the floor would not advance on either axis (idempotent
    // re-run, or the perpetual floor is behind ours).
    if tx_hi <= tx_lo && checkpoint_lo <= cursor.checkpoint_lo {
        return Ok(());
    }

    let mut batch = db.batch();
    let mut retractions = Retractions::default();
    // Unindex the digest reverse map for the pruned `tx_seq` range. The
    // digests live in `tx_metadata_by_seq`; iterate it (seeking to the
    // first present row) rather than point-getting each `tx_seq`, so a
    // sparse range or an unknown (zero) floor costs work proportional to
    // the rows present, not to the width of the interval.
    for entry in schema.iter_tx_seq_digests(tx_lo, tx_hi)? {
        let (_tx_seq, digest) = entry?;
        batch.delete(&schema.tx_seq_by_digest, &tx_seq_by_digest::Key(digest))?;
    }
    batch.delete_range(&schema.tx_metadata_by_seq, &U64Be(tx_lo), &U64Be(tx_hi))?;

    // Retract `object_version_by_checkpoint` for every object the pruned
    // checkpoints superseded or removed, reusing the same per-batch deduped
    // effects-driven path as the standalone `prune_chunk` (its paired
    // `objects` delete lives in that caller, not the helper, and the embedded
    // store has no `objects` CF). Each effect carries the checkpoint it was
    // pruned from, and repeated retractions for one object are coalesced to the
    // greatest checkpoint, so the retraction keeps each object's anchor at its
    // true latest supersession checkpoint and drops the older ones; a removed
    // object drops its tombstone too.
    for (checkpoint, effects) in effects {
        for (id, _version) in effects.modified_at_versions() {
            retractions.record(id, *checkpoint, false);
        }
        for (id, _version) in effects.all_tombstones() {
            retractions.record(id, *checkpoint, true);
        }
    }
    retractions.stage(&mut batch, schema, retraction_cursors)?;

    // Advance the persisted floor atomically with the deletes, taking
    // the monotonic max on each axis so a stale lower floor never
    // regresses an axis the other call already advanced.
    let new = Watermarks {
        tx_seq_lo: tx_hi.max(tx_lo),
        checkpoint_lo: checkpoint_lo.max(cursor.checkpoint_lo),
    };
    let (k, v) = pruning_watermark::store(&new);
    batch.put(&schema.pruning_watermark, &k, &v)?;
    batch.commit()?;

    retractions.commit_to_cursors(retraction_cursors);

    // Durable now: advance the in-memory bitmap floor so the bitmap
    // compaction filters start dropping fully-pruned buckets on the next
    // natural background compaction. The prune forces no sweep of its own:
    // compacting on every prune batch is far more compaction work than the
    // reclaimed space is worth.
    schema.set_pruning_floor(new.tx_seq_lo);

    Ok(())
}

/// The highest checkpoint the embedded fullnode's pruner may prune
/// through (inclusive) without deleting source data the embedded
/// indexer still needs: `min(checkpoint_hi_inclusive)` across every
/// embedded-cohort pipeline ([`LIVE_COHORT`] and [`HISTORY_COHORT`]).
///
/// Both cohorts assemble full checkpoints from the perpetual and
/// checkpoint stores through the local ingestion client — the history
/// cohort while backfilling `(L, T]`, the live cohort when filling
/// gaps behind the executor's broadcast stream — so a checkpoint's
/// data may only be deleted once every pipeline has committed it.
/// Pruning past a pipeline's watermark would leave that pipeline
/// permanently stalled on a checkpoint that can no longer be served
/// (`NotFound` is retried forever).
///
/// Returns `None` when any cohort pipeline has no watermark yet — a
/// from-genesis build before that pipeline's first commit — in which
/// case nothing may be pruned: the pipeline still needs the entire
/// available range.
///
/// [`LIVE_COHORT`]: crate::LIVE_COHORT
/// [`HISTORY_COHORT`]: crate::HISTORY_COHORT
pub fn embedded_prunable_checkpoint(db: &Db) -> anyhow::Result<Option<u64>> {
    let framework = db.framework();
    let mut min_hi: Option<u64> = None;
    for name in LIVE_COHORT.iter().chain(HISTORY_COHORT) {
        let key = PipelineTaskKey::new(*name);
        let Some(watermark) = framework
            .watermarks
            .get(&key)
            .with_context(|| format!("reading watermark for {name}"))?
        else {
            return Ok(None);
        };
        let hi = watermark.checkpoint_hi_inclusive;
        min_hi = Some(min_hi.map_or(hi, |m| m.min(hi)));
    }
    Ok(min_hi)
}

/// The lowest epoch fully committed across every registered pipeline,
/// or `None` if no pipeline has committed a watermark yet.
///
/// Taking the minimum is deliberately conservative: it lags the true
/// tip epoch by at most one epoch while a pipeline catches up across
/// a boundary, which only ever causes the pruner to retain slightly
/// more.
fn current_committed_epoch(db: &Db) -> anyhow::Result<Option<u64>> {
    let framework = FrameworkSchema::new(db.clone());
    let mut min_epoch: Option<u64> = None;
    for entry in framework.watermarks.iter(..)? {
        let (_, watermark) = entry?;
        let epoch = watermark.epoch_hi_inclusive;
        min_epoch = Some(min_epoch.map_or(epoch, |m| m.min(epoch)));
    }
    Ok(min_epoch)
}

/// The target checkpoint floor implied by epoch-based retention: the
/// start checkpoint of the oldest epoch that is still retained.
///
/// Returns `None` when nothing is eligible yet — either the chain is
/// younger than the retention window, or the oldest retained epoch's
/// row (or its `start_checkpoint`) has not been observed.
fn retention_checkpoint_floor(
    schema: &RpcStoreSchema,
    current_epoch: u64,
    retention_epochs: u64,
) -> anyhow::Result<Option<u64>> {
    debug_assert!(retention_epochs >= 1, "validated in start_pruner");

    // Retain epochs `[oldest_retained, current_epoch]`.
    let oldest_retained = current_epoch.saturating_sub(retention_epochs - 1);
    if oldest_retained == 0 {
        // Epoch 0 is still retained, so no epoch has fully aged out.
        return Ok(None);
    }

    let Some(info) = schema.get_epoch(oldest_retained)? else {
        return Ok(None);
    };
    Ok(info.start_checkpoint)
}

/// Clamp the retention-derived floor so it never advances past the
/// oldest in-memory snapshot's checkpoint. With no snapshots the
/// retention floor stands; otherwise the floor is held at or below
/// the oldest snapshot so that snapshot's advertised available range
/// stays valid (and the bitmap compaction filter, which ignores
/// snapshots, never drops a bucket the snapshot still serves).
fn clamp_to_snapshot(retention_lo: u64, oldest_snapshot: Option<u64>) -> u64 {
    match oldest_snapshot {
        Some(snap) => retention_lo.min(snap),
        None => retention_lo,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use prometheus::Registry;
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_consistent_store::PipelineTaskKey;
    use sui_consistent_store::Watermark;
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::schema::epochs;
    use crate::schema::primitives::U64Varint;

    fn fresh_db() -> (tempfile::TempDir, Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    /// Stamp `checkpoint_hi_inclusive = hi` watermarks for `names`.
    fn stamp_watermarks(db: &Db, names: &[&str], hi: u64) {
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        for name in names {
            batch
                .put(
                    &framework.watermarks,
                    &PipelineTaskKey::new(*name),
                    &Watermark::for_checkpoint(hi),
                )
                .unwrap();
        }
        batch.commit().unwrap();
    }

    /// `embedded_prunable_checkpoint` is the minimum watermark across
    /// both embedded cohorts, and `None` while any cohort pipeline has
    /// no watermark at all.
    #[test]
    fn embedded_prunable_checkpoint_is_min_across_cohorts() {
        let (_dir, db, _schema) = fresh_db();

        // Fresh database: nothing committed, nothing prunable.
        assert_eq!(embedded_prunable_checkpoint(&db).unwrap(), None);

        // Live cohort at the tip, history cohort still absent (e.g. a
        // from-genesis backfill before its first commit): still
        // nothing prunable.
        stamp_watermarks(&db, LIVE_COHORT, 1_000);
        assert_eq!(embedded_prunable_checkpoint(&db).unwrap(), None);

        // Every history pipeline committed through 40 except one
        // straggler at 25: the straggler bounds the prunable range.
        stamp_watermarks(&db, HISTORY_COHORT, 40);
        stamp_watermarks(&db, &[HISTORY_COHORT[0]], 25);
        assert_eq!(embedded_prunable_checkpoint(&db).unwrap(), Some(25));

        // The straggler catches up past the live cohort: the live
        // cohort's watermark now bounds the range.
        stamp_watermarks(&db, HISTORY_COHORT, 2_000);
        assert_eq!(embedded_prunable_checkpoint(&db).unwrap(), Some(1_000));
    }

    /// Populate the CFs the pruner reads and deletes by running the
    /// real pipelines' `process` over `checkpoint` and staging their
    /// rows — `objects`, `effects`, `checkpoint_summary`, and the two
    /// digest reverse indexes. These cover both deletion mechanisms
    /// (range delete and point delete) plus the effects-driven object
    /// retraction.
    async fn seed(
        db: &Db,
        schema: &RpcStoreSchema,
        checkpoint: &Arc<sui_types::full_checkpoint_content::Checkpoint>,
    ) {
        let mut batch = db.batch();
        for row in crate::indexer::objects::Objects
            .process(checkpoint)
            .await
            .unwrap()
        {
            batch
                .put(
                    &schema.objects,
                    &objects::Key {
                        id: row.id,
                        version: row.version,
                    },
                    &row.value,
                )
                .unwrap();
        }
        for row in crate::indexer::effects::Effects
            .process(checkpoint)
            .await
            .unwrap()
        {
            batch
                .put(&schema.effects, &U64Be(row.tx_seq), &row.value)
                .unwrap();
        }
        for row in crate::indexer::checkpoint_summary::CheckpointSummary
            .process(checkpoint)
            .await
            .unwrap()
        {
            batch
                .put(&schema.checkpoint_summary, &U64Be(row.seq), &row.value)
                .unwrap();
        }
        for row in crate::indexer::tx_seq_by_digest::TxSeqByDigest
            .process(checkpoint)
            .await
            .unwrap()
        {
            batch
                .put(
                    &schema.tx_seq_by_digest,
                    &tx_seq_by_digest::Key(row.digest),
                    &U64Varint(row.tx_seq),
                )
                .unwrap();
        }
        for row in crate::indexer::checkpoint_seq_by_digest::CheckpointSeqByDigest
            .process(checkpoint)
            .await
            .unwrap()
        {
            batch
                .put(
                    &schema.checkpoint_seq_by_digest,
                    &checkpoint_seq_by_digest::Key(row.digest),
                    &U64Varint(row.seq),
                )
                .unwrap();
        }
        batch.commit().unwrap();
    }

    fn seed_checkpoint_versions(
        db: &Db,
        schema: &RpcStoreSchema,
        id: ObjectID,
        rows: &[(u64, u64)],
    ) {
        let mut batch = db.batch();
        for &(checkpoint, version) in rows {
            let (k, v) = object_version_by_checkpoint::store(
                id,
                checkpoint,
                sui_types::base_types::SequenceNumber::from_u64(version),
            );
            batch
                .put(&schema.object_version_by_checkpoint, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();
    }

    #[test]
    fn retractions_stage_widest_range_per_object() {
        let (_dir, db, schema) = fresh_db();
        let obj = TestCheckpointBuilder::derive_object_id(0);
        seed_checkpoint_versions(&db, &schema, obj, &[(0, 1), (1, 2), (2, 3)]);

        let mut retractions = Retractions::default();
        retractions.record(obj, 1, true);
        retractions.record(obj, 2, false);
        let mut batch = db.batch();
        retractions
            .stage(&mut batch, &schema, &RetractionCursors::default())
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_object_version_at_checkpoint(obj, 1).unwrap(),
            None,
            "the widest range must cover lower-checkpoint removals",
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj, 2).unwrap(),
            Some(sui_types::base_types::SequenceNumber::from_u64(3)),
            "a lower removed=true retraction must not delete the latest anchor",
        );
    }

    #[test]
    fn retractions_or_removed_on_equal_checkpoint() {
        let (_dir, db, schema) = fresh_db();
        let obj = TestCheckpointBuilder::derive_object_id(0);
        seed_checkpoint_versions(&db, &schema, obj, &[(0, 1), (2, 3)]);

        let mut retractions = Retractions::default();
        retractions.record(obj, 2, false);
        retractions.record(obj, 2, true);
        let mut batch = db.batch();
        retractions
            .stage(&mut batch, &schema, &RetractionCursors::default())
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_object_version_at_checkpoint(obj, 2).unwrap(),
            None,
            "same-checkpoint removals must drop the checkpoint row",
        );
    }

    /// A hot object with deep checkpoint-pinned history (the Clock /
    /// SuiSystemState shape, superseded in every checkpoint) is cleared below
    /// the coalesced retraction checkpoint in a single pass, keeping only the
    /// anchor at that checkpoint. Exercises the point-delete prefix walk that
    /// replaced the per-object range delete, and confirms it deletes exactly
    /// the rows in `[id||0, id||cp)` without spilling into the next object.
    #[test]
    fn retraction_point_deletes_deep_history() {
        let (_dir, db, schema) = fresh_db();
        let obj = TestCheckpointBuilder::derive_object_id(0);
        let neighbor = TestCheckpointBuilder::derive_object_id(1);

        // The object changed in every checkpoint 0..1000 (version = cp + 1);
        // seed a neighboring object below the retraction floor to prove the
        // bounded scan does not cross the id boundary.
        let rows: Vec<(u64, u64)> = (0..1_000u64).map(|c| (c, c + 1)).collect();
        seed_checkpoint_versions(&db, &schema, obj, &rows);
        seed_checkpoint_versions(&db, &schema, neighbor, &[(10, 42)]);

        // Superseded in every checkpoint: without coalescing this would walk
        // the prefix 1000 times; the collector reduces it to one retraction at
        // the greatest checkpoint (999).
        let mut retractions = Retractions::default();
        for (checkpoint, _) in &rows {
            retractions.record(obj, *checkpoint, false);
        }
        let mut batch = db.batch();
        retractions
            .stage(&mut batch, &schema, &RetractionCursors::default())
            .unwrap();
        batch.commit().unwrap();

        // Everything below 999 is gone; the anchor at 999 survives as the floor
        // a point-in-time read resolves to.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj, 998).unwrap(),
            None,
            "history below the coalesced checkpoint must be fully point-deleted",
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj, 999).unwrap(),
            Some(sui_types::base_types::SequenceNumber::from_u64(1_000)),
            "the anchor at the coalesced checkpoint must survive",
        );
        let remaining: Vec<u64> = schema
            .iter_object_versions_by_checkpoint(obj)
            .unwrap()
            .map(|r| r.unwrap().0.checkpoint)
            .collect();
        assert_eq!(remaining, vec![999], "only the anchor row remains");

        // The neighboring object's rows are untouched.
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(neighbor, 10)
                .unwrap(),
            Some(sui_types::base_types::SequenceNumber::from_u64(42)),
            "the bounded scan must not delete a neighboring object's rows",
        );
    }

    #[test]
    fn clamp_to_snapshot_holds_floor_at_or_below_oldest_snapshot() {
        // No snapshots: retention floor stands.
        assert_eq!(clamp_to_snapshot(100, None), 100);
        // Retention is well below the oldest snapshot: retention binds.
        assert_eq!(clamp_to_snapshot(100, Some(250)), 100);
        // Retention would overrun the oldest snapshot: clamp holds.
        assert_eq!(clamp_to_snapshot(300, Some(250)), 250);
        // Exactly at the oldest snapshot is allowed.
        assert_eq!(clamp_to_snapshot(250, Some(250)), 250);
    }

    #[test]
    fn retention_floor_none_when_chain_younger_than_window() {
        let (_dir, _db, schema) = fresh_db();
        // current_epoch=2, retention=5 => oldest_retained saturates to
        // 0, so epoch 0 is still retained and nothing has aged out.
        assert!(retention_checkpoint_floor(&schema, 2, 5).unwrap().is_none());
    }

    #[test]
    fn retention_floor_is_start_checkpoint_of_oldest_retained_epoch() {
        let (_dir, db, schema) = fresh_db();
        // Seed epoch 3's start record at checkpoint 300.
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(3),
                &epochs::start(1, 1, 0, Some(300), None),
            )
            .unwrap();
        batch.commit().unwrap();
        // current_epoch=5, retention=3 => retain [3, 5], oldest
        // retained is epoch 3, whose start checkpoint is the floor.
        assert_eq!(
            retention_checkpoint_floor(&schema, 5, 3).unwrap(),
            Some(300)
        );
    }

    #[test]
    fn retention_floor_none_when_oldest_epoch_row_missing() {
        let (_dir, _db, schema) = fresh_db();
        // Oldest retained epoch is 9, but no row has been observed.
        assert!(
            retention_checkpoint_floor(&schema, 10, 2)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn current_committed_epoch_takes_min_across_watermarks() {
        let (_dir, db, _schema) = fresh_db();
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch
            .put(
                &framework.watermarks,
                &PipelineTaskKey::new("a"),
                &Watermark {
                    epoch_hi_inclusive: 7,
                    ..Default::default()
                },
            )
            .unwrap();
        batch
            .put(
                &framework.watermarks,
                &PipelineTaskKey::new("b"),
                &Watermark {
                    epoch_hi_inclusive: 5,
                    ..Default::default()
                },
            )
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(current_committed_epoch(&db).unwrap(), Some(5));
    }

    #[test]
    fn current_committed_epoch_none_when_no_watermarks() {
        let (_dir, db, _schema) = fresh_db();
        assert!(current_committed_epoch(&db).unwrap().is_none());
    }

    /// Production-shaped bitmap reclamation: merge operands survive the
    /// first covering compaction that materializes them, then expired
    /// buckets are filtered on the second while retained buckets remain.
    #[test]
    fn merge_written_bitmap_buckets_require_two_compactions_for_reclamation() {
        let (_dir, db, schema) = fresh_db();
        let dimension = b"sender:alice".to_vec();
        let floor = transaction_bitmap::TX_BUCKET_SIZE;
        let retained_tx_seq = floor + 5;

        let (tx_low_key, tx_low_value) = transaction_bitmap::store_match(dimension.clone(), 5);
        let (tx_high_key, tx_high_value) =
            transaction_bitmap::store_match(dimension.clone(), retained_tx_seq);
        let (event_low_key, event_low_value) = event_bitmap::store_match(dimension.clone(), 5, 0);
        let (event_high_key, event_high_value) =
            event_bitmap::store_match(dimension.clone(), retained_tx_seq, 0);

        let mut batch = db.batch();
        batch
            .merge(&schema.transaction_bitmap, &tx_low_key, &tx_low_value)
            .unwrap();
        batch
            .merge(&schema.transaction_bitmap, &tx_high_key, &tx_high_value)
            .unwrap();
        batch
            .merge(&schema.event_bitmap, &event_low_key, &event_low_value)
            .unwrap();
        batch
            .merge(&schema.event_bitmap, &event_high_key, &event_high_value)
            .unwrap();
        batch.commit().unwrap();
        db.flush().unwrap();

        let (watermark_key, watermark_value) = pruning_watermark::store(&Watermarks {
            tx_seq_lo: floor,
            checkpoint_lo: 1,
        });
        let mut batch = db.batch();
        batch
            .put(&schema.pruning_watermark, &watermark_key, &watermark_value)
            .unwrap();
        batch.commit().unwrap();
        schema.set_pruning_floor(floor);

        db.compact_range_cf(transaction_bitmap::NAME, None, None)
            .unwrap();
        db.compact_range_cf(event_bitmap::NAME, None, None).unwrap();

        assert!(
            schema
                .get_transaction_bitmap(dimension.clone(), tx_low_key.bucket)
                .unwrap()
                .is_some()
        );
        assert!(
            schema
                .get_transaction_bitmap(dimension.clone(), tx_high_key.bucket)
                .unwrap()
                .is_some()
        );
        assert!(
            schema
                .get_event_bitmap(dimension.clone(), event_low_key.bucket)
                .unwrap()
                .is_some()
        );
        assert!(
            schema
                .get_event_bitmap(dimension.clone(), event_high_key.bucket)
                .unwrap()
                .is_some()
        );

        db.compact_range_cf(transaction_bitmap::NAME, None, None)
            .unwrap();
        db.compact_range_cf(event_bitmap::NAME, None, None).unwrap();

        assert!(
            schema
                .get_transaction_bitmap(dimension.clone(), tx_low_key.bucket)
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .get_transaction_bitmap(dimension.clone(), tx_high_key.bucket)
                .unwrap()
                .is_some()
        );
        assert!(
            schema
                .get_event_bitmap(dimension.clone(), event_low_key.bucket)
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .get_event_bitmap(dimension, event_high_key.bucket)
                .unwrap()
                .is_some()
        );
    }

    /// A committed chunk publishes its `tx_seq_lo` to this database's
    /// bitmap compaction filters.
    #[tokio::test]
    async fn prune_chunk_publishes_the_db_local_bitmap_floor() {
        let (_dir, db, schema) = fresh_db();
        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(0)
                .start_transaction(0)
                .create_owned_object(0)
                .finish_transaction()
                .start_transaction(0)
                .transfer_object(0, 1)
                .finish_transaction()
                .build_checkpoint(),
        );
        seed(&db, &schema, &checkpoint).await;

        let metrics = PrunerMetrics::new(None, &Registry::new());
        let new = prune_chunk(
            &db,
            &schema,
            &mut RetractionCursors::default(),
            Watermarks::default(),
            1,
            &metrics,
        )
        .unwrap();
        assert_eq!(
            schema.current_pruning_floor(),
            new.tx_seq_lo,
            "the chunk must publish its committed tx_seq floor",
        );
    }

    #[test]
    fn start_pruner_rejects_zero_retention() {
        let (_dir, db, schema) = fresh_db();
        let store = Store::new(db, Arc::new(schema));
        let config = PrunerConfig {
            retention_epochs: 0,
            ..PrunerConfig::default()
        };
        let err =
            start_pruner(store, config, PrunerMetrics::new(None, &Registry::new())).unwrap_err();
        assert!(
            format!("{err:#}").contains("retention_epochs"),
            "expected a retention_epochs validation error, got: {err:#}",
        );
    }

    #[test]
    fn start_pruner_rejects_zero_checkpoints_per_tick() {
        let (_dir, db, schema) = fresh_db();
        let store = Store::new(db, Arc::new(schema));
        let config = PrunerConfig {
            max_checkpoints_per_tick: 0,
            ..PrunerConfig::default()
        };
        let err =
            start_pruner(store, config, PrunerMetrics::new(None, &Registry::new())).unwrap_err();
        assert!(
            format!("{err:#}").contains("max_checkpoints_per_tick"),
            "expected a max_checkpoints_per_tick validation error, got: {err:#}",
        );
    }

    /// A single `prune_once` pass advances the floor by at most
    /// `max_checkpoints_per_tick` checkpoints, and successive passes
    /// converge to the retention target. Five single-transaction
    /// checkpoints are eligible (retention floor at checkpoint 5); a
    /// per-tick budget of 2 must take three passes to drain them
    /// (2, 4, 5), after which the floor sits at the target and further
    /// passes are no-ops.
    #[tokio::test]
    async fn prune_once_advances_at_most_the_per_tick_budget() {
        let (_dir, db, schema) = fresh_db();

        // Five single-transaction checkpoints (seq 0..=4) from one
        // accumulating builder, so `network_total_transactions` grows
        // by one per checkpoint and the pruned tx range is contiguous.
        let mut builder = TestCheckpointBuilder::new(0);
        let mut checkpoints = Vec::new();
        for i in 0..5u64 {
            builder = builder
                .start_transaction(0)
                .create_owned_object(i)
                .finish_transaction();
            checkpoints.push(Arc::new(builder.build_checkpoint()));
        }
        for cp in &checkpoints {
            seed(&db, &schema, cp).await;
        }

        // Drive the target floor: the committed epoch is 2, and with
        // `retention_epochs = 1` the oldest retained epoch is 2, whose
        // start checkpoint (5) is the target floor — so checkpoints
        // [0, 5) are eligible.
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch
            .put(
                &framework.watermarks,
                &PipelineTaskKey::new("p"),
                &Watermark {
                    epoch_hi_inclusive: 2,
                    ..Default::default()
                },
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(2),
                &epochs::start(1, 1, 0, Some(5), None),
            )
            .unwrap();
        batch.commit().unwrap();

        let config = PrunerConfig {
            retention_epochs: 1,
            interval_ms: 1,
            max_chunk_checkpoints: 2,
            max_checkpoints_per_tick: 2,
        };
        let metrics = PrunerMetrics::new(None, &Registry::new());

        let floor = |schema: &RpcStoreSchema| {
            schema
                .get_pruning_watermarks()
                .unwrap()
                .unwrap_or_default()
                .checkpoint_lo
        };

        // Each pass advances by at most the per-tick budget of 2.
        let mut cursors = RetractionCursors::default();
        prune_once(&db, &schema, &mut cursors, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 2, "first tick advances by the budget");
        prune_once(&db, &schema, &mut cursors, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 4, "second tick advances by the budget");
        prune_once(&db, &schema, &mut cursors, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 5, "third tick reaches the target");
        // Caught up: history below the floor is gone, the live target
        // boundary is retained, and another pass is a no-op.
        assert!(schema.get_effects(4).unwrap().is_none());
        assert!(schema.get_checkpoint_summary(4).unwrap().is_none());
        prune_once(&db, &schema, &mut cursors, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 5, "a pass at the target is a no-op");
    }

    /// End-to-end chunk prune: one checkpoint where tx0 creates an
    /// object and tx1 transfers it (superseding the first version).
    /// Pruning the chunk must range-delete the per-tx / per-checkpoint
    /// CFs, point-delete the digest reverse indexes, retract the
    /// superseded object version, preserve the live version, and
    /// advance the persisted floor.
    #[tokio::test]
    async fn prune_chunk_deletes_history_and_preserves_live_object() {
        let (_dir, db, schema) = fresh_db();

        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(0)
                .start_transaction(0)
                .create_owned_object(0)
                .finish_transaction()
                .start_transaction(0)
                .transfer_object(0, 1)
                .finish_transaction()
                .build_checkpoint(),
        );

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let v_a = checkpoint.transactions[0].effects.lamport_version();
        let v_b = checkpoint.transactions[1].effects.lamport_version();
        assert_ne!(v_a, v_b, "the transfer must bump the object's version");
        let digest0 = *checkpoint.transactions[0].effects.transaction_digest();
        let digest1 = *checkpoint.transactions[1].effects.transaction_digest();
        let ckpt_digest = checkpoint.summary.data().digest();

        seed(&db, &schema, &checkpoint).await;

        // Preconditions: both versions present, history present.
        assert!(schema.get_object_by_key(obj0, v_a).unwrap().is_some());
        assert!(schema.get_object_by_key(obj0, v_b).unwrap().is_some());
        assert!(schema.get_effects(0).unwrap().is_some());
        assert!(schema.get_effects(1).unwrap().is_some());
        assert!(schema.get_checkpoint_summary(0).unwrap().is_some());

        // Prune the whole checkpoint: checkpoints [0, 1), tx [0, 2).
        let metrics = PrunerMetrics::new(None, &Registry::new());
        let new = prune_chunk(
            &db,
            &schema,
            &mut RetractionCursors::default(),
            Watermarks::default(),
            1,
            &metrics,
        )
        .unwrap();
        assert_eq!(
            new,
            Watermarks {
                tx_seq_lo: 2,
                checkpoint_lo: 1,
            },
        );

        // Superseded version retracted; live version preserved.
        assert!(
            schema.get_object_by_key(obj0, v_a).unwrap().is_none(),
            "superseded version v_a should be pruned",
        );
        assert!(
            schema.get_object_by_key(obj0, v_b).unwrap().is_some(),
            "live version v_b must be preserved",
        );

        // Range-deleted CFs are emptied over the pruned range.
        assert!(schema.get_effects(0).unwrap().is_none());
        assert!(schema.get_effects(1).unwrap().is_none());
        assert!(schema.get_checkpoint_summary(0).unwrap().is_none());

        // Point-deleted digest reverse indexes are gone.
        assert!(
            schema
                .tx_seq_by_digest
                .get(&tx_seq_by_digest::Key(digest0))
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .tx_seq_by_digest
                .get(&tx_seq_by_digest::Key(digest1))
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .checkpoint_seq_by_digest
                .get(&checkpoint_seq_by_digest::Key(ckpt_digest))
                .unwrap()
                .is_none()
        );

        // The persisted floor advanced.
        assert_eq!(
            schema.get_pruning_watermarks().unwrap().unwrap(),
            Watermarks {
                tx_seq_lo: 2,
                checkpoint_lo: 1,
            },
        );
    }

    /// Advance the floor across two single-checkpoint chunks and
    /// confirm a superseded object version is retracted only once the
    /// chunk containing its *superseding* transaction is pruned.
    ///
    /// Checkpoint 0 creates `obj0@v_a`; checkpoint 1 transfers it to
    /// `obj0@v_b`. Pruning checkpoint 0 alone must keep `v_a` (its
    /// superseding transaction is still live); pruning checkpoint 1
    /// then retracts `v_a` while preserving the live `v_b`.
    #[tokio::test]
    async fn prune_chunk_retracts_version_only_when_superseding_tx_is_pruned() {
        let (_dir, db, schema) = fresh_db();

        // One builder across two checkpoints so `network_total_transactions`
        // accumulates and the shared live-object set carries obj0 forward.
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let v_a = cp0.transactions[0].effects.lamport_version();
        let v_b = cp1.transactions[0].effects.lamport_version();
        assert_ne!(v_a, v_b);

        seed(&db, &schema, &cp0).await;
        seed(&db, &schema, &cp1).await;
        let metrics = PrunerMetrics::new(None, &Registry::new());
        let mut cursors = RetractionCursors::default();

        // Chunk 1: prune checkpoint 0 only (tx [0, 1)). obj0's
        // superseding transaction is in checkpoint 1, so v_a stays.
        let after_first = prune_chunk(
            &db,
            &schema,
            &mut cursors,
            Watermarks::default(),
            1,
            &metrics,
        )
        .unwrap();
        assert_eq!(
            after_first,
            Watermarks {
                tx_seq_lo: 1,
                checkpoint_lo: 1,
            },
        );
        assert!(schema.get_effects(0).unwrap().is_none());
        assert!(schema.get_effects(1).unwrap().is_some());
        assert!(
            schema.get_object_by_key(obj0, v_a).unwrap().is_some(),
            "v_a must survive while its superseding tx is still retained",
        );

        // Chunk 2: prune checkpoint 1 (tx [1, 2)). Now the superseding
        // transaction is pruned, retracting v_a; v_b remains live.
        let after_second =
            prune_chunk(&db, &schema, &mut cursors, after_first, 2, &metrics).unwrap();
        assert_eq!(
            after_second,
            Watermarks {
                tx_seq_lo: 2,
                checkpoint_lo: 2,
            },
        );
        assert!(schema.get_effects(1).unwrap().is_none());
        assert!(
            schema.get_object_by_key(obj0, v_a).unwrap().is_none(),
            "v_a must be retracted once its superseding tx is pruned",
        );
        assert!(
            schema.get_object_by_key(obj0, v_b).unwrap().is_some(),
            "live v_b must be preserved",
        );
    }

    /// The checkpoint-pinned `object_version_by_checkpoint` index is
    /// retracted in lockstep with the `objects` history: a
    /// checkpoint-pinned entry survives until the transaction that
    /// supersedes its object is pruned, and is dropped once that
    /// transaction's checkpoint ages out.
    ///
    /// Checkpoint 0 creates `obj0@v_a`; checkpoint 1 transfers it to
    /// `obj0@v_b`. Pruning checkpoint 0 keeps the cp0-pinned entry (its
    /// superseding transaction is still retained); pruning checkpoint 1
    /// retracts it while preserving the cp1-pinned floor entry.
    #[tokio::test]
    async fn prune_chunk_retracts_object_version_by_checkpoint() {
        use crate::indexer::object_version_by_checkpoint::ObjectVersionByCheckpoint;

        let (_dir, db, schema) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let v_a = cp0.transactions[0].effects.lamport_version();
        let v_b = cp1.transactions[0].effects.lamport_version();
        assert_ne!(v_a, v_b);

        // Seed the base CFs the pruner reads (`seed` populates
        // `checkpoint_summary`, from which the pruner derives each
        // transaction's checkpoint) plus the checkpoint-pinned index
        // under test.
        for cp in [&cp0, &cp1] {
            seed(&db, &schema, cp).await;
            let mut batch = db.batch();
            for row in ObjectVersionByCheckpoint::default()
                .process(cp)
                .await
                .unwrap()
            {
                // Seed only the change rows; the floor candidates are
                // exercised in the pipeline's own tests.
                let crate::indexer::object_version_by_checkpoint::Row::Change {
                    id,
                    checkpoint,
                    version,
                } = row
                else {
                    continue;
                };
                let (k, v) = object_version_by_checkpoint::store(id, checkpoint, version);
                batch
                    .put(&schema.object_version_by_checkpoint, &k, &v)
                    .unwrap();
            }
            batch.commit().unwrap();
        }

        // Precondition: obj0 resolves at both checkpoints.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            Some(v_a),
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 1).unwrap(),
            Some(v_b),
        );

        let metrics = PrunerMetrics::new(None, &Registry::new());
        let mut cursors = RetractionCursors::default();

        // Prune checkpoint 0 only: tx0 creates obj0 and supersedes
        // nothing, so the cp0-pinned entry survives.
        let after_first = prune_chunk(
            &db,
            &schema,
            &mut cursors,
            Watermarks::default(),
            1,
            &metrics,
        )
        .unwrap();
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            Some(v_a),
            "cp0-pinned entry must survive while its superseding tx is retained",
        );

        // Prune checkpoint 1: tx1 supersedes obj0@v_a, retracting the
        // cp0-pinned entry; the cp1-pinned floor entry remains.
        prune_chunk(&db, &schema, &mut cursors, after_first, 2, &metrics).unwrap();
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            None,
            "cp0-pinned entry must be retracted once its superseding tx is pruned",
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 1).unwrap(),
            Some(v_b),
            "cp1-pinned floor entry must be preserved",
        );
    }

    /// The embedded entry point retracts `object_version_by_checkpoint`
    /// effects-driven, matching the standalone `prune_chunk`: a superseded
    /// object keeps only its latest sub-floor row (the anchor), while a
    /// removed object's rows are dropped entirely.
    #[test]
    fn prune_history_cohort_retracts_object_version_by_checkpoint() {
        let (_dir, db, schema) = fresh_db();

        // Real checkpoints, built only for their effects: cp0 creates obj0
        // and obj1; cp1 transfers obj0 (supersedes it) and deletes obj1.
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(1)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let obj1 = TestCheckpointBuilder::derive_object_id(1);

        // Seed the checkpoint-pinned rows directly (values are immaterial to
        // the retraction): obj0 changed in cp0 and cp1; obj1 was created in
        // cp0 and tombstoned in cp1.
        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);
        let mut batch = db.batch();
        for (id, checkpoint, version) in [(obj0, 0, 1), (obj0, 1, 2), (obj1, 0, 1), (obj1, 1, 2)] {
            let (k, v) = object_version_by_checkpoint::store(id, checkpoint, ver(version));
            batch
                .put(&schema.object_version_by_checkpoint, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        // Precondition: both objects resolve at their creation checkpoint.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            Some(ver(1)),
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj1, 0).unwrap(),
            Some(ver(1)),
        );

        // Prune through checkpoint 1 (new floor 2), feeding the pruned
        // checkpoints' effects tagged with the checkpoint each came from.
        // `pruned_tx_seq_exclusive` is 0 here: the tx-keyed CFs are empty in
        // this test, and the checkpoint floor advancing alone is enough.
        let effects: Vec<(u64, TransactionEffects)> = cp0
            .transactions
            .iter()
            .map(|tx| (0u64, tx.effects.clone()))
            .chain(cp1.transactions.iter().map(|tx| (1u64, tx.effects.clone())))
            .collect();
        prune_history_cohort(
            &db,
            &schema,
            &mut RetractionCursors::default(),
            1,
            0,
            &effects,
        )
        .unwrap();
        // obj0 was superseded in cp1: its cp0 row is retracted, the cp1
        // anchor survives to resolve reads at or above the floor.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            None,
            "a superseded object's pre-anchor row must be retracted",
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 2).unwrap(),
            Some(ver(2)),
            "the anchor a read at the floor resolves to must survive",
        );

        // obj1 was removed in cp1: all its sub-floor rows are dropped.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj1, 2).unwrap(),
            None,
            "a removed object's rows must be dropped entirely",
        );

        // The floor advanced to the new lowest-available checkpoint.
        assert_eq!(
            schema
                .get_pruning_watermarks()
                .unwrap()
                .unwrap()
                .checkpoint_lo,
            2,
        );
    }

    /// `prune_history_cohort` (the embedded entry point) range-deletes
    /// `tx_metadata_by_seq`, point-deletes `tx_seq_by_digest` for the
    /// pruned digests, and advances the persisted floor — all from the
    /// floor the perpetual pruner supplies, without touching any raw
    /// chain-data CF.
    #[test]
    fn prune_history_cohort_deletes_tx_metadata_and_advances_floor() {
        use sui_types::digests::TransactionDigest;

        use crate::schema::tx_metadata_by_seq;

        let (_dir, db, schema) = fresh_db();

        // Six transactions, tx_seq 0..6, each with a metadata row and a
        // digest -> tx_seq reverse-index entry.
        let digests: Vec<TransactionDigest> =
            (0u8..6).map(|i| TransactionDigest::new([i; 32])).collect();
        let mut batch = db.batch();
        for (tx_seq, digest) in digests.iter().enumerate() {
            let tx_seq = tx_seq as u64;
            batch
                .put(
                    &schema.tx_metadata_by_seq,
                    &U64Be(tx_seq),
                    &tx_metadata_by_seq::store(&tx_metadata_by_seq::Metadata {
                        digest: *digest,
                        checkpoint_seq: tx_seq,
                        ckpt_position: 0,
                        event_count: 0,
                        timestamp_ms: 0,
                    }),
                )
                .unwrap();
            batch
                .put(
                    &schema.tx_seq_by_digest,
                    &tx_seq_by_digest::Key(*digest),
                    &U64Varint(tx_seq),
                )
                .unwrap();
        }
        batch.commit().unwrap();

        // Perpetual store has pruned through checkpoint 2; tx_seq 3 is
        // the first still-retained transaction.
        prune_history_cohort(&db, &schema, &mut RetractionCursors::default(), 2, 3, &[]).unwrap();
        // tx_metadata 0..3 pruned, 3..6 retained.
        for tx_seq in 0..3 {
            assert!(
                schema.get_tx_metadata_by_seq(tx_seq).unwrap().is_none(),
                "tx_metadata {tx_seq} should be pruned",
            );
        }
        for tx_seq in 3..6 {
            assert!(
                schema.get_tx_metadata_by_seq(tx_seq).unwrap().is_some(),
                "tx_metadata {tx_seq} should be retained",
            );
        }

        // Digest reverse index unindexed for the pruned range only.
        for digest in &digests[0..3] {
            assert!(schema.get_tx_seq_by_digest(digest).unwrap().is_none());
        }
        for digest in &digests[3..6] {
            assert!(schema.get_tx_seq_by_digest(digest).unwrap().is_some());
        }

        // Floor advanced: tx_seq 3 and checkpoint 3 (= pruned 2 + 1).
        assert_eq!(
            schema.get_pruning_watermarks().unwrap(),
            Some(Watermarks {
                tx_seq_lo: 3,
                checkpoint_lo: 3,
            }),
        );

        // Idempotent: a re-run at the same floor is a no-op.
        prune_history_cohort(&db, &schema, &mut RetractionCursors::default(), 2, 3, &[]).unwrap();
        assert_eq!(
            schema.get_pruning_watermarks().unwrap(),
            Some(Watermarks {
                tx_seq_lo: 3,
                checkpoint_lo: 3,
            }),
        );
    }

    /// `prune_history_cohort` visits only the rows that exist when the
    /// floor is unknown (no prior watermark, so `tx_lo == 0`) and the
    /// `tx_seq` range is sparse with large gaps — it must not walk every
    /// integer in the interval.
    #[test]
    fn prune_history_cohort_handles_sparse_tx_seqs() {
        use sui_types::digests::TransactionDigest;

        use crate::schema::tx_metadata_by_seq;

        let (_dir, db, schema) = fresh_db();

        // Three rows spread across a wide interval.
        let entries = [
            (0u64, [10u8; 32]),
            (500_000u64, [11u8; 32]),
            (999_999u64, [12u8; 32]),
        ];
        let mut batch = db.batch();
        for (tx_seq, digest_bytes) in entries {
            let digest = TransactionDigest::new(digest_bytes);
            batch
                .put(
                    &schema.tx_metadata_by_seq,
                    &U64Be(tx_seq),
                    &tx_metadata_by_seq::store(&tx_metadata_by_seq::Metadata {
                        digest,
                        checkpoint_seq: tx_seq,
                        ckpt_position: 0,
                        event_count: 0,
                        timestamp_ms: 0,
                    }),
                )
                .unwrap();
            batch
                .put(
                    &schema.tx_seq_by_digest,
                    &tx_seq_by_digest::Key(digest),
                    &U64Varint(tx_seq),
                )
                .unwrap();
        }
        batch.commit().unwrap();

        // No prior pruning watermark (floor unknown -> 0); prune through
        // checkpoint 0 / tx_seq 600_000 exclusive. Only the two rows
        // below 600_000 are unindexed; the one at 999_999 survives.
        prune_history_cohort(
            &db,
            &schema,
            &mut RetractionCursors::default(),
            0,
            600_000,
            &[],
        )
        .unwrap();
        assert!(schema.get_tx_metadata_by_seq(0).unwrap().is_none());
        assert!(schema.get_tx_metadata_by_seq(500_000).unwrap().is_none());
        assert!(schema.get_tx_metadata_by_seq(999_999).unwrap().is_some());
        assert!(
            schema
                .get_tx_seq_by_digest(&TransactionDigest::new([10u8; 32]))
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .get_tx_seq_by_digest(&TransactionDigest::new([11u8; 32]))
                .unwrap()
                .is_none()
        );
        assert!(
            schema
                .get_tx_seq_by_digest(&TransactionDigest::new([12u8; 32]))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            schema.get_pruning_watermarks().unwrap(),
            Some(Watermarks {
                tx_seq_lo: 600_000,
                checkpoint_lo: 1,
            }),
        );
    }
    fn all_object_version_by_checkpoint_rows(
        schema: &RpcStoreSchema,
    ) -> Vec<(
        object_version_by_checkpoint::Key,
        sui_types::base_types::SequenceNumber,
    )> {
        schema
            .object_version_by_checkpoint
            .iter(..)
            .unwrap()
            .map(|res| {
                let (key, value) = res.unwrap();
                (
                    key,
                    sui_types::base_types::SequenceNumber::from_u64(value.into_inner().version),
                )
            })
            .collect()
    }
    /// Differential test: an identical multi-batch prune sequence executed
    /// with persistent cursors vs. fresh/unwarmed cursors produces byte-identical
    /// `object_version_by_checkpoint` CF contents.
    #[test]
    fn prune_history_cohort_differential_with_warm_and_fresh_cursors() {
        let (_dir_warm, db_warm, schema_warm) = fresh_db();
        let (_dir_fresh, db_fresh, schema_fresh) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 2)
            .create_owned_object(3)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .delete_object(2)
            .create_owned_object(4)
            .finish_transaction();
        let cp2 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .transfer_object(3, 4)
            .delete_object(4)
            .finish_transaction();
        let cp3 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 4)
            .delete_object(1)
            .transfer_object(3, 5)
            .finish_transaction();
        let cp4 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 5)
            .finish_transaction();
        let cp5 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let obj1 = TestCheckpointBuilder::derive_object_id(1);
        let obj2 = TestCheckpointBuilder::derive_object_id(2);
        let obj3 = TestCheckpointBuilder::derive_object_id(3);
        let obj4 = TestCheckpointBuilder::derive_object_id(4);

        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);
        let rows = [
            (obj0, 0, 1),
            (obj0, 1, 2),
            (obj0, 2, 3),
            (obj0, 3, 4),
            (obj0, 4, 5),
            (obj0, 5, 6),
            (obj1, 0, 1),
            (obj1, 1, 2),
            (obj1, 4, 3),
            (obj2, 0, 1),
            (obj2, 2, 2),
            (obj3, 1, 1),
            (obj3, 3, 2),
            (obj3, 4, 3),
            (obj4, 2, 1),
            (obj4, 3, 2),
        ];

        for (db, schema) in [(&db_warm, &schema_warm), (&db_fresh, &schema_fresh)] {
            let mut batch = db.batch();
            for (id, cp, version) in rows {
                let (k, v) = object_version_by_checkpoint::store(id, cp, ver(version));
                batch
                    .put(&schema.object_version_by_checkpoint, &k, &v)
                    .unwrap();
            }
            batch.commit().unwrap();
        }

        let checkpoints = [
            (&cp0, 0u64),
            (&cp1, 1),
            (&cp2, 2),
            (&cp3, 3),
            (&cp4, 4),
            (&cp5, 5),
        ];

        // Batch 1: prune through cp 1
        let effects_b1: Vec<(u64, TransactionEffects)> = checkpoints[0..=1]
            .iter()
            .flat_map(|(cp, seq)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (*seq, tx.effects.clone()))
            })
            .collect();
        // Batch 2: prune through cp 3
        let effects_b2: Vec<(u64, TransactionEffects)> = checkpoints[2..=3]
            .iter()
            .flat_map(|(cp, seq)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (*seq, tx.effects.clone()))
            })
            .collect();
        // Batch 3: prune through cp 5
        let effects_b3: Vec<(u64, TransactionEffects)> = checkpoints[4..=5]
            .iter()
            .flat_map(|(cp, seq)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (*seq, tx.effects.clone()))
            })
            .collect();

        // Run with persistent warm cursors
        let mut warm_cursors = RetractionCursors::default();
        prune_history_cohort(&db_warm, &schema_warm, &mut warm_cursors, 1, 0, &effects_b1).unwrap();
        prune_history_cohort(&db_warm, &schema_warm, &mut warm_cursors, 3, 0, &effects_b2).unwrap();
        prune_history_cohort(&db_warm, &schema_warm, &mut warm_cursors, 5, 0, &effects_b3).unwrap();

        // Run with fresh cursors every batch (simulating full cache misses / lower bound 0 fallback)
        prune_history_cohort(
            &db_fresh,
            &schema_fresh,
            &mut RetractionCursors::default(),
            1,
            0,
            &effects_b1,
        )
        .unwrap();
        prune_history_cohort(
            &db_fresh,
            &schema_fresh,
            &mut RetractionCursors::default(),
            3,
            0,
            &effects_b2,
        )
        .unwrap();
        prune_history_cohort(
            &db_fresh,
            &schema_fresh,
            &mut RetractionCursors::default(),
            5,
            0,
            &effects_b3,
        )
        .unwrap();

        let rows_warm = all_object_version_by_checkpoint_rows(&schema_warm);
        let rows_fresh = all_object_version_by_checkpoint_rows(&schema_fresh);
        assert_eq!(rows_warm, rows_fresh);
        // Non-vacuity anchor: the retractions really deleted superseded
        // rows — obj0 (transferred in every checkpoint) keeps only its
        // cp5 anchor.
        let obj0_checkpoints: Vec<u64> = rows_warm
            .iter()
            .filter(|(k, _)| k.id == obj0)
            .map(|(k, _)| k.checkpoint)
            .collect();
        assert_eq!(obj0_checkpoints, vec![5], "obj0 keeps only its cp5 anchor");
    }

    /// Simulated restart test: dropping the cursor state mid-sequence produces
    /// CF contents identical to an uninterrupted run.
    #[test]
    fn prune_history_cohort_simulated_restart() {
        let (_dir_uninterrupted, db_uninterrupted, schema_uninterrupted) = fresh_db();
        let (_dir_restarted, db_restarted, schema_restarted) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 2)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .delete_object(1)
            .finish_transaction();
        let cp2 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .finish_transaction();
        let cp3 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let obj1 = TestCheckpointBuilder::derive_object_id(1);

        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);
        let rows = [
            (obj0, 0, 1),
            (obj0, 1, 2),
            (obj0, 2, 3),
            (obj0, 3, 4),
            (obj1, 0, 1),
            (obj1, 1, 2),
            (obj1, 2, 3),
        ];

        for (db, schema) in [
            (&db_uninterrupted, &schema_uninterrupted),
            (&db_restarted, &schema_restarted),
        ] {
            let mut batch = db.batch();
            for (id, cp, version) in rows {
                let (k, v) = object_version_by_checkpoint::store(id, cp, ver(version));
                batch
                    .put(&schema.object_version_by_checkpoint, &k, &v)
                    .unwrap();
            }
            batch.commit().unwrap();
        }

        let checkpoints = [(&cp0, 0u64), (&cp1, 1), (&cp2, 2), (&cp3, 3)];

        let effects_b1: Vec<(u64, TransactionEffects)> = checkpoints[0..=1]
            .iter()
            .flat_map(|(cp, seq)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (*seq, tx.effects.clone()))
            })
            .collect();
        let effects_b2: Vec<(u64, TransactionEffects)> = checkpoints[2..=3]
            .iter()
            .flat_map(|(cp, seq)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (*seq, tx.effects.clone()))
            })
            .collect();

        // Uninterrupted run
        let mut uninterrupted_cursors = RetractionCursors::default();
        prune_history_cohort(
            &db_uninterrupted,
            &schema_uninterrupted,
            &mut uninterrupted_cursors,
            1,
            0,
            &effects_b1,
        )
        .unwrap();
        prune_history_cohort(
            &db_uninterrupted,
            &schema_uninterrupted,
            &mut uninterrupted_cursors,
            3,
            0,
            &effects_b2,
        )
        .unwrap();

        // Restarted run: cursors dropped/reset mid-sequence
        let mut restarted_cursors = RetractionCursors::default();
        prune_history_cohort(
            &db_restarted,
            &schema_restarted,
            &mut restarted_cursors,
            1,
            0,
            &effects_b1,
        )
        .unwrap();
        // Drop and recreate cursors (simulating restart)
        restarted_cursors = RetractionCursors::default();
        prune_history_cohort(
            &db_restarted,
            &schema_restarted,
            &mut restarted_cursors,
            3,
            0,
            &effects_b2,
        )
        .unwrap();

        let rows_uninterrupted = all_object_version_by_checkpoint_rows(&schema_uninterrupted);
        let rows_restarted = all_object_version_by_checkpoint_rows(&schema_restarted);
        assert_eq!(rows_uninterrupted, rows_restarted);
    }

    /// Hot object across batches: an object superseded in every checkpoint
    /// over >= 3 batches keeps exactly its latest anchor each time.
    #[test]
    fn prune_history_cohort_hot_object_across_batches() {
        let (_dir, db, schema) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .finish_transaction();
        let cp2 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .finish_transaction();
        let cp3 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 4)
            .finish_transaction();
        let cp4 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 5)
            .finish_transaction();
        let cp5 = Arc::new(builder.build_checkpoint());

        let hot_obj = TestCheckpointBuilder::derive_object_id(0);
        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);

        let mut batch = db.batch();
        for (checkpoint, version) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)] {
            let (k, v) = object_version_by_checkpoint::store(hot_obj, checkpoint, ver(version));
            batch
                .put(&schema.object_version_by_checkpoint, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        let mut cursors = RetractionCursors::default();

        // Batch 1: prune through checkpoint 1 (new floor 2)
        let effects_b1 = vec![
            (0u64, cp0.transactions[0].effects.clone()),
            (1u64, cp1.transactions[0].effects.clone()),
        ];
        prune_history_cohort(&db, &schema, &mut cursors, 1, 0, &effects_b1).unwrap();
        assert_eq!(cursors.lower_bound(&hot_obj), 1);
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 0).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 1).unwrap(),
            Some(ver(2))
        );

        // Batch 2: prune through checkpoint 3 (new floor 4)
        let effects_b2 = vec![
            (2u64, cp2.transactions[0].effects.clone()),
            (3u64, cp3.transactions[0].effects.clone()),
        ];
        prune_history_cohort(&db, &schema, &mut cursors, 3, 0, &effects_b2).unwrap();
        assert_eq!(cursors.lower_bound(&hot_obj), 3);
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 0).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 1).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 2).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 3).unwrap(),
            Some(ver(4))
        );

        // Batch 3: prune through checkpoint 5 (new floor 6)
        let effects_b3 = vec![
            (4u64, cp4.transactions[0].effects.clone()),
            (5u64, cp5.transactions[0].effects.clone()),
        ];
        prune_history_cohort(&db, &schema, &mut cursors, 5, 0, &effects_b3).unwrap();
        assert_eq!(cursors.lower_bound(&hot_obj), 5);
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 3).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 4).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(hot_obj, 6).unwrap(),
            Some(ver(6))
        );
    }

    /// Eviction fallback: with a capacity of one, every batch's cursor
    /// advances evict each other, so the next batch's retractions run
    /// from the `lo_cp = 0` fallback — and must delete exactly the same
    /// rows a default-capacity (warm-cursor) run deletes. The default
    /// arm retaining more cursors than the tiny arm proves evictions
    /// actually happened.
    #[test]
    fn prune_history_cohort_eviction_fallback() {
        let (_dir_tiny, db_tiny, schema_tiny) = fresh_db();
        let (_dir_default, db_default, schema_default) = fresh_db();

        // One continuous builder: cp0 creates three objects, cp1..cp3
        // each transfer all three, so every checkpoint records real
        // supersessions for all of them.
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let cp0 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let cp1 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .transfer_object(1, 2)
            .transfer_object(2, 2)
            .finish_transaction();
        let cp2 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .transfer_object(1, 3)
            .transfer_object(2, 3)
            .finish_transaction();
        let cp3 = builder.build_checkpoint();

        let objs: Vec<ObjectID> = (0..3u64)
            .map(TestCheckpointBuilder::derive_object_id)
            .collect();
        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);

        for (db, schema) in [(&db_tiny, &schema_tiny), (&db_default, &schema_default)] {
            let mut batch = db.batch();
            for &id in &objs {
                for cp in 0..4u64 {
                    let (k, v) = object_version_by_checkpoint::store(id, cp, ver(cp + 1));
                    batch
                        .put(&schema.object_version_by_checkpoint, &k, &v)
                        .unwrap();
                }
            }
            batch.commit().unwrap();
        }

        let effects_b1: Vec<(u64, TransactionEffects)> = [(0u64, &cp0), (1, &cp1)]
            .into_iter()
            .flat_map(|(seq, cp)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (seq, tx.effects.clone()))
            })
            .collect();
        let effects_b2: Vec<(u64, TransactionEffects)> = [(2u64, &cp2), (3, &cp3)]
            .into_iter()
            .flat_map(|(seq, cp)| {
                cp.transactions
                    .iter()
                    .map(move |tx| (seq, tx.effects.clone()))
            })
            .collect();

        // Tiny arm: capacity 1 — each advance evicts the previous entry,
        // so batch 2 runs its scans from the checkpoint-0 fallback.
        let mut tiny_cursors = RetractionCursors::with_capacity(1);
        prune_history_cohort(&db_tiny, &schema_tiny, &mut tiny_cursors, 1, 0, &effects_b1).unwrap();
        assert!(
            objs.iter()
                .filter(|id| tiny_cursors.lower_bound(id) > 0)
                .count()
                <= 1,
            "capacity 1 must have evicted all but at most one cursor"
        );
        prune_history_cohort(&db_tiny, &schema_tiny, &mut tiny_cursors, 3, 0, &effects_b2).unwrap();

        // Default arm: every cursor stays warm across batches.
        let mut default_cursors = RetractionCursors::default();
        prune_history_cohort(
            &db_default,
            &schema_default,
            &mut default_cursors,
            1,
            0,
            &effects_b1,
        )
        .unwrap();
        for id in &objs {
            assert_eq!(
                default_cursors.lower_bound(id),
                1,
                "default capacity keeps every cursor warm"
            );
        }
        prune_history_cohort(
            &db_default,
            &schema_default,
            &mut default_cursors,
            3,
            0,
            &effects_b2,
        )
        .unwrap();

        // Evictions actually happened: the tiny arm holds one cursor
        // where the default arm holds one per recorded object.
        assert_eq!(tiny_cursors.len(), 1);
        assert!(default_cursors.len() > tiny_cursors.len());

        // Fallback scans deleted exactly what warm scans deleted: only
        // each object's cp3 anchor survives, in both arms.
        let rows_tiny = all_object_version_by_checkpoint_rows(&schema_tiny);
        let rows_default = all_object_version_by_checkpoint_rows(&schema_default);
        assert_eq!(rows_tiny, rows_default);
        let mut expected: Vec<_> = objs
            .iter()
            .map(|&id| {
                (
                    object_version_by_checkpoint::Key { id, checkpoint: 3 },
                    ver(4),
                )
            })
            .collect();
        expected.sort_by_key(|(k, _)| k.id);
        assert_eq!(rows_tiny, expected);
    }

    /// `removed` at or below cursor: an object removed at a checkpoint where
    /// the cursor is already at or above the removal checkpoint deletes the
    /// tombstone without resurrecting old rows or erroring.
    #[test]
    fn prune_history_cohort_removed_at_or_below_cursor() {
        let (_dir, db, schema) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let _cp0 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);

        let mut batch = db.batch();
        let (k0, v0) = object_version_by_checkpoint::store(obj0, 0, ver(1));
        let (k1, v1) = object_version_by_checkpoint::store(obj0, 1, ver(2));
        batch
            .put(&schema.object_version_by_checkpoint, &k0, &v0)
            .unwrap();
        batch
            .put(&schema.object_version_by_checkpoint, &k1, &v1)
            .unwrap();
        batch.commit().unwrap();

        let mut cursors = RetractionCursors::default();

        // Step 1: Supersession in cp1 retracts cp0 and sets cursor to 1.
        let effects_supersede: Vec<(u64, TransactionEffects)> =
            vec![(1u64, cp1.transactions[0].effects.clone())];
        prune_history_cohort(&db, &schema, &mut cursors, 0, 0, &effects_supersede).unwrap();
        assert_eq!(cursors.lower_bound(&obj0), 1);
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 1).unwrap(),
            Some(ver(2))
        );

        // Step 2: Removal at cp1 where cursor is already 1 (cursor >= cp).
        let effects_remove: Vec<(u64, TransactionEffects)> =
            vec![(1u64, cp1.transactions[1].effects.clone())];
        prune_history_cohort(&db, &schema, &mut cursors, 1, 0, &effects_remove).unwrap();
        assert_eq!(cursors.lower_bound(&obj0), 1);

        // The tombstone at cp1 is deleted, no resurrecting rows below cp1.
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 1).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 2).unwrap(),
            None
        );

        // Step 3: Idempotent re-run with cursor >= cp is a clean no-op without error.
        prune_history_cohort(&db, &schema, &mut cursors, 1, 0, &effects_remove).unwrap();
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 1).unwrap(),
            None
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 2).unwrap(),
            None
        );
    }

    /// LRU recency: `advance` refreshes an entry's recency, so an object
    /// advanced in every batch stays resident under capacity pressure
    /// while one-shot cold objects are evicted around it. Capacity 4
    /// with three distinct objects per batch (one hot + two fresh colds)
    /// keeps eviction pressure on stale colds, never the hot entry.
    #[test]
    fn retraction_cursors_hot_object_survives_lru_churn() {
        let hot = TestCheckpointBuilder::derive_object_id(0);
        let mut cursors = RetractionCursors::with_capacity(4);
        for batch in 1..=5u64 {
            cursors.advance(hot, batch);
            for j in 0..2u64 {
                cursors.advance(
                    TestCheckpointBuilder::derive_object_id(100 + batch * 10 + j),
                    batch,
                );
            }
            assert_eq!(
                cursors.lower_bound(&hot),
                batch,
                "hot object must survive batch {batch}'s cold churn"
            );
        }
        // A cold object from the first batch fell back to the
        // checkpoint-0 default; the cache holds only the newest entries.
        assert_eq!(
            cursors.lower_bound(&TestCheckpointBuilder::derive_object_id(110)),
            0
        );
        assert_eq!(cursors.len(), 4);
    }

    /// Stale-floor re-delivery with the cursor strictly above the
    /// effect's checkpoint (`lo_cp > cp`): the call passes the no-op
    /// gate on the tx axis alone, the retraction's scan range is empty,
    /// nothing is deleted, and the cursor holds its high-water mark.
    #[test]
    fn prune_history_cohort_stale_floor_redelivery_is_noop() {
        let (_dir, db, schema) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let cp1 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .finish_transaction();
        let cp2 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .finish_transaction();
        let cp3 = builder.build_checkpoint();

        let obj0 = TestCheckpointBuilder::derive_object_id(0);
        let ver = |n: u64| sui_types::base_types::SequenceNumber::from_u64(n);

        let mut batch = db.batch();
        for cp in 0..4u64 {
            let (k, v) = object_version_by_checkpoint::store(obj0, cp, ver(cp + 1));
            batch
                .put(&schema.object_version_by_checkpoint, &k, &v)
                .unwrap();
        }
        batch.commit().unwrap();

        // Retract through cp3: rows below 3 deleted, the cp3 anchor
        // kept, cursor at 3.
        let effects_all: Vec<(u64, TransactionEffects)> =
            [(0u64, &cp0), (1, &cp1), (2, &cp2), (3, &cp3)]
                .into_iter()
                .flat_map(|(seq, cp)| {
                    cp.transactions
                        .iter()
                        .map(move |tx| (seq, tx.effects.clone()))
                })
                .collect();
        let mut cursors = RetractionCursors::default();
        prune_history_cohort(&db, &schema, &mut cursors, 3, 0, &effects_all).unwrap();
        assert_eq!(cursors.lower_bound(&obj0), 3);
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 3).unwrap(),
            Some(ver(4))
        );

        // Re-deliver cp1's effect with a stale checkpoint floor; the tx
        // axis advances, so the call passes the no-op gate and reaches
        // the retraction with `lo_cp = 3 > cp = 1`.
        let redelivered: Vec<(u64, TransactionEffects)> =
            vec![(1, cp1.transactions[0].effects.clone())];
        prune_history_cohort(&db, &schema, &mut cursors, 1, 5, &redelivered).unwrap();

        // No deletion, the anchor survives, and the cursor held its
        // high-water mark instead of regressing to 1.
        assert_eq!(cursors.lower_bound(&obj0), 3);
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 3).unwrap(),
            Some(ver(4))
        );
        let rows = all_object_version_by_checkpoint_rows(&schema);
        assert_eq!(
            rows,
            vec![(
                object_version_by_checkpoint::Key {
                    id: obj0,
                    checkpoint: 3
                },
                ver(4)
            )]
        );
    }

    /// Standalone `prune_chunk` path coverage: verifies cursor advance,
    /// differential consistency, and simulated restart across multiple chunks.
    #[tokio::test]
    async fn prune_chunk_standalone_retraction_cursors_and_restart() {
        use crate::indexer::object_version_by_checkpoint::ObjectVersionByCheckpoint;

        let (_dir1, db1, schema1) = fresh_db();
        let (_dir2, db2, schema2) = fresh_db();
        let (_dir3, db3, schema3) = fresh_db();

        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 2)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 2)
            .delete_object(1)
            .finish_transaction();
        let cp2 = Arc::new(builder.build_checkpoint());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 3)
            .finish_transaction();
        let cp3 = Arc::new(builder.build_checkpoint());

        let checkpoints = [&cp0, &cp1, &cp2, &cp3];

        for (db, schema) in [(&db1, &schema1), (&db2, &schema2), (&db3, &schema3)] {
            for cp in &checkpoints {
                seed(db, schema, cp).await;
                let mut batch = db.batch();
                for row in ObjectVersionByCheckpoint::default()
                    .process(cp)
                    .await
                    .unwrap()
                {
                    let crate::indexer::object_version_by_checkpoint::Row::Change {
                        id,
                        checkpoint,
                        version,
                    } = row
                    else {
                        continue;
                    };
                    let (k, v) = object_version_by_checkpoint::store(id, checkpoint, version);
                    batch
                        .put(&schema.object_version_by_checkpoint, &k, &v)
                        .unwrap();
                }
                batch.commit().unwrap();
            }
        }

        let metrics = PrunerMetrics::new(None, &Registry::new());

        // Run 1: Continuous cursors across chunks
        let mut cursors1 = RetractionCursors::default();
        let w1 = prune_chunk(
            &db1,
            &schema1,
            &mut cursors1,
            Watermarks::default(),
            2,
            &metrics,
        )
        .unwrap();
        prune_chunk(&db1, &schema1, &mut cursors1, w1, 4, &metrics).unwrap();

        // Run 2: Fresh cursors per chunk
        let w2 = prune_chunk(
            &db2,
            &schema2,
            &mut RetractionCursors::default(),
            Watermarks::default(),
            2,
            &metrics,
        )
        .unwrap();
        prune_chunk(
            &db2,
            &schema2,
            &mut RetractionCursors::default(),
            w2,
            4,
            &metrics,
        )
        .unwrap();

        // Run 3: Simulated restart between chunks
        let mut cursors3 = RetractionCursors::default();
        let w3 = prune_chunk(
            &db3,
            &schema3,
            &mut cursors3,
            Watermarks::default(),
            2,
            &metrics,
        )
        .unwrap();
        cursors3 = RetractionCursors::default();
        prune_chunk(&db3, &schema3, &mut cursors3, w3, 4, &metrics).unwrap();

        let rows1 = all_object_version_by_checkpoint_rows(&schema1);
        let rows2 = all_object_version_by_checkpoint_rows(&schema2);
        let rows3 = all_object_version_by_checkpoint_rows(&schema3);

        assert_eq!(rows1, rows2);
        assert_eq!(rows1, rows3);
        assert!(!rows1.is_empty());
    }
}

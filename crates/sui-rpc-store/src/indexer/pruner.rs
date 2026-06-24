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
//!   so it — and the `live_objects` pointer at it — is preserved.
//! - **`object_version_by_checkpoint`** — retracted in lockstep with
//!   `objects` history: the same effects-driven walk issues a
//!   per-object range delete clearing every checkpoint-pinned entry
//!   below the superseding transaction's checkpoint, plus a point
//!   delete of the tombstone entry when the object was removed. The
//!   retained set mirrors the `objects` versions kept, so the index
//!   never points at a pruned version.
//! - **Ledger-history bitmaps** (`transaction_bitmap`,
//!   `event_bitmap`) — not deleted directly; advancing the shared
//!   [`tx_seq_floor`](crate::schema::pruning_watermark::tx_seq_floor)
//!   lets their compaction filters drop fully-pruned buckets. We
//!   force a compaction once the floor advances so the eviction is
//!   prompt rather than waiting for a natural sweep.
//!
//! The live-set-bounded indexes (`live_objects`, `object_by_owner`,
//! `object_by_type`, `balance`, `package_versions`) and the tiny
//! `epochs` CF are never pruned.
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

use std::sync::Arc;

use anyhow::Context as _;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;
use sui_consistent_store::Batch;
use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::Schema;
use sui_indexer_alt_framework::service::Service;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::message_envelope::Message;
use tokio::time::MissedTickBehavior;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::RpcStoreSchema;
use crate::config::PrunerConfig;
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

/// Start the background pruner as a [`Service`].
///
/// Errors if `config.retention_epochs` is `0` (which would prune the
/// current epoch). The returned service runs an infinite tick loop;
/// it is aborted on graceful shutdown (each chunk is atomic, so an
/// abort leaves the database consistent).
pub fn start_pruner(
    db: Db,
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

    // Reconstruct typed handles once; they are cheap views over the
    // shared `Db` and are reused across every tick.
    let schema = Arc::new(RpcStoreSchema::open(&db).context("Opening schema for pruner")?);

    let service = Service::new().spawn_aborting(async move {
        let mut ticker = tokio::time::interval(config.interval());
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            let db = db.clone();
            let schema = schema.clone();
            let config = config.clone();
            let metrics = metrics.clone();

            // The pruner does blocking RocksDB iteration and writes;
            // keep it off the async runtime threads.
            let res =
                tokio::task::spawn_blocking(move || prune_once(&db, &schema, &config, &metrics))
                    .await;

            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!("rpc-store pruner pass failed (will retry next interval): {e:#}")
                }
                Err(e) => warn!("rpc-store pruner task join error: {e}"),
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
        cursor = prune_chunk(db, schema, cursor, chunk_ckpt_hi, metrics)?;
        metrics.checkpoint_lo.set(cursor.checkpoint_lo as i64);
        metrics.tx_seq_lo.set(cursor.tx_seq_lo as i64);
        metrics.chunks_committed.inc();
    }

    // The bitmap CFs' compaction filters only drop fully-pruned
    // buckets on a compaction sweep; force one once the floor has
    // reached its retention target so the eviction is prompt. While a
    // backlog is still draining over multiple ticks we skip the
    // whole-CF compaction so it does not become the per-tick long
    // pole; natural background compaction still applies the same
    // filter opportunistically in the meantime, and the final
    // catch-up tick forces a prompt sweep.
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
                // Retract checkpoint-pinned entries older than this
                // supersession; the entry at `seq` (the object's final
                // version in this checkpoint) is kept.
                retract_object_version_by_checkpoint(&mut batch, schema, id, seq, false)?;
                objects_deleted += 1;
            }
            for (id, version) in effects.all_tombstones() {
                batch.delete(&schema.objects, &objects::Key { id, version })?;
                // The object was removed in `seq`: drop its tombstone
                // entry at `seq` too.
                retract_object_version_by_checkpoint(&mut batch, schema, id, seq, true)?;
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

    // Advance the persisted floor atomically with the deletes.
    let new = Watermarks {
        tx_seq_lo: tx_hi,
        checkpoint_lo: chunk_ckpt_hi,
    };
    let (k, v) = pruning_watermark::store(&new);
    batch.put(&schema.pruning_watermark, &k, &v)?;

    batch.commit()?;

    // The commit is durable; advance the in-memory bitmap floor so
    // the compaction filters drop buckets below `tx_hi`.
    schema.set_pruning_floor(new.tx_seq_lo);
    metrics.objects_deleted.inc_by(objects_deleted);

    Ok(new)
}

/// Retract `object_version_by_checkpoint` rows for one object, given a
/// transaction at checkpoint `cp` that superseded or removed it, in
/// lockstep with the `objects` CF.
///
/// Deletes every checkpoint-pinned entry for `id` strictly older than
/// `cp` with a single per-object range delete. Once the floor advances
/// past `cp`, the entry at `cp` (or a newer one) is the floor a
/// checkpoint-pinned read resolves to, so the older entries can never
/// be the answer again. Because the chunk only prunes checkpoints below
/// the new floor, `cp` is itself below the floor, so the kept entry is
/// never the answer to an in-range read either; it survives only until
/// its own superseding transaction is pruned in a later chunk.
///
/// The entry *at* `cp` is kept for a supersession (it is the object's
/// final live version in `cp`). When `removed` is set, the object was
/// deleted or wrapped in `cp`: its tombstone entry at `cp` is dropped
/// too, since nothing at or after the floor can reference a removed
/// object.
fn retract_object_version_by_checkpoint(
    batch: &mut Batch,
    schema: &RpcStoreSchema,
    id: ObjectID,
    cp: u64,
    removed: bool,
) -> anyhow::Result<()> {
    let lo = object_version_by_checkpoint::Key { id, checkpoint: 0 };
    let hi = object_version_by_checkpoint::Key { id, checkpoint: cp };
    batch.delete_range(&schema.object_version_by_checkpoint, &lo, &hi)?;
    if removed {
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
/// `checkpoint_*`), so it can neither derive a retention floor nor walk
/// effects to find the rows to delete. Instead the perpetual pruner —
/// which owns the raw data — supplies the floor directly, and this
/// prunes exactly the history-cohort CFs that grow without bound:
///
/// - `tx_metadata_by_seq` — range-deleted over
///   `[old_tx_lo, pruned_tx_seq_exclusive)`.
/// - `tx_seq_by_digest` — point-deleted; the digests are read from
///   `tx_metadata_by_seq` (the only history CF that still carries them)
///   over the pruned range, before that range is deleted.
/// - `transaction_bitmap` / `event_bitmap` — evicted by advancing the
///   shared `tx_seq` floor so their compaction filters drop
///   fully-pruned buckets, then forcing a compaction.
///
/// The live cohort and the tiny `epochs` CF are never pruned.
///
/// `pruned_checkpoint_watermark` is the highest checkpoint the
/// perpetual store has pruned (inclusive); `pruned_tx_seq_exclusive` is
/// the first still-retained `tx_seq`. These mirror
/// `sui_core::rpc_index::RpcIndexStore::prune`'s parameters so the
/// embedded rpc-store and the legacy index prune in lockstep on the
/// same floor. Idempotent: a re-run with the same or a lower floor is a
/// no-op.
pub fn prune_history_cohort(
    db: &Db,
    schema: &RpcStoreSchema,
    pruned_checkpoint_watermark: u64,
    pruned_tx_seq_exclusive: u64,
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

    // Durable now: advance the in-memory bitmap floor and force a
    // compaction so the bitmap filters drop fully-pruned buckets
    // promptly rather than waiting for a natural sweep.
    schema.set_pruning_floor(new.tx_seq_lo);
    db.compact_range_cf(transaction_bitmap::NAME, None, None)
        .context("Compacting transaction_bitmap after prune")?;
    db.compact_range_cf(event_bitmap::NAME, None, None)
        .context("Compacting event_bitmap after prune")?;

    Ok(())
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

    /// The bitmap eviction path end to end: with the floor advanced
    /// past a bucket, a forced compaction runs the bucket's
    /// compaction filter and drops it, while a bucket above the floor
    /// survives. This is what `prune_once` relies on when it compacts
    /// the bitmap CFs after a floor advance.
    #[test]
    fn bitmap_buckets_below_floor_are_evicted_by_compaction() {
        use std::sync::atomic::Ordering;

        use crate::schema::pruning_watermark::tx_seq_floor;
        use crate::schema::transaction_bitmap;

        // The floor is process-wide; snapshot and restore it so this
        // test doesn't perturb others sharing the same atomic.
        let baseline = tx_seq_floor().load(Ordering::Relaxed);
        let (_dir, db, schema) = fresh_db();
        let dim = b"sender:alice".to_vec();

        // Materialize one bucket fully below the floor (bucket 0) and
        // one above it (bucket 1). The compaction filter keys off the
        // bucket id in the key, so the stored bitmap contents are
        // immaterial here.
        let mut bitmap0 = roaring::RoaringBitmap::new();
        bitmap0.insert(transaction_bitmap::bit_of(5));
        let mut bitmap1 = roaring::RoaringBitmap::new();
        bitmap1.insert(transaction_bitmap::bit_of(
            transaction_bitmap::TX_BUCKET_SIZE + 5,
        ));
        let (k0, v0) = transaction_bitmap::store_bitmap(dim.clone(), 0, bitmap0);
        let (k1, v1) = transaction_bitmap::store_bitmap(dim.clone(), 1, bitmap1);

        let mut batch = db.batch();
        batch.put(&schema.transaction_bitmap, &k0, &v0).unwrap();
        batch.put(&schema.transaction_bitmap, &k1, &v1).unwrap();
        batch.commit().unwrap();
        db.flush().unwrap();

        // Advance the floor to the top of bucket 0, then force a
        // compaction. Bucket 0's whole range is below the floor, so
        // its filter returns Remove; bucket 1 straddles above it.
        schema.set_pruning_floor(transaction_bitmap::TX_BUCKET_SIZE);
        db.compact_range_cf(transaction_bitmap::NAME, None, None)
            .unwrap();

        assert!(
            schema
                .get_transaction_bitmap(dim.clone(), 0)
                .unwrap()
                .is_none(),
            "fully-pruned bucket 0 should be evicted by compaction",
        );
        assert!(
            schema.get_transaction_bitmap(dim, 1).unwrap().is_some(),
            "bucket 1 above the floor must remain",
        );

        tx_seq_floor().store(baseline, Ordering::Relaxed);
    }

    /// A committed chunk advances the process-wide bitmap floor (the
    /// value the bitmap CFs' compaction filters read) to the chunk's
    /// new `tx_seq_lo`. The filter's own removal logic is covered by
    /// `transaction_bitmap::should_remove_bucket`.
    #[tokio::test]
    async fn prune_chunk_advances_the_bitmap_floor_atomic() {
        use std::sync::atomic::Ordering;

        use crate::schema::pruning_watermark::tx_seq_floor;

        // The floor is process-wide; snapshot and restore it so this
        // test doesn't perturb others sharing the same atomic.
        let baseline = tx_seq_floor().load(Ordering::Relaxed);

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
        let new = prune_chunk(&db, &schema, Watermarks::default(), 1, &metrics).unwrap();

        assert_eq!(
            tx_seq_floor().load(Ordering::Relaxed),
            new.tx_seq_lo,
            "the chunk must publish its new tx_seq floor to the bitmap atomic",
        );

        tx_seq_floor().store(baseline, Ordering::Relaxed);
    }

    #[test]
    fn start_pruner_rejects_zero_retention() {
        let (_dir, db, _schema) = fresh_db();
        let config = PrunerConfig {
            retention_epochs: 0,
            ..PrunerConfig::default()
        };
        let err = start_pruner(db, config, PrunerMetrics::new(None, &Registry::new())).unwrap_err();
        assert!(
            format!("{err:#}").contains("retention_epochs"),
            "expected a retention_epochs validation error, got: {err:#}",
        );
    }

    #[test]
    fn start_pruner_rejects_zero_checkpoints_per_tick() {
        let (_dir, db, _schema) = fresh_db();
        let config = PrunerConfig {
            max_checkpoints_per_tick: 0,
            ..PrunerConfig::default()
        };
        let err = start_pruner(db, config, PrunerMetrics::new(None, &Registry::new())).unwrap_err();
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
        use std::sync::atomic::Ordering;

        use crate::schema::pruning_watermark::tx_seq_floor;

        // The floor is process-wide; snapshot and restore it so this
        // test doesn't perturb others sharing the same atomic.
        let baseline = tx_seq_floor().load(Ordering::Relaxed);

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
        prune_once(&db, &schema, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 2, "first tick advances by the budget");
        prune_once(&db, &schema, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 4, "second tick advances by the budget");
        prune_once(&db, &schema, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 5, "third tick reaches the target");

        // Caught up: history below the floor is gone, the live target
        // boundary is retained, and another pass is a no-op.
        assert!(schema.get_effects(4).unwrap().is_none());
        assert!(schema.get_checkpoint_summary(4).unwrap().is_none());
        prune_once(&db, &schema, &config, &metrics).unwrap();
        assert_eq!(floor(&schema), 5, "a pass at the target is a no-op");

        tx_seq_floor().store(baseline, Ordering::Relaxed);
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
        let new = prune_chunk(&db, &schema, Watermarks::default(), 1, &metrics).unwrap();
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

        // Chunk 1: prune checkpoint 0 only (tx [0, 1)). obj0's
        // superseding transaction is in checkpoint 1, so v_a stays.
        let after_first = prune_chunk(&db, &schema, Watermarks::default(), 1, &metrics).unwrap();
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
        let after_second = prune_chunk(&db, &schema, after_first, 2, &metrics).unwrap();
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

        // Prune checkpoint 0 only: tx0 creates obj0 and supersedes
        // nothing, so the cp0-pinned entry survives.
        let after_first = prune_chunk(&db, &schema, Watermarks::default(), 1, &metrics).unwrap();
        assert_eq!(
            schema.get_object_version_at_checkpoint(obj0, 0).unwrap(),
            Some(v_a),
            "cp0-pinned entry must survive while its superseding tx is retained",
        );

        // Prune checkpoint 1: tx1 supersedes obj0@v_a, retracting the
        // cp0-pinned entry; the cp1-pinned floor entry remains.
        prune_chunk(&db, &schema, after_first, 2, &metrics).unwrap();
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
        prune_history_cohort(&db, &schema, 2, 3).unwrap();

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
        prune_history_cohort(&db, &schema, 2, 3).unwrap();
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
        prune_history_cohort(&db, &schema, 0, 600_000).unwrap();

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
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Entry point for bulk-loading the [`RpcStoreSchema`]'s
//! derived-index CFs from a [`RestoreSource`].
//!
//! Registers the three live-object-derivable index pipelines
//! ([`ObjectByOwner`], [`ObjectByType`], [`Balance`]) plus the
//! [`ObjectVersionByCheckpoint`] and [`PackageVersions`] floor rows —
//! and, when the caller's [`RestoreLayer`] opts in, the raw
//! [`Objects`] CF — against a single [`RestoreDriver`] and returns a
//! [`Service`] driving the restore through to completion. Once
//! finished, every registered
//! pipeline's `__restore` row is `Complete` and its `__watermark`
//! row is set to the source's target, so the regular
//! [`Indexer::add_pipelines`] path will accept them for tip
//! indexing.
//!
//! Restoration is run separately from tip indexing — open the
//! database, call [`restore_indexes`] to populate the indexes,
//! then construct an [`Indexer`] over the same store to start
//! tip-following.
//!
//! [`Indexer`]: crate::Indexer
//! [`Indexer::add_pipelines`]: crate::Indexer::add_pipelines

use std::sync::Arc;

use anyhow::Context as _;
use sui_consistent_store::Batch;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Schema as _;
use sui_consistent_store::Watermark;
use sui_consistent_store::restore::RestoreDriver;
use sui_consistent_store::restore::RestoreDriverConfig;
use sui_consistent_store::restore::RestoreSource;
use sui_consistent_store::restore::metrics::RestoreMetrics;
use sui_futures::service::Service;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::storage::ObjectStore;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::get_sui_system_state;
use tracing::info;
use tracing::warn;

use crate::RestoreLayer;
use crate::RpcStoreReader;
use crate::RpcStoreSchema;
use crate::indexer::balance::Balance;
use crate::indexer::checkpoint_contents::CheckpointContents;
use crate::indexer::checkpoint_seq_by_digest::CheckpointSeqByDigest;
use crate::indexer::checkpoint_summary::CheckpointSummary;
use crate::indexer::effects::Effects;
use crate::indexer::epochs::Epochs;
use crate::indexer::event_bitmap::EventBitmap;
use crate::indexer::events::Events;
use crate::indexer::object_by_owner::ObjectByOwner;
use crate::indexer::object_by_type::ObjectByType;
use crate::indexer::object_version_by_checkpoint::ObjectVersionByCheckpoint;
use crate::indexer::objects::Objects;
use crate::indexer::package_versions::PackageVersions;
use crate::indexer::transaction_bitmap::TransactionBitmap;
use crate::indexer::transactions::Transactions;
use crate::indexer::tx_metadata_by_seq::TxMetadataBySeq;
use crate::indexer::tx_seq_by_digest::TxSeqByDigest;
use crate::schema::epochs;
use crate::schema::primitives::U64Be;
use crate::schema::pruning_watermark;

/// The embedded fullnode's **live cohort**: the pipelines that
/// [`restore_indexes`] bulk-loads and that are restored to the
/// perpetual store's tip `T`, then follow live from there. They are
/// bounded by the live object set, so a snapshot restore reproduces
/// them exactly.
///
/// Matches the live half of
/// [`PipelineLayer::embedded`](crate::config::PipelineLayer::embedded);
/// the `embedded_registers_only_cohort_pipelines` test pins the two
/// together.
pub const LIVE_COHORT: &[&str] = &[ObjectByOwner::NAME, ObjectByType::NAME, Balance::NAME];

/// The embedded fullnode's **history cohort**: the pipelines seeded to
/// the lowest available checkpoint `L` and backfilled upward from the
/// perpetual store, then followed live.
///
/// Most cannot be reconstructed from a live-object snapshot at all --
/// they record ledger history (`tx_seq` <-> digest maps, the
/// transaction and event bitmaps) and per-epoch metadata (`epochs`) --
/// so they are seeded, never restored.
///
/// `object_version_by_checkpoint` and `package_versions` are the
/// exceptions: they are *both* restored and backfilled.
/// [`restore_indexes`] bulk-loads their floor rows at the tip `T` (the
/// versions live in the snapshot but predate the available window, so a
/// checkpoint-bounded read treats them as having always existed), and
/// the history seed then rewinds their `__watermark` to `L-1` so they
/// also backfill the per-checkpoint detail over `(L, T]` --
/// `object_version_by_checkpoint`'s per-checkpoint changes, and
/// `package_versions`'s real publish checkpoint for versions published
/// in the window. The embedded bootstrap runs the restore before the
/// seed, so the `L-1` watermark wins.
///
/// Matches the history half of
/// [`PipelineLayer::embedded`](crate::config::PipelineLayer::embedded).
pub const HISTORY_COHORT: &[&str] = &[
    Epochs::NAME,
    ObjectVersionByCheckpoint::NAME,
    PackageVersions::NAME,
    TxSeqByDigest::NAME,
    TxMetadataBySeq::NAME,
    TransactionBitmap::NAME,
    EventBitmap::NAME,
];

/// Every rpc-store pipeline. Kept exhaustive so any new pipeline
/// added to [`PipelineLayer`](crate::config::PipelineLayer) needs an
/// explicit decision about restore behaviour (in
/// [`floor_unrestored_pipelines`]) and gets a stable name for
/// availability policies.
pub const ALL_PIPELINES: &[&str] = &[
    Epochs::NAME,
    CheckpointSummary::NAME,
    CheckpointContents::NAME,
    CheckpointSeqByDigest::NAME,
    Transactions::NAME,
    TxSeqByDigest::NAME,
    TxMetadataBySeq::NAME,
    Effects::NAME,
    Events::NAME,
    Objects::NAME,
    ObjectVersionByCheckpoint::NAME,
    ObjectByOwner::NAME,
    ObjectByType::NAME,
    Balance::NAME,
    PackageVersions::NAME,
    TransactionBitmap::NAME,
    EventBitmap::NAME,
];

/// Register every [`Restore`]-implementing pipeline opted in by
/// `layer` on a [`RestoreDriver`] bound to `db` / `schema` and
/// `source`, then run the resulting [`Service`].
///
/// The live-cohort pipelines are always registered, plus
/// `object_version_by_checkpoint` and `package_versions` -- history-
/// cohort members whose floor rows are bulk-loaded from the live set
/// here (the history seed separately rewinds their watermarks so they
/// also backfill `(L, T]`). The raw [`Objects`] pipeline is only
/// registered when `layer.objects` is set. The returned `Service`'s
/// primary task completes once every registered pipeline transitions to
/// [`RestoreState::Complete`].
///
/// [`Restore`]: sui_consistent_store::Restore
/// [`RestoreState::Complete`]: sui_consistent_store::restore_state::Complete
pub fn restore_indexes<Src: RestoreSource>(
    db: Db,
    schema: Arc<RpcStoreSchema>,
    source: Src,
    config: RestoreDriverConfig,
    layer: RestoreLayer,
    metrics: Arc<RestoreMetrics>,
) -> anyhow::Result<Service> {
    // Capture the anchor before the driver consumes `source`: the
    // checkpoint-pinned object index attributes every restored live
    // object to it.
    let target_checkpoint = source.target_checkpoint();
    let mut driver = RestoreDriver::new(db, schema, source, config, metrics);
    // History-cohort members, but their floor rows are restored from the
    // live set; the embedded history seed later rewinds their watermarks
    // so they also backfill `(L, T]`.
    driver.register(ObjectVersionByCheckpoint::for_restore(target_checkpoint))?;
    driver.register(PackageVersions)?;
    driver.register(ObjectByOwner)?;
    driver.register(ObjectByType)?;
    driver.register(Balance)?;
    if layer.objects {
        driver.register(Objects)?;
    }
    driver.run()
}

/// After [`restore_indexes`] returns, prime the framework state of
/// every pipeline that the restore did *not* cover so tip indexing
/// resumes from `target_watermark.checkpoint_hi_inclusive + 1`
/// across the board instead of replaying from genesis for the
/// raw-chain-data and bitmap pipelines.
///
/// Specifically, for every pipeline not in `layer`'s restored
/// set, writes:
///
/// - `__watermark = target_watermark` — the framework's
///   tip-resume reads this and starts at
///   `checkpoint_hi_inclusive + 1`.
/// - `__chain_id = target_chain_id` — pins the pipeline to the
///   chain the snapshot was taken from, matching what
///   [`restore_indexes`]'s finalize step already wrote for the
///   restored pipelines.
///
/// Also writes the singleton `pruning_watermark` so
/// `available_range` queries and the bitmap CFs' compaction
/// filters reflect that data only starts at the post-restore
/// floor (`tx_seq_lo = target_watermark.tx_hi`,
/// `checkpoint_lo = checkpoint_hi_inclusive + 1`).
///
/// Idempotent: re-running after a successful restore overwrites
/// the unrestored pipelines' watermarks with the same values and
/// re-writes the pruning row.
pub fn floor_unrestored_pipelines(
    db: &Db,
    target_watermark: Watermark,
    target_chain_id: ChainId,
    layer: &RestoreLayer,
) -> anyhow::Result<()> {
    let restored: &[&'static str] = if layer.objects {
        &[
            ObjectVersionByCheckpoint::NAME,
            ObjectByOwner::NAME,
            ObjectByType::NAME,
            Balance::NAME,
            PackageVersions::NAME,
            Objects::NAME,
        ]
    } else {
        &[
            ObjectVersionByCheckpoint::NAME,
            ObjectByOwner::NAME,
            ObjectByType::NAME,
            Balance::NAME,
            PackageVersions::NAME,
        ]
    };

    // Use the owned `FrameworkSchema` over `Db` (rather than the
    // borrowed view from `Db::framework`) so the `DbMap`s line up
    // with `Batch::put`'s `R = Db` expectation.
    let framework = FrameworkSchema::new(db.clone());
    let mut batch = db.batch();
    for name in ALL_PIPELINES.iter().filter(|n| !restored.contains(n)) {
        let key = PipelineTaskKey::new(*name);
        batch
            .put(&framework.watermarks, &key, &target_watermark)
            .with_context(|| format!("stage __watermark for {name:?}"))?;
        batch
            .put(&framework.chain_ids, &key, &target_chain_id)
            .with_context(|| format!("stage __chain_id for {name:?}"))?;
    }

    // Resolve the rpc-store schema handle once for the
    // pruning-watermark CF. The schema is cheap to re-bind to a
    // live `Db` and gives the typed `store` helper plus the
    // pruning-floor setter the bitmap CFs depend on.
    let schema =
        Arc::new(RpcStoreSchema::open(db).context("re-open RpcStoreSchema for pruning watermark")?);
    let (k, v) = pruning_watermark::store(&pruning_watermark::Watermarks {
        tx_seq_lo: target_watermark.tx_hi,
        checkpoint_lo: target_watermark.checkpoint_hi_inclusive.saturating_add(1),
    });
    batch
        .put(&schema.pruning_watermark, &k, &v)
        .context("stage pruning_watermark row")?;

    // Seed the `epochs` row for the epoch the snapshot lands in. The
    // chain advanced to it at the anchor's end-of-epoch checkpoint,
    // but the `epochs` pipeline only emits a start record while
    // processing such a checkpoint, which tip indexing skips on
    // resume (it starts at anchor + 1). Without this seed, the
    // current epoch's row would never get its protocol version, gas
    // price, start timestamp, start checkpoint, or system state.
    // Requires the raw objects to read the on-chain `SuiSystemState`,
    // so it only runs when the `objects` CF was restored; failure to
    // read it is logged and skipped rather than failing the restore.
    if layer.objects {
        let reader = RpcStoreReader::new(db.clone(), schema.clone());
        let start_checkpoint = target_watermark.checkpoint_hi_inclusive.saturating_add(1);
        match seed_current_epoch_start(&schema, &reader, Some(start_checkpoint), &mut batch) {
            Ok(epoch) => info!(
                epoch,
                start_checkpoint, "seeded start record for restore epoch"
            ),
            Err(e) => warn!(
                error = %e,
                "could not seed the restore epoch's start record; get_epoch / \
                 get_committee / Move type-layout resolution for the current epoch \
                 will be unavailable until the next epoch boundary",
            ),
        }
    }

    batch.commit().context("commit floor batch")?;

    // Mirror the on-disk floor into the process-wide atomic so any
    // compaction-filter clones started in this process see the
    // updated value immediately. A subsequent `Indexer::from_store`
    // also calls `refresh_pruning_atomics` so cross-process boots
    // converge.
    schema.set_pruning_floor(target_watermark.tx_hi);

    Ok(())
}

/// Stage a start record for the epoch reflected by the on-chain
/// `SuiSystemState` in `objects`, keyed by that epoch.
///
/// The `epochs` pipeline derives a start record only from an
/// end-of-epoch checkpoint's `epoch_info`, which a restore-then-tip
/// flow never processes (tip indexing resumes at `anchor + 1`), so a
/// freshly restored database has no start record for the epoch it
/// landed in. This reconstructs that record straight from the
/// restored object set: protocol version, reference gas price, and
/// epoch-start timestamp come from the `SuiSystemState`, the BCS of
/// which is stored so `get_committee` and Move type-layout resolution
/// work too.
///
/// `start_checkpoint` is supplied by the caller and may be `None`.
/// The formal-snapshot restore lands at an epoch boundary, so it
/// passes `Some(anchor + 1)`. The embedded-fullnode restore lands at
/// a *mid-epoch* tip, so the epoch's first checkpoint is unknown and
/// it passes `None`; the upward backfill fills `start_checkpoint` in
/// later if that boundary falls within the available range.
///
/// Stages a merge into `batch`; the caller commits. Returns the epoch
/// that was seeded.
pub fn seed_current_epoch_start(
    schema: &RpcStoreSchema,
    objects: &dyn ObjectStore,
    start_checkpoint: Option<u64>,
    batch: &mut Batch,
) -> anyhow::Result<u64> {
    let system_state =
        get_sui_system_state(objects).context("read SuiSystemState from restored objects")?;
    let epoch = system_state.epoch();
    let system_state_bcs = bcs::to_bytes(&system_state).context("bcs encode SuiSystemState")?;
    batch
        .merge(
            &schema.epochs,
            &U64Be(epoch),
            &epochs::start(
                system_state.protocol_version(),
                system_state.reference_gas_price(),
                system_state.epoch_start_timestamp_ms(),
                start_checkpoint,
                Some(system_state_bcs),
            ),
        )
        .context("stage epoch start seed")?;
    Ok(epoch)
}

/// Seed the framework state for the embedded fullnode's
/// [`HISTORY_COHORT`] after [`restore_indexes`] has bulk-loaded the
/// [`LIVE_COHORT`].
///
/// The live cohort resumes from the restore target `T` (written by
/// the restore driver's finalize step). The history cohort is *not*
/// restored; instead each of its pipelines is seeded to
/// `history_watermark` — the lowest available checkpoint `L` in the
/// perpetual store — so tip indexing backfills `(L, T]` from the
/// perpetual store and then follows live. For each history pipeline
/// this writes:
///
/// - `__watermark = history_watermark` — the framework resumes at
///   `history_watermark.checkpoint_hi_inclusive + 1`.
/// - `__chain_id = chain_id` — pins the chain, matching what the
///   restore driver wrote for the live cohort.
///
/// When `objects` is supplied, also seeds the current epoch's
/// `epochs` row from its on-chain `SuiSystemState` — a *partial*
/// start record without `start_checkpoint` (see
/// [`seed_current_epoch_start`]) — so `get_epoch` / `get_committee`
/// and Move type-layout resolution work immediately after restore
/// rather than only once the backfill reaches the epoch's boundary.
/// `objects` is read through the [`ObjectStore`] trait, so the
/// embedded caller passes the validator's perpetual store directly
/// (this crate stays free of any `sui-core` dependency).
///
/// Stamps the singleton `pruning_watermark` at the lowest available
/// checkpoint `L` — the first checkpoint the backfill will index,
/// `history_watermark.checkpoint_hi_inclusive + 1` — with
/// `tx_seq_lo = history_watermark.tx_hi` (the first `tx_seq` that
/// checkpoint contributes). This records that nothing below `L` is
/// available and sets the bitmap compaction filter's floor. The
/// backfill only ever writes `tx_seq` at or above the floor, so the
/// filter drops nothing it produces. (The *upper* bound of history
/// availability while a backfill is in progress is a separate
/// concern, handled elsewhere.)
///
/// Idempotent: re-running overwrites the same rows. Does not touch
/// the live cohort or the deactivated (perpetual-store-served) CFs.
pub fn seed_history_cohort(
    db: &Db,
    schema: &RpcStoreSchema,
    history_watermark: Watermark,
    chain_id: ChainId,
    objects: Option<&dyn ObjectStore>,
) -> anyhow::Result<()> {
    let framework = FrameworkSchema::new(db.clone());
    let mut batch = db.batch();

    for name in HISTORY_COHORT {
        let key = PipelineTaskKey::new(*name);
        batch
            .put(&framework.watermarks, &key, &history_watermark)
            .with_context(|| format!("stage __watermark for {name:?}"))?;
        batch
            .put(&framework.chain_ids, &key, &chain_id)
            .with_context(|| format!("stage __chain_id for {name:?}"))?;
    }

    // Stamp the pruning floor at the lowest available checkpoint `L`
    // (the first checkpoint the backfill will index). `tx_seq_lo` is
    // the tx count through the seed point, i.e. the first `tx_seq`
    // checkpoint `L` contributes, so the bitmap compaction filter
    // drops nothing the backfill produces.
    let tx_seq_lo = history_watermark.tx_hi;
    let (k, v) = pruning_watermark::store(&pruning_watermark::Watermarks {
        tx_seq_lo,
        checkpoint_lo: history_watermark.checkpoint_hi_inclusive.saturating_add(1),
    });
    batch
        .put(&schema.pruning_watermark, &k, &v)
        .context("stage pruning_watermark row")?;

    if let Some(objects) = objects {
        // Mid-epoch restore: the epoch's first checkpoint precedes the
        // tip, so seed a partial start record (no `start_checkpoint`).
        match seed_current_epoch_start(schema, objects, None, &mut batch) {
            Ok(epoch) => info!(epoch, "seeded partial start record for the current epoch"),
            Err(e) => warn!(
                error = %e,
                "could not seed the current epoch's start record; get_epoch / \
                 get_committee / Move type-layout resolution for the current epoch \
                 will be unavailable until the backfill reaches its boundary",
            ),
        }
    }

    batch.commit().context("commit history-cohort seed batch")?;

    // Mirror the on-disk floor into the process-wide atomic so any
    // bitmap compaction-filter clones in this process see it
    // immediately (a subsequent `Indexer::from_store` also calls
    // `refresh_pruning_atomics`).
    schema.set_pruning_floor(tx_seq_lo);

    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures::StreamExt;
    use futures::stream;
    use futures::stream::BoxStream;
    use sui_consistent_store::ChainId;
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_consistent_store::PipelineTaskKey;
    use sui_consistent_store::Watermark;
    use sui_consistent_store::restore::RestoreChunk;
    use sui_consistent_store::restore_state;
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::object::Object;

    use super::*;
    use crate::RpcStoreSchema;
    use crate::indexer::objects::Objects;
    use crate::schema::object_by_owner::OwnerKind;

    /// Minimal [`RestoreSource`] that wraps a `Vec<RestoreChunk>`
    /// and uses the 4-byte BE chunk index as cursor. Lets us
    /// drive the end-to-end pipeline registration / commit path
    /// without standing up a real snapshot.
    struct VecSource {
        target: u64,
        chain_id: ChainId,
        chunks: Vec<RestoreChunk>,
    }

    impl VecSource {
        fn from_objects(target: u64, chain_id: ChainId, objects: Vec<Vec<Object>>) -> Self {
            let chunks = objects
                .into_iter()
                .enumerate()
                .map(|(i, objs)| RestoreChunk {
                    objects: objs,
                    cursor: Bytes::copy_from_slice(&(i as u32).to_be_bytes()),
                })
                .collect();
            Self {
                target,
                chain_id,
                chunks,
            }
        }
    }

    #[async_trait]
    impl RestoreSource for VecSource {
        fn target_checkpoint(&self) -> u64 {
            self.target
        }

        fn target_chain_id(&self) -> ChainId {
            self.chain_id
        }

        fn shards(&self) -> u32 {
            1
        }

        fn stream(
            &self,
            shard_id: u32,
            cursor: Option<Bytes>,
        ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
            assert_eq!(shard_id, 0);
            let resume_after = cursor.map(|c| {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&c[..4]);
                u32::from_be_bytes(buf)
            });
            let chunks: Vec<_> = self
                .chunks
                .iter()
                .enumerate()
                .filter_map(|(i, chunk)| {
                    let i = i as u32;
                    if let Some(after) = resume_after
                        && i <= after
                    {
                        None
                    } else {
                        Some(Ok(RestoreChunk {
                            objects: chunk.objects.clone(),
                            cursor: chunk.cursor.clone(),
                        }))
                    }
                })
                .collect();
            stream::iter(chunks).boxed()
        }
    }

    /// End-to-end: drive a handful of address-owned objects
    /// through every registered pipeline. Verifies that the
    /// rows we expect end up in `object_version_by_checkpoint` and
    /// `object_by_owner`, and that every pipeline's
    /// `__restore` / `__watermark` rows are set up for the
    /// tip-indexer to take over.
    #[tokio::test]
    async fn restore_indexes_populates_schema_and_finalises() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let schema = Arc::new(schema);

        let owner = SuiAddress::random_for_testing_only();
        let objects: Vec<Object> = (1..=4u8)
            .map(|i| Object::with_id_owner_for_testing(ObjectID::from_single_byte(i), owner))
            .collect();

        let chain_id = ChainId([7u8; 32]);
        let source = VecSource::from_objects(123, chain_id, vec![objects.clone()]);

        restore_indexes(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
            RestoreLayer::indexes_only(),
            RestoreMetrics::new(None, &prometheus::Registry::new()),
        )
        .unwrap()
        .shutdown()
        .await
        .unwrap();

        // Each object's restore floor row landed in the checkpoint-pinned
        // index at the anchor checkpoint (123).
        for o in &objects {
            assert_eq!(
                schema
                    .get_object_version_at_checkpoint(o.id(), 123)
                    .unwrap(),
                Some(o.version()),
            );
        }

        // Owner index has every object under the same
        // AddressOwner(owner) key.
        let owned: Vec<(OwnerKind, ObjectID)> = schema
            .iter_objects_owned_by_address(owner)
            .unwrap()
            .map(Result::unwrap)
            .map(|(key, _v)| (key.kind, key.object_id))
            .collect();
        let mut got_ids: Vec<_> = owned.iter().map(|(_, id)| *id).collect();
        got_ids.sort();
        let mut expected_ids: Vec<_> = objects.iter().map(|o| o.id()).collect();
        expected_ids.sort();
        assert_eq!(got_ids, expected_ids);
        for (kind, _) in &owned {
            assert!(matches!(kind, OwnerKind::AddressOwner(addr) if *addr == owner));
        }

        // `indexes_only` did not register `objects`, so the
        // `(id, version)` CF stays empty.
        for o in &objects {
            assert_eq!(schema.get_object_by_key(o.id(), o.version()).unwrap(), None,);
        }

        // Every registered pipeline finished and has __restore
        // Complete, __watermark, and __chain_id all set. `objects`
        // was not registered with `indexes_only`, so it has no
        // __restore row at all.
        for name in [
            ObjectVersionByCheckpoint::NAME,
            ObjectByOwner::NAME,
            ObjectByType::NAME,
            Balance::NAME,
            PackageVersions::NAME,
        ] {
            let key = PipelineTaskKey::new(name);
            let state = db.framework().restore.get(&key).unwrap().unwrap();
            match state.state.unwrap() {
                restore_state::State::Complete(c) => assert_eq!(c.restored_at, 123),
                other => panic!("expected Complete, got {other:?}"),
            }
            let wm = db.framework().watermarks.get(&key).unwrap().unwrap();
            assert_eq!(wm, Watermark::for_checkpoint(123));
            let pinned_chain_id = db.framework().chain_ids.get(&key).unwrap().unwrap();
            assert_eq!(pinned_chain_id, chain_id);
        }
        let objects_key = PipelineTaskKey::new(Objects::NAME);
        assert!(
            db.framework().restore.get(&objects_key).unwrap().is_none(),
            "indexes_only should leave the objects pipeline unregistered",
        );
    }

    /// `floor_unrestored_pipelines` writes a `__watermark` /
    /// `__chain_id` row for every pipeline outside the restored
    /// set and stamps the singleton `pruning_watermark` so the
    /// available range tracks the post-restore floor.
    #[test]
    fn floor_unrestored_pipelines_writes_watermarks_for_tip_only_pipelines() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let chain_id = ChainId([42u8; 32]);
        let target = Watermark {
            epoch_hi_inclusive: 7,
            checkpoint_hi_inclusive: 1_000,
            tx_hi: 5_000,
            timestamp_ms_hi_inclusive: 1_700_000_000_000,
        };

        floor_unrestored_pipelines(&db, target, chain_id, &RestoreLayer::all()).unwrap();

        // Sample raw-chain-data / bitmap pipelines that the
        // formal-snapshot path doesn't cover — every one of them
        // should be primed with the target watermark + chain id.
        for name in [
            Epochs::NAME,
            CheckpointSummary::NAME,
            CheckpointContents::NAME,
            CheckpointSeqByDigest::NAME,
            Transactions::NAME,
            TxSeqByDigest::NAME,
            TxMetadataBySeq::NAME,
            Effects::NAME,
            Events::NAME,
            TransactionBitmap::NAME,
            EventBitmap::NAME,
        ] {
            let key = PipelineTaskKey::new(name);
            assert_eq!(
                db.framework().watermarks.get(&key).unwrap(),
                Some(target),
                "{name} should have the post-restore watermark",
            );
            assert_eq!(
                db.framework().chain_ids.get(&key).unwrap(),
                Some(chain_id),
                "{name} should pin the restored chain id",
            );
        }

        // Restored pipelines are left to whatever the restore
        // driver wrote (here: nothing, since we didn't actually
        // run a restore in this test). The helper must not
        // clobber them.
        for name in [
            ObjectVersionByCheckpoint::NAME,
            ObjectByOwner::NAME,
            ObjectByType::NAME,
            Balance::NAME,
            PackageVersions::NAME,
            Objects::NAME,
        ] {
            let key = PipelineTaskKey::new(name);
            assert!(
                db.framework().watermarks.get(&key).unwrap().is_none(),
                "{name} watermark should be untouched by the floor helper",
            );
            assert!(
                db.framework().chain_ids.get(&key).unwrap().is_none(),
                "{name} chain id should be untouched by the floor helper",
            );
        }

        // Pruning singleton reflects the post-restore floor: tx
        // ids and checkpoint sequences below this row aren't
        // available in the new database.
        let schema = RpcStoreSchema::open(&db).unwrap();
        assert_eq!(
            schema.get_pruning_watermarks().unwrap(),
            Some(crate::schema::pruning_watermark::Watermarks {
                tx_seq_lo: target.tx_hi,
                checkpoint_lo: target.checkpoint_hi_inclusive + 1,
            }),
        );
    }

    /// With `RestoreLayer::indexes_only`, the `objects` pipeline
    /// is *not* in the restored set, so the floor helper primes
    /// it the same way it does the raw-chain-data pipelines.
    #[test]
    fn floor_unrestored_pipelines_includes_objects_when_layer_skips_it() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let chain_id = ChainId([11u8; 32]);
        let target = Watermark::for_checkpoint(42);

        floor_unrestored_pipelines(&db, target, chain_id, &RestoreLayer::indexes_only()).unwrap();

        let key = PipelineTaskKey::new(Objects::NAME);
        assert_eq!(db.framework().watermarks.get(&key).unwrap(), Some(target),);
        assert_eq!(db.framework().chain_ids.get(&key).unwrap(), Some(chain_id));
    }

    /// `RestoreLayer::all` additionally registers the `objects`
    /// pipeline, so every restored live object lands in the
    /// `(id, version)` CF and the pipeline itself transitions to
    /// `Complete`.
    #[tokio::test]
    async fn restore_indexes_with_objects_layer_populates_objects_cf() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let schema = Arc::new(schema);

        let owner = SuiAddress::random_for_testing_only();
        let objects: Vec<Object> = (1..=4u8)
            .map(|i| Object::with_id_owner_for_testing(ObjectID::from_single_byte(i), owner))
            .collect();

        let chain_id = ChainId([9u8; 32]);
        let source = VecSource::from_objects(123, chain_id, vec![objects.clone()]);

        restore_indexes(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
            RestoreLayer::all(),
            RestoreMetrics::new(None, &prometheus::Registry::new()),
        )
        .unwrap()
        .shutdown()
        .await
        .unwrap();

        // Every object lands at its current version in `objects`.
        for o in &objects {
            assert_eq!(
                schema.get_object_by_key(o.id(), o.version()).unwrap(),
                Some(o.clone()),
            );
        }

        // The `objects` pipeline's __restore / __watermark /
        // __chain_id rows all match the source target.
        let key = PipelineTaskKey::new(Objects::NAME);
        let state = db.framework().restore.get(&key).unwrap().unwrap();
        match state.state.unwrap() {
            restore_state::State::Complete(c) => assert_eq!(c.restored_at, 123),
            other => panic!("expected Complete, got {other:?}"),
        }
        assert_eq!(
            db.framework().watermarks.get(&key).unwrap().unwrap(),
            Watermark::for_checkpoint(123),
        );
        assert_eq!(
            db.framework().chain_ids.get(&key).unwrap().unwrap(),
            chain_id,
        );
    }

    /// `seed_history_cohort` primes every history-cohort pipeline with
    /// the seed watermark and the chain id, stamps the pruning floor at
    /// the lowest available checkpoint `L`, and leaves the live cohort
    /// untouched (the restore driver owns it).
    #[test]
    fn seed_history_cohort_seeds_history_watermarks_and_pruning_floor() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let chain_id = ChainId([5u8; 32]);
        // Seed point: committed through checkpoint 999 / tx 5000, so the
        // backfill resumes at (and the floor is) checkpoint 1000.
        let seed = Watermark {
            checkpoint_hi_inclusive: 999,
            tx_hi: 5_000,
            ..Watermark::default()
        };
        seed_history_cohort(&db, &schema, seed, chain_id, None).unwrap();

        // History cohort resumes from the seed point, pinned to the
        // chain.
        for name in HISTORY_COHORT {
            let key = PipelineTaskKey::new(*name);
            assert_eq!(
                db.framework().watermarks.get(&key).unwrap(),
                Some(seed),
                "{name} should resume from the seed point",
            );
            assert_eq!(
                db.framework().chain_ids.get(&key).unwrap(),
                Some(chain_id),
                "{name} should pin the chain id",
            );
        }

        // Live cohort is the restore driver's responsibility — the
        // history seed must not touch it.
        for name in LIVE_COHORT {
            let key = PipelineTaskKey::new(*name);
            assert!(
                db.framework().watermarks.get(&key).unwrap().is_none(),
                "{name} watermark must be untouched by the history seed",
            );
        }

        // Pruning floor sits at the lowest available checkpoint
        // (seed + 1) and the seed point's tx count.
        assert_eq!(
            schema.get_pruning_watermarks().unwrap(),
            Some(crate::schema::pruning_watermark::Watermarks {
                tx_seq_lo: 5_000,
                checkpoint_lo: 1_000,
            }),
        );
    }

    /// The live and history cohorts are disjoint and each has the
    /// expected size. (Their union being exactly the embedded layer's
    /// enabled set is pinned by `embedded_registers_only_cohort_pipelines`.)
    #[test]
    fn cohorts_are_disjoint() {
        let live: std::collections::BTreeSet<_> = LIVE_COHORT.iter().collect();
        let history: std::collections::BTreeSet<_> = HISTORY_COHORT.iter().collect();
        assert!(live.is_disjoint(&history), "cohorts must not overlap");
        assert_eq!(live.len(), 3);
        assert_eq!(history.len(), 7);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint-pinned index resolution and pruning-floor checks for `StateService`.

use std::sync::Arc;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::storage::CheckpointIndexes;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::query_options::above_latest_available;
use crate::ledger_history::query_options::below_serving_floor;
use crate::reader::NotReadyCause;
use crate::reader::NotServedHereCause;
use crate::reader::index_empty;
use crate::reader::indexes_disabled;
use crate::reader::not_ready;
use crate::reader::not_served_here;

/// Why a checkpoint-pinned state read selected its checkpoint.
///
/// Determines the `bound` metadata on `ABOVE_LATEST_AVAILABLE` (`"at_checkpoint"` vs `"cursor"`)
/// and whether a checkpoint below the object pruning floor is `BELOW_EARLIEST_AVAILABLE` (caller
/// chose the checkpoint) or `NOT_READY` / `"pruning_race"` (server chose the newest checkpoint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PinSource {
    /// Explicit `at_checkpoint` field on the request.
    AtCheckpoint,
    /// Checkpoint carried in a cursor (`options.after`, `options.before`, or pinned `page_token`).
    Cursor,
    /// Server selected the highest servable checkpoint (e.g. `options` without `at_checkpoint` or
    /// cursor).
    Latest,
}

impl PinSource {
    fn bound_name(self) -> &'static str {
        match self {
            Self::AtCheckpoint => "at_checkpoint",
            Self::Cursor => "cursor",
            Self::Latest => "at_checkpoint",
        }
    }
}

/// An [`RpcIndexes`] handle pinned to one checkpoint snapshot, acquired once so the snapshot
/// cannot be evicted while the request executes.
pub(super) struct PinnedStateRead {
    pub(super) checkpoint: u64,
    pub(super) source: PinSource,
    pub(super) indexes: Arc<dyn RpcIndexes>,
}

impl PinnedStateRead {
    /// Refuse the read if `self.checkpoint` is below this node's object pruning floor.
    ///
    /// Called before object-loading reads (to fail fast) and after object loads (to catch pruning
    /// that advanced while the request ran).
    pub(super) fn check_object_floor(&self, reader: &dyn RpcStateReader) -> Result<()> {
        let object_floor = object_pruning_floor(reader)?;
        if self.checkpoint < object_floor {
            return Err(match self.source {
                PinSource::AtCheckpoint | PinSource::Cursor => {
                    let window_lo = reader
                        .retained_checkpoint_indexes_range()
                        .map_or(0, |range| *range.start());
                    below_serving_floor(window_lo.max(object_floor))
                }
                PinSource::Latest => pruning_race(format!(
                    "checkpoint {} fell below the object pruning floor ({object_floor})",
                    self.checkpoint
                )),
            });
        }
        Ok(())
    }

    /// Error for an indexed object version that could not be loaded during a checkpoint-pinned
    /// read. Never skips the entry: reports `BELOW_EARLIEST_AVAILABLE` when an explicit
    /// checkpoint/cursor is now below the object pruning floor, and `NOT_READY` (`pruning_race`)
    /// otherwise.
    pub(super) fn missing_object_error(
        &self,
        reader: &dyn RpcStateReader,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> RpcError {
        if let Err(err) = self.check_object_floor(reader) {
            return err;
        }
        pruning_race(format!(
            "indexed object {object_id} at version {} is missing for checkpoint {}",
            version.value(),
            self.checkpoint
        ))
    }
}

/// Resolved index target for a `StateService` call: either current live state or a snapshot
/// pinned to a specific checkpoint.
pub(super) enum StateReadTarget<'a> {
    Current(&'a dyn RpcIndexes),
    Pinned(PinnedStateRead),
}

impl StateReadTarget<'_> {
    pub(super) fn indexes(&self) -> &dyn RpcIndexes {
        match self {
            Self::Current(indexes) => *indexes,
            Self::Pinned(pinned) => pinned.indexes.as_ref(),
        }
    }

    pub(super) fn pinned(&self) -> Option<&PinnedStateRead> {
        match self {
            Self::Current(_) => None,
            Self::Pinned(pinned) => Some(pinned),
        }
    }
}

/// Lowest checkpoint whose live object versions remain intact in the object store.
///
/// Pruning through checkpoint `P` removes versions replaced at or before `P`, leaving input
/// objects of `P + 1` (the live state at the end of `P`) intact; `get_lowest_available_checkpoint_objects`
/// returns `P + 1` (or `0` when nothing is pruned).
pub(super) fn object_pruning_floor(reader: &dyn RpcStateReader) -> Result<u64> {
    Ok(reader
        .get_lowest_available_checkpoint_objects()?
        .saturating_sub(1))
}

pub(super) fn pruning_race(message: impl Into<String>) -> RpcError {
    not_ready(NotReadyCause::PruningRace, message)
}

/// Resolve an unpinned or checkpoint-pinned index reader for a `StateService` call.
///
/// Always checks that rpc indexes are enabled first (`NOT_SERVED_HERE` / `no_rpc_indexes`). When
/// `pin` is `true`, acquires a snapshot handle once and maps any missing handle to the proto's
/// refusal codes; never falls back to current state.
pub(super) fn resolve_state_read(
    service: &RpcService,
    checkpoint: Option<u64>,
    pin: bool,
    source: PinSource,
    loads_objects: bool,
) -> Result<StateReadTarget<'_>> {
    let reader = service.reader.inner();
    let current = reader.indexes().ok_or_else(indexes_disabled)?;
    if !pin {
        return Ok(StateReadTarget::Current(current));
    }
    resolve_pinned(reader.as_ref(), checkpoint, source, loads_objects).map(StateReadTarget::Pinned)
}

/// Acquire a checkpoint-pinned index reader once from `reader`.
pub(super) fn resolve_pinned(
    reader: &dyn RpcStateReader,
    checkpoint: Option<u64>,
    source: PinSource,
    loads_objects: bool,
) -> Result<PinnedStateRead> {
    reader.indexes().ok_or_else(indexes_disabled)?;

    let lookup = match checkpoint {
        Some(k) => reader.indexes_at_checkpoint(k),
        None => reader.latest_checkpoint_indexes(),
    };

    match lookup {
        CheckpointIndexes::Disabled => Err(not_served_here(
            NotServedHereCause::DisabledByConfig,
            "checkpoint-pinned state reads are disabled on this server",
        )),
        CheckpointIndexes::Empty => Err(index_empty()),
        CheckpointIndexes::Unavailable { retained_range } => {
            let k = checkpoint.expect("Unavailable is only returned for a specific checkpoint");
            let window_lo = *retained_range.start();
            let hi = *retained_range.end();
            if k > hi {
                return Err(above_latest_available(
                    source.bound_name(),
                    k,
                    hi.saturating_add(1),
                ));
            }
            let effective_lo = if loads_objects {
                window_lo.max(object_pruning_floor(reader)?)
            } else {
                window_lo
            };
            if k < effective_lo {
                return Err(below_serving_floor(effective_lo));
            }
            Err(not_ready(
                NotReadyCause::CheckpointUnavailable,
                format!(
                    "checkpoint {k} is within the retained checkpoint range \
                     ({window_lo}..={hi}) but is not yet servable on this replica"
                ),
            ))
        }
        CheckpointIndexes::Available {
            checkpoint: resolved_checkpoint,
            indexes,
        } => {
            let pinned = PinnedStateRead {
                checkpoint: resolved_checkpoint,
                source,
                indexes,
            };
            if loads_objects {
                pinned.check_object_floor(reader)?;
            }
            Ok(pinned)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_consistent_store::Watermark;
    use sui_rpc_store::RpcStoreReader;
    use sui_rpc_store::RpcStoreSchema;
    use sui_types::base_types::EpochId;
    use sui_types::committee::Committee;
    use sui_types::digests::ChainIdentifier;
    use sui_types::digests::CheckpointContentsDigest;
    use sui_types::digests::CheckpointDigest;
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEvents;
    use sui_types::error::SuiResult;
    use sui_types::messages_checkpoint::CheckpointContents;
    use sui_types::messages_checkpoint::CheckpointSequenceNumber;
    use sui_types::messages_checkpoint::VerifiedCheckpoint;
    use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
    use sui_types::object::Object;
    use sui_types::storage::BackingPackageStore;
    use sui_types::storage::ObjectStore;
    use sui_types::storage::PackageObject;
    use sui_types::storage::ReadStore;
    use sui_types::storage::RuntimeObjectResolver;
    use sui_types::storage::error::Result as StorageResult;
    use sui_types::transaction::VerifiedTransaction;

    use super::*;
    use crate::ErrorReason;
    use crate::ledger_history::query_options::assert_above_latest_available_error;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::assert_not_served_here;
    use crate::reader::assert_refusal;
    use crate::stub_store::StubStore;

    struct TestSnapshotReader {
        store: RpcStoreReader,
        lowest_available_objects: AtomicU64,
    }

    impl TestSnapshotReader {
        fn open(snapshot_capacity: usize) -> (tempfile::TempDir, Db, Self) {
            let dir = tempfile::tempdir().unwrap();
            let (db, schema) = Db::open::<RpcStoreSchema>(
                dir.path(),
                DbOptions {
                    snapshot_capacity,
                    ..DbOptions::default()
                },
            )
            .unwrap();
            let store = RpcStoreReader::new(db.clone(), Arc::new(schema));
            (
                dir,
                db,
                Self {
                    store,
                    lowest_available_objects: AtomicU64::new(0),
                },
            )
        }
    }

    impl ObjectStore for TestSnapshotReader {
        fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
            self.store.get_object(object_id)
        }

        fn get_object_by_key(
            &self,
            object_id: &ObjectID,
            version: SequenceNumber,
        ) -> Option<Object> {
            self.store.get_object_by_key(object_id, version)
        }
    }

    impl BackingPackageStore for TestSnapshotReader {
        fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
            self.store.get_package_object(package_id)
        }
    }

    impl RuntimeObjectResolver for TestSnapshotReader {
        fn read_child_object(
            &self,
            parent: &ObjectID,
            child: &ObjectID,
            child_version_upper_bound: SequenceNumber,
        ) -> SuiResult<Option<Object>> {
            self.store
                .read_child_object(parent, child, child_version_upper_bound)
        }

        fn get_object_received_at_version(
            &self,
            owner: &ObjectID,
            receiving_object_id: &ObjectID,
            receive_object_at_version: SequenceNumber,
            epoch_id: EpochId,
        ) -> SuiResult<Option<Object>> {
            self.store.get_object_received_at_version(
                owner,
                receiving_object_id,
                receive_object_at_version,
                epoch_id,
            )
        }
    }

    impl ReadStore for TestSnapshotReader {
        fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
            self.store.get_committee(epoch)
        }

        fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            self.store.get_latest_checkpoint()
        }

        fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            self.store.get_highest_verified_checkpoint()
        }

        fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            self.store.get_highest_synced_checkpoint()
        }

        fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
            self.store.get_lowest_available_checkpoint()
        }

        fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
            self.store.get_checkpoint_by_digest(digest)
        }

        fn get_checkpoint_by_sequence_number(
            &self,
            sequence_number: CheckpointSequenceNumber,
        ) -> Option<VerifiedCheckpoint> {
            self.store
                .get_checkpoint_by_sequence_number(sequence_number)
        }

        fn get_checkpoint_contents_by_digest(
            &self,
            digest: &CheckpointContentsDigest,
        ) -> Option<CheckpointContents> {
            self.store.get_checkpoint_contents_by_digest(digest)
        }

        fn get_checkpoint_contents_by_sequence_number(
            &self,
            sequence_number: CheckpointSequenceNumber,
        ) -> Option<CheckpointContents> {
            self.store
                .get_checkpoint_contents_by_sequence_number(sequence_number)
        }

        fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
            self.store.get_transaction(tx_digest)
        }

        fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
            self.store.get_transaction_effects(tx_digest)
        }

        fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
            self.store.get_events(event_digest)
        }

        fn get_full_checkpoint_contents(
            &self,
            sequence_number: Option<CheckpointSequenceNumber>,
            digest: &CheckpointContentsDigest,
        ) -> Option<VersionedFullCheckpointContents> {
            self.store
                .get_full_checkpoint_contents(sequence_number, digest)
        }

        fn get_unchanged_loaded_runtime_objects(
            &self,
            digest: &TransactionDigest,
        ) -> Option<Vec<sui_types::storage::ObjectKey>> {
            self.store.get_unchanged_loaded_runtime_objects(digest)
        }

        fn get_transaction_checkpoint(
            &self,
            digest: &TransactionDigest,
        ) -> Option<CheckpointSequenceNumber> {
            self.store.get_transaction_checkpoint(digest)
        }
    }

    impl RpcStateReader for TestSnapshotReader {
        fn get_lowest_available_checkpoint_objects(
            &self,
        ) -> StorageResult<CheckpointSequenceNumber> {
            Ok(self.lowest_available_objects.load(Ordering::SeqCst))
        }

        fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
            self.store.get_chain_identifier()
        }

        fn indexes(&self) -> Option<&dyn RpcIndexes> {
            self.store.indexes()
        }

        fn indexes_at_checkpoint(
            &self,
            checkpoint: CheckpointSequenceNumber,
        ) -> CheckpointIndexes {
            self.store.indexes_at_checkpoint(checkpoint)
        }

        fn latest_checkpoint_indexes(&self) -> CheckpointIndexes {
            self.store.latest_checkpoint_indexes()
        }

        fn retained_checkpoint_indexes_range(
            &self,
        ) -> Option<std::ops::RangeInclusive<CheckpointSequenceNumber>> {
            self.store.retained_checkpoint_indexes_range()
        }
    }

    #[test]
    fn no_indexes_and_disabled_window_return_not_served_here() {
        let no_indexes = StubStore::new();
        assert_not_served_here(
            resolve_pinned(&no_indexes, Some(5), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            NotServedHereCause::NoRpcIndexes,
        );

        let (_dir, _db, disabled) = TestSnapshotReader::open(0);
        assert_not_served_here(
            resolve_pinned(&disabled, Some(5), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            NotServedHereCause::DisabledByConfig,
        );
        assert_not_served_here(
            resolve_pinned(&disabled, None, PinSource::Latest, false)
                .err()
                .unwrap(),
            NotServedHereCause::DisabledByConfig,
        );
    }

    #[test]
    fn empty_window_returns_index_empty() {
        let (_dir, _db, reader) = TestSnapshotReader::open(5);
        assert_refusal(
            resolve_pinned(&reader, Some(5), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "index_empty",
        );
        assert_refusal(
            resolve_pinned(&reader, None, PinSource::Latest, true)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "index_empty",
        );
    }

    #[test]
    fn range_refusals_distinguish_below_gap_and_above() {
        // Capacity 5, replica restarted and only took snapshots 10 and 11.
        // Configured retained window is 7..=11.
        let (_dir, db, reader) = TestSnapshotReader::open(5);
        db.take_snapshot(Watermark::for_checkpoint(10));
        db.take_snapshot(Watermark::for_checkpoint(11));

        // Below configured window (6 < 7) -> BELOW_EARLIEST_AVAILABLE (floor 7).
        assert_below_serving_floor_error(
            resolve_pinned(&reader, Some(6), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            7,
        );

        // Inside configured window (8 in 7..=11) but not on this replica -> checkpoint_unavailable.
        assert_refusal(
            resolve_pinned(&reader, Some(8), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "checkpoint_unavailable",
        );

        // Above latest snapshot (12 > 11) -> ABOVE_LATEST_AVAILABLE with bound matching source.
        assert_above_latest_available_error(
            resolve_pinned(&reader, Some(12), PinSource::AtCheckpoint, false)
                .err()
                .unwrap(),
            "at_checkpoint",
            12,
            12,
        );
        assert_above_latest_available_error(
            resolve_pinned(&reader, Some(15), PinSource::Cursor, false)
                .err()
                .unwrap(),
            "cursor",
            15,
            12,
        );

        // Held checkpoint succeeds, and Latest selects 11.
        assert_eq!(
            resolve_pinned(&reader, Some(10), PinSource::AtCheckpoint, false)
                .unwrap()
                .checkpoint,
            10
        );
        assert_eq!(
            resolve_pinned(&reader, None, PinSource::Latest, false)
                .unwrap()
                .checkpoint,
            11
        );
    }

    #[test]
    fn object_pruning_floor_applies_only_to_object_loading_methods() {
        // Configured retained window is 7..=11, snapshots 10 and 11 held.
        // Object pruning floor = 11 - 1 = 10 (lowest_available_objects = 11 -> intact floor 10).
        let (_dir, db, reader) = TestSnapshotReader::open(5);
        db.take_snapshot(Watermark::for_checkpoint(9));
        db.take_snapshot(Watermark::for_checkpoint(10));
        db.take_snapshot(Watermark::for_checkpoint(11));
        reader.lowest_available_objects.store(11, Ordering::SeqCst);

        // Balance read (loads_objects = false) at 9 succeeds.
        assert_eq!(
            resolve_pinned(&reader, Some(9), PinSource::AtCheckpoint, false)
                .unwrap()
                .checkpoint,
            9
        );

        // Object-loading read at 9 fails BELOW_EARLIEST_AVAILABLE with floor 10.
        assert_below_serving_floor_error(
            resolve_pinned(&reader, Some(9), PinSource::AtCheckpoint, true)
                .err()
                .unwrap(),
            10,
        );

        // Unheld checkpoint 8 inside snapshot window (7..=11) but below object floor (10)
        // is permanently pruned (BELOW_EARLIEST_AVAILABLE), not checkpoint_unavailable.
        assert_below_serving_floor_error(
            resolve_pinned(&reader, Some(8), PinSource::Cursor, true)
                .err()
                .unwrap(),
            10,
        );

        // Server-selected Latest that falls below object floor is a pruning_race.
        reader.lowest_available_objects.store(13, Ordering::SeqCst);
        assert_refusal(
            resolve_pinned(&reader, None, PinSource::Latest, true)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );

        // Missing object version on an intact checkpoint is a pruning_race; once pruning advances
        // past an explicit checkpoint, it becomes BELOW_EARLIEST_AVAILABLE.
        reader.lowest_available_objects.store(0, Ordering::SeqCst);
        let pinned = resolve_pinned(&reader, Some(10), PinSource::AtCheckpoint, true).unwrap();
        assert_refusal(
            pinned.missing_object_error(&reader, ObjectID::ZERO, SequenceNumber::from_u64(1)),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );
        reader.lowest_available_objects.store(12, Ordering::SeqCst);
        assert_below_serving_floor_error(
            pinned.missing_object_error(&reader, ObjectID::ZERO, SequenceNumber::from_u64(1)),
            11,
        );
    }
}

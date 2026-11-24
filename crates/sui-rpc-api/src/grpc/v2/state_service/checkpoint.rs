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
use crate::ledger_history::query_options::above_latest_available;
use crate::ledger_history::query_options::below_serving_floor;
use crate::reader::NotReadyCause;
use crate::reader::NotServedHereCause;
use crate::reader::index_empty;
use crate::reader::indexes_disabled;
use crate::reader::not_ready;
use crate::reader::not_served_here;
use crate::reader::version_not_retained;

/// Which checkpoint a checkpoint-pinned state read targets, and why.
///
/// Determines the `bound` metadata on `ABOVE_LATEST_AVAILABLE` (`"at_checkpoint"` vs `"cursor"`)
/// and whether a checkpoint below the object pruning floor is `BELOW_EARLIEST_AVAILABLE` (caller
/// chose the checkpoint) or `NOT_READY` / `"pruning_race"` (server chose the newest checkpoint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PinSource {
    /// Explicit `at_checkpoint` field on the request.
    AtCheckpoint(u64),
    /// Checkpoint carried in a cursor (`options.after`, `options.before`, or pinned `page_token`).
    Cursor(u64),
    /// Server selects the highest servable checkpoint (e.g. `options` without `at_checkpoint` or
    /// cursor).
    Latest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinOrigin {
    Explicit,
    Latest,
}

/// An [`RpcIndexes`] handle pinned to one checkpoint snapshot, acquired once so the snapshot
/// cannot be evicted while the request executes.
pub(super) struct PinnedStateRead {
    pub(super) checkpoint: u64,
    origin: PinOrigin,
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
            return Err(match self.origin {
                PinOrigin::Explicit => {
                    let window_lo = reader
                        .retained_checkpoint_indexes_range()
                        .map_or(0, |range| *range.start());
                    below_serving_floor(window_lo.max(object_floor))
                }
                PinOrigin::Latest => pruning_race(format!(
                    "checkpoint {} fell below the object pruning floor ({object_floor})",
                    self.checkpoint
                )),
            });
        }
        Ok(())
    }

    /// Error for an indexed object version that could not be loaded during a checkpoint-pinned
    /// read. Never skips the entry: reports `VERSION_NOT_RETAINED` when the store does not keep
    /// replaced versions, `BELOW_EARLIEST_AVAILABLE` when an explicit checkpoint/cursor is now
    /// below the object pruning floor, and `NOT_READY` (`pruning_race`) otherwise.
    pub(super) fn missing_object_error(
        &self,
        reader: &dyn RpcStateReader,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> RpcError {
        if !reader.retains_object_version_history() {
            return version_not_retained();
        }
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

/// Acquire a checkpoint-pinned index reader once from `reader`.
pub(super) fn resolve_pinned(
    reader: &dyn RpcStateReader,
    source: PinSource,
    loads_objects: bool,
) -> Result<PinnedStateRead> {
    reader.indexes().ok_or_else(indexes_disabled)?;

    let (requested, origin, lookup) = match source {
        PinSource::AtCheckpoint(k) => (
            Some((k, "at_checkpoint")),
            PinOrigin::Explicit,
            reader.indexes_at_checkpoint(k),
        ),
        PinSource::Cursor(k) => (
            Some((k, "cursor")),
            PinOrigin::Explicit,
            reader.indexes_at_checkpoint(k),
        ),
        PinSource::Latest => (None, PinOrigin::Latest, reader.latest_checkpoint_indexes()),
    };
    match lookup {
        CheckpointIndexes::Disabled => Err(not_served_here(
            NotServedHereCause::DisabledByConfig,
            "checkpoint-pinned state reads are disabled on this server",
        )),
        CheckpointIndexes::Empty => Err(index_empty()),
        CheckpointIndexes::Unavailable { retained_range } => {
            let Some((k, bound_name)) = requested else {
                return Err(index_empty());
            };
            let window_lo = *retained_range.start();
            let hi = *retained_range.end();
            if k > hi {
                return Err(above_latest_available(bound_name, k, hi.saturating_add(1)));
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
            checkpoint,
            indexes,
        } => {
            let pinned = PinnedStateRead {
                checkpoint,
                origin,
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
    use super::*;
    use crate::ErrorReason;
    use crate::grpc::v2::state_service::test_support::TestStateStore;
    use crate::ledger_history::query_options::assert_above_latest_available_error;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::assert_not_served_here;
    use crate::reader::assert_refusal;
    use crate::stub_store::StubStore;

    #[test]
    fn no_indexes_and_disabled_window_return_not_served_here() {
        let no_indexes = StubStore::new();
        assert_not_served_here(
            resolve_pinned(&no_indexes, PinSource::AtCheckpoint(5), false)
                .err()
                .unwrap(),
            NotServedHereCause::NoRpcIndexes,
        );

        let (_dir, disabled, _service) = TestStateStore::open(0);
        assert_not_served_here(
            resolve_pinned(disabled.as_ref(), PinSource::AtCheckpoint(5), false)
                .err()
                .unwrap(),
            NotServedHereCause::DisabledByConfig,
        );
        assert_not_served_here(
            resolve_pinned(disabled.as_ref(), PinSource::Latest, false)
                .err()
                .unwrap(),
            NotServedHereCause::DisabledByConfig,
        );
    }

    #[test]
    fn empty_window_returns_index_empty() {
        let (_dir, reader, _service) = TestStateStore::open(5);
        assert_refusal(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(5), false)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "index_empty",
        );
        assert_refusal(
            resolve_pinned(reader.as_ref(), PinSource::Latest, true)
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
        let (_dir, reader, _service) = TestStateStore::open(5);
        reader.take_snapshot(10);
        reader.take_snapshot(11);

        // Below configured window (6 < 7) -> BELOW_EARLIEST_AVAILABLE (floor 7).
        assert_below_serving_floor_error(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(6), false)
                .err()
                .unwrap(),
            7,
        );

        // Inside configured window (8 in 7..=11) but not on this replica -> checkpoint_unavailable.
        assert_refusal(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(8), false)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "checkpoint_unavailable",
        );

        // Above latest snapshot (12 > 11) -> ABOVE_LATEST_AVAILABLE with bound matching source.
        assert_above_latest_available_error(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(12), false)
                .err()
                .unwrap(),
            "at_checkpoint",
            12,
            12,
        );
        assert_above_latest_available_error(
            resolve_pinned(reader.as_ref(), PinSource::Cursor(15), false)
                .err()
                .unwrap(),
            "cursor",
            15,
            12,
        );

        // Held checkpoint succeeds, and Latest selects 11.
        assert_eq!(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(10), false)
                .unwrap()
                .checkpoint,
            10
        );
        assert_eq!(
            resolve_pinned(reader.as_ref(), PinSource::Latest, false)
                .unwrap()
                .checkpoint,
            11
        );
    }

    #[test]
    fn object_pruning_floor_applies_only_to_object_loading_methods() {
        // Configured retained window is 7..=11, snapshots 9, 10 and 11 held.
        // Object pruning floor = 11 - 1 = 10 (lowest_available_objects = 11 -> intact floor 10).
        let (_dir, reader, _service) = TestStateStore::open(5);
        reader.take_snapshot(9);
        reader.take_snapshot(10);
        reader.take_snapshot(11);
        reader.set_lowest_available_objects(11);

        // Balance read (loads_objects = false) at 9 succeeds.
        assert_eq!(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(9), false)
                .unwrap()
                .checkpoint,
            9
        );

        // Object-loading read at 9 fails BELOW_EARLIEST_AVAILABLE with floor 10.
        assert_below_serving_floor_error(
            resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(9), true)
                .err()
                .unwrap(),
            10,
        );

        // Unheld checkpoint 8 inside snapshot window (7..=11) but below object floor (10)
        // is permanently pruned (BELOW_EARLIEST_AVAILABLE), not checkpoint_unavailable.
        assert_below_serving_floor_error(
            resolve_pinned(reader.as_ref(), PinSource::Cursor(8), true)
                .err()
                .unwrap(),
            10,
        );

        // Server-selected Latest that falls below object floor is a pruning_race.
        reader.set_lowest_available_objects(13);
        assert_refusal(
            resolve_pinned(reader.as_ref(), PinSource::Latest, true)
                .err()
                .unwrap(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );

        // Missing object version on an intact checkpoint is a pruning_race; once pruning advances
        // past an explicit checkpoint, it becomes BELOW_EARLIEST_AVAILABLE.
        reader.set_lowest_available_objects(0);
        let pinned = resolve_pinned(reader.as_ref(), PinSource::AtCheckpoint(10), true).unwrap();
        assert_refusal(
            pinned.missing_object_error(
                reader.as_ref(),
                ObjectID::ZERO,
                SequenceNumber::from_u64(1),
            ),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );
        reader.set_lowest_available_objects(12);
        assert_below_serving_floor_error(
            pinned.missing_object_error(
                reader.as_ref(),
                ObjectID::ZERO,
                SequenceNumber::from_u64(1),
            ),
            11,
        );
    }
}

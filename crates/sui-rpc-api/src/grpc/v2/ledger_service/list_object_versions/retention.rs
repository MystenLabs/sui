// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::ledger_history::object_versions::ObjectVersionsQuery;
use crate::reader::AtCheckpointUpperBound;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::storage::ObjectOrTombstone;
use tokio_util::sync::CancellationToken;

use super::super::chunked_scan::cancelled;
use super::IndexedTip;
use super::stored_version;
use crate::reader::StateReader;

/// The listed object's retention floor, as of the last read of the lower bound.
#[derive(Clone, Copy, Debug)]
pub(super) struct Retention {
    /// See [`Floors::floor_checkpoint`].
    pub(super) floor_checkpoint: Option<u64>,
    /// Whether the listing reports each removal's transaction, which raises the floor.
    lists_transactions: bool,
    /// What the object keeps of its history above `floor_checkpoint`.
    kept: KeptHistory,
}

/// One read of the floors.
#[derive(Clone, Copy, Debug)]
pub(super) struct Floors {
    /// The object floor ([`crate::reader::AtCheckpointLowerBound::pruned_floor`]), or `None`
    /// when no object state is pruned.
    pub(super) object_floor: Option<u64>,
    /// The listing's floor checkpoint, or `None` when nothing is pruned: the object floor,
    /// raised, when the listing reports removal transactions, to one below the lowest
    /// checkpoint with transaction data. The two differ only on nodes that keep every object
    /// version and prune checkpoints, whose removals at or below the higher floor may have no
    /// effects left to find their transactions by.
    pub(super) floor_checkpoint: Option<u64>,
}

/// Reads the [`Floors`].
pub(super) fn read_floor(reader: &StateReader, lists_transactions: bool) -> Result<Floors> {
    let object_floor = reader.at_checkpoint_lower_bound()?.pruned_floor();
    // The store's own bound: the object floor already covers the object bound that
    // `StateReader::get_lowest_available_checkpoint` also takes.
    let lowest_with_transactions = if lists_transactions {
        Some(reader.inner().get_lowest_available_checkpoint()?)
    } else {
        None
    };
    Ok(Floors {
        object_floor,
        floor_checkpoint: listing_floor(object_floor, lowest_with_transactions),
    })
}

/// [`Floors::floor_checkpoint`], given the object floor and, for a listing that reports removal
/// transactions, the lowest checkpoint with transaction data: the checkpoint before that one,
/// when higher.
fn listing_floor(object_floor: Option<u64>, lowest_with_transactions: Option<u64>) -> Option<u64> {
    object_floor.max(lowest_with_transactions.and_then(|lowest| lowest.checked_sub(1)))
}

/// The version of the tombstone the object store holds for the removal the version index records
/// at `indexed`, or `None` if the item there is a live version: the first stored item at or
/// above `indexed` (see [`super::stored_version_at`]).
fn removal_stored_at(
    reader: &StateReader,
    object_id: ObjectID,
    indexed: SequenceNumber,
) -> Result<Option<SequenceNumber>> {
    let first_stored = reader
        .object_versions_iter(object_id, indexed..=SequenceNumber::MAX, false)?
        .next()
        .transpose()?;
    Ok(match first_stored {
        Some(ObjectOrTombstone::Tombstone((_, version, _))) => Some(version),
        _ => None,
    })
}

/// The object's first item, live or a removal, above `version` and at or below the indexed tip.
pub(super) fn first_item_after(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    tip: &IndexedTip,
) -> Result<Option<ObjectOrTombstone>> {
    let after = SequenceNumber::from_u64(version.value().saturating_add(1));
    if after > tip.newest_version {
        return Ok(None);
    }
    reader
        .object_versions_iter(object_id, after..=tip.newest_version, false)?
        .next()
        .transpose()
}

/// What an object keeps of its history up to the indexed tip.
#[derive(Clone, Copy, Debug)]
pub(super) enum KeptHistory {
    /// Its whole history: nothing is pruned.
    Whole,
    /// The items from its oldest retained version on: its version at the end of the floor
    /// checkpoint, or for an object with none, the first item after it, live or a removal, that
    /// this node places there (see [`first_retained_version_after_removal`]).
    From(SequenceNumber),
    /// Nothing: pruning removed every item of the object up to the indexed tip, which happens
    /// when it raises the floor past the object's last removal while the object is listed.
    Nothing,
}

impl Retention {
    /// The floor, and what the object keeps above it, starting from `floor_checkpoint`, the
    /// listing's floor checkpoint as its caller last read it (see [`read_floor`]). Pruning can
    /// raise the floor while the object's history is read, and a read below the new floor may
    /// find the object's rows retracted, so the floor is read again after it, until it has not
    /// moved. Fails with `CANCELLED` once `cancel` fires.
    pub(super) fn read(
        reader: &StateReader,
        object_id: ObjectID,
        tip: &IndexedTip,
        mut floor_checkpoint: Option<u64>,
        lists_transactions: bool,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        loop {
            let kept = match floor_checkpoint {
                Some(floor_checkpoint) => {
                    // Pruning never passes the indexed tip, so the floor is within the upper
                    // bound as of now. The planning tip's bound serves while the floor is at or
                    // below it; pruning can raise the floor past it during a long listing.
                    let upper_bound = if floor_checkpoint <= tip.upper_bound.highest_indexed() {
                        tip.upper_bound
                    } else {
                        reader.at_checkpoint_upper_bound()?
                    };
                    let at_floor = reader.get_object_version_at_checkpoint(
                        object_id,
                        floor_checkpoint,
                        &upper_bound,
                    )?;
                    match at_floor {
                        Some(version) => match lists_transactions
                            .then(|| removal_stored_at(reader, object_id, version))
                            .transpose()?
                            .flatten()
                        {
                            // Above the object floor the index still holds a removal at or before
                            // the floor checkpoint, whose transaction a listing that reports
                            // transactions cannot find. Every item above it was written after the
                            // floor checkpoint, so the first is the oldest retained one.
                            Some(removed) => {
                                match first_item_after(reader, object_id, removed, tip)? {
                                    Some(item) => KeptHistory::From(stored_version(&item)),
                                    None => KeptHistory::Nothing,
                                }
                            }
                            None => KeptHistory::From(version),
                        },
                        None => first_retained_version_after_removal(
                            reader,
                            object_id,
                            floor_checkpoint,
                            tip,
                            cancel,
                        )?,
                    }
                }
                None => KeptHistory::Whole,
            };
            let floor_after = read_floor(reader, lists_transactions)?.floor_checkpoint;
            if floor_after == floor_checkpoint {
                return Ok(Self {
                    floor_checkpoint,
                    lists_transactions,
                    kept,
                });
            }
            floor_checkpoint = floor_after;
        }
    }

    /// The floor again, after a read of the object's versions: this retention when the floor has
    /// not moved, or else read anew (see [`Self::read`]).
    pub(super) fn reread(
        self,
        reader: &StateReader,
        object_id: ObjectID,
        tip: &IndexedTip,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let floor_checkpoint = read_floor(reader, self.lists_transactions)?.floor_checkpoint;
        if floor_checkpoint == self.floor_checkpoint {
            Ok(self)
        } else {
            Self::read(
                reader,
                object_id,
                tip,
                floor_checkpoint,
                self.lists_transactions,
                cancel,
            )
        }
    }

    /// The first version at or above `version` that the object keeps: `version`, or its oldest
    /// retained version when higher. `None` when it keeps nothing.
    pub(super) fn first_kept_from(&self, version: SequenceNumber) -> Option<SequenceNumber> {
        match self.kept {
            KeptHistory::Whole => Some(version),
            KeptHistory::From(oldest_retained) => Some(version.max(oldest_retained)),
            KeptHistory::Nothing => None,
        }
    }

    /// Whether the object keeps `version` and everything above it. Pruning raises the floor no
    /// later than it deletes versions, and deletes none at or above the oldest retained one, so
    /// a read of the versions down to `version` followed by this floor read missed nothing when
    /// this holds.
    pub(super) fn keeps_from(&self, version: SequenceNumber) -> bool {
        self.first_kept_from(version) == Some(version)
    }

    /// The lowest `start_checkpoint` the listing accepts under this floor, the checkpoint after
    /// it, or `None` when nothing is pruned.
    pub(super) fn lowest_start_checkpoint(&self) -> Option<u64> {
        self.floor_checkpoint
            .map(|floor_checkpoint| floor_checkpoint.saturating_add(1))
    }

    /// Fails with `query`'s [`ObjectVersionsQuery::floor_error`] for this floor, unless the
    /// object keeps `version` and everything above it (see [`Self::keeps_from`]): a listing
    /// whose client holds the history right below `version`, or that lists a checkpoint range
    /// from `start_checkpoint`, would otherwise skip the versions pruning deleted.
    pub(super) fn check_keeps_from(
        &self,
        version: SequenceNumber,
        query: &ObjectVersionsQuery,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<()> {
        match self.floor_checkpoint {
            Some(floor_checkpoint) if !self.keeps_from(version) => {
                Err(query.floor_error(floor_checkpoint, upper_bound))
            }
            _ => Ok(()),
        }
    }
}

/// For an object with no version at the end of `floor_checkpoint`: the first item written after
/// that checkpoint that this node places there, live or a removal, or nothing when it has no
/// such item, because pruning removed every item up to the indexed tip. Fails with `CANCELLED`
/// once `cancel` fires.
///
/// The way this is decided is complicated only for want of an index from an object version to
/// its transaction; see the notice in [`super::removals`].
///
/// Such an object was created after the floor checkpoint, or removed at or before it and then
/// unwrapped, or created again under the same ID, after it. Its stored items are read oldest
/// first. Those written at or before the floor checkpoint, which pruning has not deleted yet (or,
/// for tombstones, never deletes while `killswitch_tombstone_pruning` is set), are passed over.
/// Whether an item was written after the floor checkpoint is decided without reading its
/// transaction's effects, except for a removal below the version the first row records:
/// - A live version, through the rpc indexes: the sequence number of the transaction that
///   wrote it, and that transaction's checkpoint. The indexes prune both in the same write that
///   raises the floor, so a version they can no longer place was written at or before the floor
///   checkpoint. Most objects with no version at the floor checkpoint were created after it,
///   and their first stored item settles this with no index search.
/// - A removal, through the version index. Pruning dropped every row of the removed object, so
///   its first row records its version at the end of the first checkpoint after the floor that
///   wrote an item of it (see [`StateReader::first_checkpoint_recording_above`]), and the first
///   item written after the floor checkpoint is at or below that version. The removal that row
///   records was written after the floor checkpoint. One below it was too when a transaction of
///   that first checkpoint removed the object at its version, which a walk of the object's
///   transactions in that checkpoint finds; otherwise it was written at or before the floor
///   checkpoint. A removal written after the floor checkpoint is the floor.
pub(super) fn first_retained_version_after_removal(
    reader: &StateReader,
    object_id: ObjectID,
    floor_checkpoint: u64,
    tip: &IndexedTip,
    cancel: &CancellationToken,
) -> Result<KeptHistory> {
    // The first checkpoint after the floor that the version index records, and the stored
    // version of what it records there, once a removal needs them.
    let mut first_recorded: Option<(u64, SequenceNumber)> = None;
    // The versions at which the transactions of that checkpoint removed the object, once a
    // removal below the version it records needs them.
    let mut removed_in_first_recorded: Option<Vec<SequenceNumber>> = None;
    // The first item is almost always the answer, and the iterator decodes only the items taken.
    for item in
        reader.object_versions_iter(object_id, SequenceNumber::MIN..=tip.newest_version, false)?
    {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let item = item?;
        if first_recorded.is_some_and(|(_, recorded)| recorded < stored_version(&item)) {
            break;
        }
        match item {
            ObjectOrTombstone::Object(object) => {
                let transaction = match reader.transaction_sequence(&object.previous_transaction)? {
                    Some(sequence) => reader.ledger_transaction(sequence)?,
                    None => None,
                };
                if transaction
                    .is_some_and(|transaction| transaction.checkpoint_number > floor_checkpoint)
                {
                    return Ok(KeptHistory::From(object.version()));
                }
            }
            ObjectOrTombstone::Tombstone((_, version, _)) => {
                let (checkpoint, recorded) = match first_recorded {
                    Some(first_recorded) => first_recorded,
                    None => {
                        let indexed = reader.first_checkpoint_recording_above(
                            object_id,
                            None,
                            floor_checkpoint.saturating_add(1),
                            tip,
                        )?;
                        let Some((checkpoint, indexed)) = indexed else {
                            return Ok(KeptHistory::Nothing);
                        };
                        let recorded =
                            removal_stored_at(reader, object_id, indexed)?.unwrap_or(indexed);
                        *first_recorded.insert((checkpoint, recorded))
                    }
                };
                if recorded < version {
                    break;
                }
                if version == recorded {
                    return Ok(KeptHistory::From(version));
                }
                let removed = match &removed_in_first_recorded {
                    Some(removed) => removed,
                    None => removed_in_first_recorded.insert(removals_in_checkpoint(
                        reader, object_id, checkpoint, cancel,
                    )?),
                };
                if removed.contains(&version) {
                    return Ok(KeptHistory::From(version));
                }
            }
        }
    }
    // No item is placed after the floor checkpoint, or the item the first row records is not
    // stored: pruning has raised the floor since it was read.
    Ok(KeptHistory::Nothing)
}

/// The versions at which `object_id`'s transactions in `checkpoint` removed it, walking the
/// transaction bitmap back from the checkpoint's last transaction. Stops early at a transaction
/// whose rpc-index row or effects are pruned. A node that prunes checkpoints sooner than objects
/// can have pruned the effects of `checkpoint` while the objects after the floor are retained;
/// a removal it then misses is treated as written at or before the floor, so the object is
/// listed from a later item. Taking such a removal as retained instead could start the listing
/// at a leftover written at or before the floor, followed by pruned items, and list history with
/// a gap in it.
fn removals_in_checkpoint(
    reader: &StateReader,
    object_id: ObjectID,
    checkpoint: u64,
    cancel: &CancellationToken,
) -> Result<Vec<SequenceNumber>> {
    let mut removed = Vec::new();
    let mut before = reader.transactions_end(checkpoint)?;
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let Some(sequence) = reader.last_object_transaction(object_id, before)? else {
            return Ok(removed);
        };
        let Some(transaction) = reader.ledger_transaction(sequence)? else {
            return Ok(removed);
        };
        if transaction.checkpoint_number < checkpoint {
            return Ok(removed);
        }
        match reader.removal_version(&transaction.digest, object_id) {
            Some(version) => removed.extend(version),
            None => return Ok(removed),
        }
        before = sequence;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing that reports removal transactions starts from the state at the end of the
    /// checkpoint before the lowest one with transaction data, when that is above the object
    /// floor; otherwise, and for every other listing, at the end of the object floor.
    #[test]
    fn listing_floor_is_the_checkpoint_before_transaction_data_when_higher() {
        for (object_floor, lowest_with_transactions, expected) in [
            (Some(3), Some(10), Some(9)),
            (Some(3), None, Some(3)),
            (Some(12), Some(10), Some(12)),
            (None, Some(10), Some(9)),
            (None, Some(0), None),
            (None, None, None),
        ] {
            assert_eq!(
                listing_floor(object_floor, lowest_with_transactions),
                expected,
                "object floor {object_floor:?}, transactions from {lowest_with_transactions:?}"
            );
        }
    }
}

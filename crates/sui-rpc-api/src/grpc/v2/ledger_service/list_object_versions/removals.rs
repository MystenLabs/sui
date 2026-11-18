// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
//! !! THIS MODULE, AND THE FLOOR SEARCH IN `retention.rs`, ARE COMPLICATED ONLY BECAUSE THE     !!
//! !! FULLNODE HAS NO INDEX FROM AN OBJECT VERSION TO THE TRANSACTION THAT WROTE IT.           !!
//! !!                                                                                          !!
//! !! A tombstone stores no transaction digest, the version index is keyed by checkpoint, and  !!
//! !! the transaction bitmap holds object IDs without versions. So the transaction behind a    !!
//! !! removal is pieced together from all three: the item above it, a search of the version   !!
//! !! index for its checkpoint, a walk of the bitmap, and the candidates' effects.             !!
//! !!                                                                                          !!
//! !! An rpc index keyed by (object ID, version), holding the sequence number of the           !!
//! !! transaction that wrote that item, live or a removal, answers both questions with one     !!
//! !! read and lets all of this be deleted. It needs a backfill from history, so the next time !!
//! !! a release backfills an rpc index from history, ADD THIS INDEX IN THAT SAME RELEASE.      !!
//! !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

use crate::Result;
use crate::reader::StateReader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::ObjectOrTombstone;
use tokio_util::sync::CancellationToken;

use super::super::chunked_scan::cancelled;
use super::IndexedTip;
use super::retention::first_item_after;
use super::stored_version;

/// A tombstone whose removal a listing has found, which the lookup for the tombstone next to it
/// in listing order starts from.
#[derive(Clone, Copy, Debug)]
pub(super) struct FoundRemoval {
    pub(super) version: SequenceNumber,
    pub(super) transaction: LedgerTxSeqDigest,
}

/// The transaction that deleted or wrapped `object_id` at the tombstone stored at `version`, or
/// `None` when this node no longer has what it needs to find it. Fails with `CANCELLED` once
/// `cancel` fires.
///
/// Nothing changes a deleted object until a transaction creates it again under the same ID, nor
/// a wrapped one until a transaction unwraps or deletes it. Such a transaction writes the
/// object's next item, unless it creates the object straight into a wrapper, which writes no
/// item of it. So between the transaction that wrote the item right below the tombstone and the
/// one that wrote the item right above it, the object's only transactions are the removal and
/// such creations, and the transaction bitmap lists them. The lookup walks them from a bound it
/// knows, passing over candidates that removed nothing:
/// - The item above is a live version: back from the transaction that wrote it.
/// - There is no item above up to the indexed tip: back from the end of the tip checkpoint's
///   transactions.
/// - The item above is a tombstone whose removal `previous` holds, which a descending listing
///   found last: back from that removal.
/// - The item below is a tombstone whose removal `previous` holds, which an ascending listing
///   found last: forward from that removal.
/// - Otherwise, inside a run of tombstones: back from the end of the checkpoint the version index
///   places the removal in, the first one after the floor at whose end it records a version
///   above the object's stored item right below the tombstone, stopping below that checkpoint.
///   The comparison is with the item below because the index records a removal by a transaction
///   with V1 effects below its tombstone (see [`super::stored_version_at`]). Later transactions
///   of the object in that checkpoint are passed over, so a listing pays for them once, at the
///   first tombstone it looks up in a run.
///
/// `None` when the transaction that wrote the item above is not indexed, when a candidate's
/// rpc-index row or effects are pruned or it is at or below `floor_checkpoint`, or when the
/// search or the walk finds no removal at `version`.
pub(super) fn removal_transaction(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    previous: Option<FoundRemoval>,
    floor_checkpoint: Option<u64>,
    tip: &IndexedTip,
    cancel: &CancellationToken,
) -> Result<Option<LedgerTxSeqDigest>> {
    let after_floor = floor_checkpoint.map_or(0, |floor| floor.saturating_add(1));
    let walk_back_from = |before, lowest_checkpoint| {
        walk_back(
            reader,
            object_id,
            version,
            before,
            lowest_checkpoint,
            cancel,
        )
    };
    let above = match first_item_after(reader, object_id, version, tip)? {
        None => {
            return walk_back_from(
                reader.transactions_end(tip.upper_bound.highest_indexed())?,
                after_floor,
            );
        }
        Some(ObjectOrTombstone::Object(object)) => {
            return match reader.transaction_sequence(&object.previous_transaction)? {
                Some(sequence) => walk_back_from(sequence, after_floor),
                None => Ok(None),
            };
        }
        Some(ObjectOrTombstone::Tombstone((_, above, _))) => above,
    };
    if let Some(previous) = previous
        && previous.version == above
    {
        return walk_back_from(previous.transaction.tx_sequence_number, after_floor);
    }
    let below = match version.value().checked_sub(1) {
        Some(below) => reader
            .object_versions_iter(
                object_id,
                SequenceNumber::MIN..=SequenceNumber::from_u64(below),
                true,
            )?
            .next()
            .transpose()?
            .map(|item| stored_version(&item)),
        None => None,
    };
    if let Some(previous) = previous
        && Some(previous.version) == below
    {
        return walk_forward(
            reader,
            object_id,
            version,
            previous.transaction.tx_sequence_number + 1,
            tip,
            cancel,
        );
    }
    let Some((checkpoint, _)) =
        reader.first_checkpoint_recording_above(object_id, below, after_floor, tip)?
    else {
        return Ok(None);
    };
    walk_back_from(reader.transactions_end(checkpoint)?, checkpoint)
}

/// The transaction that removed `object_id` at `version`: the first of the object's
/// transactions from sequence number `from` on, when the transactions before it that changed
/// the object since `from` removed nothing (see [`removal_transaction`]). `None` at the first
/// candidate that removed the object at another version, that is above the indexed tip, or
/// whose rpc-index row or effects are pruned.
fn walk_forward(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    mut from: u64,
    tip: &IndexedTip,
    cancel: &CancellationToken,
) -> Result<Option<LedgerTxSeqDigest>> {
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let Some(sequence) = reader.next_object_transaction(object_id, from)? else {
            return Ok(None);
        };
        let Some(transaction) = reader.ledger_transaction(sequence)? else {
            return Ok(None);
        };
        if transaction.checkpoint_number > tip.upper_bound.highest_indexed() {
            return Ok(None);
        }
        match reader.removal_version(&transaction.digest, object_id) {
            Some(Some(removed)) => return Ok((removed == version).then_some(transaction)),
            Some(None) => from = sequence + 1,
            None => return Ok(None),
        }
    }
}

/// The transaction that removed `object_id` at `version`: the last of the object's transactions
/// before the one with sequence number `before` whose effects show it. Candidates whose effects
/// show no removal, or one at a later version by a later transaction of the same checkpoint, are
/// passed over. `None` at the first candidate that removed the object below `version`, which
/// the walk passed the removal to reach, in a checkpoint below `lowest_checkpoint`, or whose
/// rpc-index row or effects are pruned. The indexes prune a transaction's bitmap entry no sooner
/// than its row, so the walk never passes over a removal whose entry is gone without stopping
/// at a pruned row first.
fn walk_back(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    mut before: u64,
    lowest_checkpoint: u64,
    cancel: &CancellationToken,
) -> Result<Option<LedgerTxSeqDigest>> {
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let Some(sequence) = reader.last_object_transaction(object_id, before)? else {
            return Ok(None);
        };
        let Some(transaction) = reader.ledger_transaction(sequence)? else {
            return Ok(None);
        };
        if transaction.checkpoint_number < lowest_checkpoint {
            return Ok(None);
        }
        match reader.removal_version(&transaction.digest, object_id) {
            Some(Some(removed)) if removed == version => return Ok(Some(transaction)),
            Some(Some(removed)) if removed < version => return Ok(None),
            Some(_) => before = sequence,
            None => return Ok(None),
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::reader::StateReader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::ObjectOrTombstone;

use super::IndexedTip;
use super::next_version;

/// The transaction that deleted or wrapped an object at a version, as the object's entries in
/// the transaction bitmap place it.
pub(super) struct Removal {
    /// The rpc indexes' row for the removal's transaction if `confirmed`, or else for a
    /// transaction no earlier than it.
    pub(super) transaction: LedgerTxSeqDigest,
    /// Whether the effects of `transaction` show that it is the removal. Without them, it is the
    /// removal or a later transaction that created the object straight into a wrapper.
    pub(super) confirmed: bool,
}

/// The [`Removal`] of `object_id` at `version`.
///
/// Nothing changes a deleted object until a transaction creates it again under the same ID, nor
/// a wrapped one until a transaction unwraps or deletes it. Such a transaction writes the
/// object's next item, unless it creates the object straight into a wrapper, which writes no item
/// of it. So the removal is the object's last transaction before the one that wrote its next
/// item, or, when it has no next item the indexes have committed, before the end of the indexed
/// transactions ([`IndexedTip`]), once such creations are passed over: walking back from there,
/// the first candidate whose effects show that it removed the object at `version` is the
/// removal, and one whose effects show otherwise is passed over. The first candidate whose
/// effects are pruned is the removal unconfirmed: effects are pruned oldest checkpoint first, so
/// no earlier candidate's are kept.
///
/// `None` when this node no longer has what it needs to find the removal: the sequence number of
/// the transaction that wrote the next item, or a candidate's bitmap entry or rpc-index row. The
/// indexes prune a transaction's bitmap entry no sooner than its row, so the walk never passes
/// over a removal whose entry is gone without stopping at a pruned row first.
pub(super) fn removal(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    tip: &IndexedTip,
) -> Result<Option<Removal>> {
    let after = next_version(version);
    let next_item = if after > tip.newest_version {
        None
    } else {
        reader
            .object_versions_iter(object_id, after..=tip.newest_version, false)?
            .next()
            .transpose()?
    };
    let next_transaction = match next_item {
        None => Some(tip.transactions_end),
        Some(ObjectOrTombstone::Object(object)) => {
            reader.transaction_sequence(&object.previous_transaction)?
        }
        Some(ObjectOrTombstone::Tombstone((_, next_version, _))) => {
            removal(reader, object_id, next_version, tip)?
                .map(|removal| removal.transaction.tx_sequence_number)
        }
    };
    let Some(mut before) = next_transaction else {
        return Ok(None);
    };
    loop {
        let Some(sequence) = reader.last_object_transaction(object_id, before)? else {
            return Ok(None);
        };
        let Some(transaction) = reader.ledger_transaction(sequence)? else {
            return Ok(None);
        };
        let confirmed = match reader.removed_object(&transaction.digest, object_id, version) {
            Some(true) => true,
            Some(false) => {
                before = sequence;
                continue;
            }
            None => false,
        };
        return Ok(Some(Removal {
            transaction,
            confirmed,
        }));
    }
}

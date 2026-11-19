// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::Result;
use crate::reader::StateReader;
use crate::reader::indexes_disabled;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::encode_dimension_key;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::storage::LedgerBitmapBucket;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::ObjectOrTombstone;

use super::super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::super::ledger_read::storage_error;
use super::IndexedTip;

/// The reads that bound a fullnode listing, decide its retention floor, and find the
/// transactions that removed the object.
impl StateReader {
    /// [`sui_types::storage::RpcIndexes::object_versions_iter`].
    pub(super) fn object_versions_iter(
        &self,
        object_id: ObjectID,
        versions: RangeInclusive<SequenceNumber>,
        descending: bool,
    ) -> Result<impl Iterator<Item = Result<ObjectOrTombstone>> + '_> {
        let versions = self
            .inner()
            .indexes()
            .ok_or_else(indexes_disabled)?
            .object_versions_iter(object_id, versions, descending)
            .map_err(storage_error)?;
        Ok(versions.map(|version| version.map_err(storage_error)))
    }

    /// The rpc indexes' row for the transaction with sequence number `sequence`, holding its
    /// digest and the checkpoint that executed it, or `None` if the indexes have pruned it, which
    /// they do in the same write that raises the floor past that checkpoint.
    pub(super) fn ledger_transaction(&self, sequence: u64) -> Result<Option<LedgerTxSeqDigest>> {
        self.inner()
            .indexes()
            .ok_or_else(indexes_disabled)?
            .ledger_tx_seq_digest(sequence)
            .map_err(storage_error)
    }

    /// The version at which transaction `digest` deleted or wrapped `object_id`, `Some(None)` if
    /// it did neither, or `None` if its effects are pruned.
    pub(super) fn removal_version(
        &self,
        digest: &TransactionDigest,
        object_id: ObjectID,
    ) -> Option<Option<SequenceNumber>> {
        let effects = self.inner().get_transaction_effects(digest)?;
        Some(
            effects
                .all_tombstones()
                .into_iter()
                .find_map(|(id, version)| (id == object_id).then_some(version)),
        )
    }

    /// The first checkpoint from `low` up to the indexed tip at whose end the version index
    /// records a version of `object_id` above `version`, or any version when `version` is
    /// `None`, with the version it records there. `None` if no checkpoint in that range does.
    ///
    /// For any version, a read of the object's first index row answers (see
    /// [`sui_types::storage::RpcIndexes::first_checkpoint_with_object_version`]). Above a
    /// version, a binary search does: the index is keyed by checkpoint, and the version it
    /// records for an object never decreases from one checkpoint to the next, so the checkpoints
    /// that record a version above `version` are the upper end of the range, and the search
    /// finds the first of them with one index read per halving of the range.
    pub(super) fn first_checkpoint_recording_above(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
        mut low: u64,
        tip: &IndexedTip,
    ) -> Result<Option<(u64, SequenceNumber)>> {
        let mut high = tip.upper_bound.highest_indexed();
        if low > high {
            return Ok(None);
        }
        let Some(version) = version else {
            return self
                .inner()
                .indexes()
                .ok_or_else(indexes_disabled)?
                .first_checkpoint_with_object_version(object_id, low..=high)
                .map_err(storage_error);
        };
        let recorded_above = |checkpoint| -> Result<Option<SequenceNumber>> {
            Ok(self
                .get_object_version_at_checkpoint(object_id, checkpoint, &tip.upper_bound)?
                .filter(|recorded| version < *recorded))
        };
        let Some(mut recorded) = recorded_above(high)? else {
            return Ok(None);
        };
        while low < high {
            let middle = low + (high - low) / 2;
            match recorded_above(middle)? {
                Some(at_middle) => {
                    high = middle;
                    recorded = at_middle;
                }
                None => low = middle + 1,
            }
        }
        Ok(Some((high, recorded)))
    }

    /// One past the sequence number of the last transaction in `checkpoint`. Fails with
    /// `INTERNAL` if the checkpoint's summary is not in the store: pruning never deletes
    /// summaries, and every checkpoint looked up is between the floor and the indexed tip.
    pub(super) fn transactions_end(&self, checkpoint: u64) -> Result<u64> {
        self.inner()
            .get_checkpoint_by_sequence_number(checkpoint)
            .map(|summary| summary.data().network_total_transactions)
            .ok_or_else(|| storage_error(format!("missing the summary of checkpoint {checkpoint}")))
    }

    /// The sequence number of the last transaction before `before` that changed `object_id`,
    /// from the transaction bitmap's affected-object dimension. Below the pruned floor the bitmap
    /// may still hold some transactions, until compaction drops them.
    pub(super) fn last_object_transaction(
        &self,
        object_id: ObjectID,
        before: u64,
    ) -> Result<Option<u64>> {
        let Some(last) = before.checked_sub(1) else {
            return Ok(None);
        };
        for bucket in
            self.object_transaction_buckets(object_id, 0, last / TX_BITMAP_BUCKET_SIZE + 1, true)?
        {
            if let Some(sequence) = last_at_or_below(&bucket?, last) {
                return Ok(Some(sequence));
            }
        }
        Ok(None)
    }

    /// The sequence number of the first transaction at or after `from` that changed `object_id`,
    /// from the transaction bitmap's affected-object dimension.
    pub(super) fn next_object_transaction(
        &self,
        object_id: ObjectID,
        from: u64,
    ) -> Result<Option<u64>> {
        let first_bucket = from / TX_BITMAP_BUCKET_SIZE;
        for bucket in self.object_transaction_buckets(object_id, first_bucket, u64::MAX, false)? {
            if let Some(sequence) = first_at_or_after(&bucket?, from) {
                return Ok(Some(sequence));
            }
        }
        Ok(None)
    }

    /// The buckets of `object_id`'s entries in the transaction bitmap's affected-object
    /// dimension, from bucket `start` to before bucket `end`, highest first if `descending`.
    fn object_transaction_buckets(
        &self,
        object_id: ObjectID,
        start: u64,
        end: u64,
        descending: bool,
    ) -> Result<impl Iterator<Item = Result<LedgerBitmapBucket>> + '_> {
        let buckets = self
            .inner()
            .indexes()
            .ok_or_else(indexes_disabled)?
            .transaction_bitmap_bucket_iter(
                encode_dimension_key(IndexDimension::AffectedObject, object_id.as_ref()),
                start,
                end,
                descending,
            )
            .map_err(storage_error)?;
        Ok(buckets.map(|bucket| bucket.map_err(storage_error)))
    }

    /// The sequence number of transaction `digest`, or `None` if the indexes have not committed
    /// it or it is pruned.
    pub(super) fn transaction_sequence(&self, digest: &TransactionDigest) -> Result<Option<u64>> {
        self.inner()
            .indexes()
            .ok_or_else(indexes_disabled)?
            .ledger_tx_seq_by_digest(digest)
            .map_err(storage_error)
    }
}

/// The highest sequence number in `bucket` at or below `last`.
fn last_at_or_below(bucket: &LedgerBitmapBucket, last: u64) -> Option<u64> {
    let first_in_bucket = bucket.bucket_id * TX_BITMAP_BUCKET_SIZE;
    let last_bit = last
        .checked_sub(first_in_bucket)?
        .min(TX_BITMAP_BUCKET_SIZE - 1);
    let bit = bucket.bitmap.range(..=last_bit as u32).next_back()?;
    Some(first_in_bucket + u64::from(bit))
}

/// The lowest sequence number in `bucket` at or above `from`.
fn first_at_or_after(bucket: &LedgerBitmapBucket, from: u64) -> Option<u64> {
    let first_in_bucket = bucket.bucket_id * TX_BITMAP_BUCKET_SIZE;
    let from_bit = from
        .saturating_sub(first_in_bucket)
        .min(TX_BITMAP_BUCKET_SIZE);
    let bit = bucket.bitmap.range(from_bit as u32..).next()?;
    Some(first_in_bucket + u64::from(bit))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walk step finds the nearest entry on its side of the bound, the bound included, at the
    /// bucket's first and last bit, and nothing past the bucket's ends.
    #[test]
    fn bucket_steps_find_the_nearest_entry_at_or_past_the_bound() {
        let size = TX_BITMAP_BUCKET_SIZE;
        let first = 2 * size;
        let bucket = LedgerBitmapBucket {
            bucket_id: 2,
            bitmap: [0u32, 5, (size - 1) as u32].into_iter().collect(),
        };
        let last_entry = first + size - 1;

        for (last, expected) in [
            (first - 1, None),
            (first, Some(first)),
            (first + 4, Some(first)),
            (first + 5, Some(first + 5)),
            (last_entry - 1, Some(first + 5)),
            (last_entry, Some(last_entry)),
            (first + 10 * size, Some(last_entry)),
        ] {
            assert_eq!(
                last_at_or_below(&bucket, last),
                expected,
                "at or below {last}"
            );
        }
        for (from, expected) in [
            (0, Some(first)),
            (first, Some(first)),
            (first + 1, Some(first + 5)),
            (first + 5, Some(first + 5)),
            (first + 6, Some(last_entry)),
            (last_entry, Some(last_entry)),
            (last_entry + 1, None),
        ] {
            assert_eq!(
                first_at_or_after(&bucket, from),
                expected,
                "at or after {from}"
            );
        }
    }
}

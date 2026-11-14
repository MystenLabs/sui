// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::Result;
use crate::RpcError;
use crate::reader::StateReader;
use crate::reader::indexes_disabled;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::encode_dimension_key;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object as StoredObject;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::ObjectOrTombstone;

use super::super::bitmap_scan::TX_BITMAP_BUCKET_SIZE;
use super::super::ledger_read::storage_error;
use super::retention::created_under_new_id;

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

    /// Whether transaction `digest` created the live version `object` under an ID that no
    /// earlier object can have had. An object whose ID derives from a parent and a key, a dynamic
    /// field or an object claimed through `sui::derived_object`, can be deleted and created again
    /// under the same ID, so its creation does not show that it began the ID's history.
    pub(super) fn created_new_object_id(
        &self,
        digest: &TransactionDigest,
        object: &StoredObject,
    ) -> bool {
        let created = self
            .inner()
            .get_transaction_effects(digest)
            .map(|effects| effects.created());
        created_under_new_id(digest, object, created.as_deref(), |id, version| {
            self.inner().get_object_by_key(id, version)
        })
    }

    /// Whether transaction `digest` deleted or wrapped `object_id` at `version`, or `None` if its
    /// effects are pruned.
    pub(super) fn removed_object(
        &self,
        digest: &TransactionDigest,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<bool> {
        let effects = self.inner().get_transaction_effects(digest)?;
        Some(effects.all_tombstones().contains(&(object_id, version)))
    }

    /// One past the sequence number of the last transaction in `checkpoint`, the newest checkpoint
    /// the rpc indexes have committed. Pruning never reaches the newest checkpoint, so a missing
    /// summary is `INTERNAL`.
    pub(super) fn transactions_end(&self, checkpoint: u64) -> Result<u64> {
        self.inner()
            .get_checkpoint_by_sequence_number(checkpoint)
            .map(|summary| summary.data().network_total_transactions)
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("missing summary of the indexed tip checkpoint {checkpoint}"),
                )
            })
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
        let buckets = self
            .inner()
            .indexes()
            .ok_or_else(indexes_disabled)?
            .transaction_bitmap_bucket_iter(
                encode_dimension_key(IndexDimension::AffectedObject, object_id.as_ref()),
                0,
                last / TX_BITMAP_BUCKET_SIZE + 1,
                true,
            )
            .map_err(storage_error)?;
        for bucket in buckets {
            let bucket = bucket.map_err(storage_error)?;
            let last_in_bucket = bucket
                .bitmap
                .iter()
                .rev()
                .map(|bit| bucket.bucket_id * TX_BITMAP_BUCKET_SIZE + u64::from(bit))
                .find(|sequence| *sequence < before);
            if last_in_bucket.is_some() {
                return Ok(last_in_bucket);
            }
        }
        Ok(None)
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

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `TxMetadata`.
//!
//! Carries digest, containing checkpoint, position-within-checkpoint,
//! event count, and timestamp. The `tx_seq → digest` direction of the
//! bijection lives here; the inverse is
//! [`super::tx_seq_by_digest`].

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::proto::TxMetadata as StoredTxMetadata;
use crate::schema::keys::U64Be;

pub const NAME: &str = "tx_metadata_by_seq";

pub type Key = U64Be;
pub type Value = Protobuf<StoredTxMetadata>;

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

/// Caller-facing view of one row, with the digest decoded back to
/// `TransactionDigest` and the integer fields exposed in canonical
/// widths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub digest: TransactionDigest,
    pub checkpoint_seq: CheckpointSequenceNumber,
    /// 0-based position of this transaction within its checkpoint's
    /// contents.
    pub ckpt_position: u32,
    /// Number of events emitted by this transaction.
    pub event_count: u32,
    /// Wall-clock timestamp of the containing checkpoint, in
    /// milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
}

/// Build a `TxMetadata` row from a `Metadata` view.
pub fn store(metadata: &Metadata) -> Value {
    Protobuf(StoredTxMetadata {
        digest: metadata.digest.inner().to_vec().into(),
        checkpoint_seq: metadata.checkpoint_seq,
        ckpt_position: metadata.ckpt_position,
        event_count: metadata.event_count,
        timestamp_ms: metadata.timestamp_ms,
    })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the metadata for the transaction at the given
    /// assigned `tx_seq`.
    pub fn get_tx_metadata_by_seq(&self, tx_seq: u64) -> Result<Option<Metadata>, Error> {
        let Some(stored) = self.tx_metadata_by_seq.get(&U64Be(tx_seq))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let digest_bytes: [u8; 32] = stored.digest.as_ref().try_into().map_err(|_| {
            DecodeError::msg(format!(
                "expected 32 bytes for {NAME} digest, got {}",
                stored.digest.len(),
            ))
        })?;
        Ok(Some(Metadata {
            digest: TransactionDigest::new(digest_bytes),
            checkpoint_seq: stored.checkpoint_seq,
            ckpt_position: stored.ckpt_position,
            event_count: stored.event_count,
            timestamp_ms: stored.timestamp_ms,
        }))
    }

    /// Iterate `(tx_seq, digest)` pairs over `[from, to_exclusive)`,
    /// decoding only the digest from each row.
    ///
    /// The pruner uses this to unindex `tx_seq_by_digest` for a pruned
    /// range. Iterating the table seeks straight to the first present
    /// row and visits only rows that exist, so a sparse range — or a
    /// floor of `0` when the lower bound is unknown — costs work
    /// proportional to the rows actually present, not to the width of
    /// the `tx_seq` interval.
    pub fn iter_tx_seq_digests(
        &self,
        from: u64,
        to_exclusive: u64,
    ) -> Result<impl Iterator<Item = Result<(u64, TransactionDigest), Error>> + '_, Error> {
        let iter = self
            .tx_metadata_by_seq
            .iter(U64Be(from)..U64Be(to_exclusive))?
            .map(|entry| {
                let (U64Be(tx_seq), stored) = entry?;
                let stored = stored.into_inner();
                let digest_bytes: [u8; 32] = stored.digest.as_ref().try_into().map_err(|_| {
                    DecodeError::msg(format!(
                        "expected 32 bytes for {NAME} digest, got {}",
                        stored.digest.len(),
                    ))
                })?;
                Ok((tx_seq, TransactionDigest::new(digest_bytes)))
            });
        Ok(iter)
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_metadata() -> Metadata {
        Metadata {
            digest: TransactionDigest::random(),
            checkpoint_seq: 100,
            ckpt_position: 3,
            event_count: 5,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_tx_metadata_by_seq(7).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let metadata = dummy_metadata();

        let mut batch = db.batch();
        batch
            .put(&schema.tx_metadata_by_seq, &U64Be(42), &store(&metadata))
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_tx_metadata_by_seq(42)
            .unwrap()
            .expect("metadata present");
        assert_eq!(read, metadata);
    }

    #[test]
    fn overwrite_replaces_previous() {
        let (_dir, db, schema) = fresh_db();
        let first = dummy_metadata();
        let later = dummy_metadata();

        let mut batch = db.batch();
        batch
            .put(&schema.tx_metadata_by_seq, &U64Be(42), &store(&first))
            .unwrap();
        batch
            .put(&schema.tx_metadata_by_seq, &U64Be(42), &store(&later))
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_tx_metadata_by_seq(42)
            .unwrap()
            .expect("metadata present");
        assert_eq!(read, later);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `CheckpointDigest` → `checkpoint_seq`.
//!
//! Resolves a checkpoint digest to its sequence number, which then
//! keys every checkpoint-keyed CF.
//!
//! Both `Key` and `Value` are thin newtypes (a 32-byte digest and a
//! varint-encoded `u64`), so no `store` helper is provided —
//! indexer pipelines stage writes directly via
//! `batch.put(&schema.checkpoint_seq_by_digest, &Key(digest), &U64Varint(seq))`.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "checkpoint_seq_by_digest";

/// Wrapper around `CheckpointDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub CheckpointDigest);

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.inner());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 32 {
            return Err(DecodeError::msg(format!(
                "expected 32 bytes for {NAME} Key, got {}",
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; 32];
        buf.copy_to_slice(&mut bytes);
        Ok(Key(CheckpointDigest::new(bytes)))
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the sequence number of the checkpoint identified by
    /// `digest`.
    pub fn get_checkpoint_seq_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(self
            .checkpoint_seq_by_digest
            .get(&Key(*digest))?
            .map(|v| v.0))
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

    #[test]
    fn get_returns_none_for_unknown_digest() {
        let (_dir, _db, schema) = fresh_db();
        let digest = CheckpointDigest::random();
        assert!(
            schema
                .get_checkpoint_seq_by_digest(&digest)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn put_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let digest = CheckpointDigest::random();

        let mut batch = db.batch();
        batch
            .put(
                &schema.checkpoint_seq_by_digest,
                &Key(digest),
                &U64Varint(42),
            )
            .unwrap();
        batch.commit().unwrap();

        let seq = schema
            .get_checkpoint_seq_by_digest(&digest)
            .unwrap()
            .expect("digest present");
        assert_eq!(seq, 42);
    }

    #[test]
    fn distinct_digests_dont_collide() {
        let (_dir, db, schema) = fresh_db();
        let d1 = CheckpointDigest::random();
        let d2 = CheckpointDigest::random();

        let mut batch = db.batch();
        batch
            .put(&schema.checkpoint_seq_by_digest, &Key(d1), &U64Varint(1))
            .unwrap();
        batch
            .put(&schema.checkpoint_seq_by_digest, &Key(d2), &U64Varint(2))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(schema.get_checkpoint_seq_by_digest(&d1).unwrap(), Some(1),);
        assert_eq!(schema.get_checkpoint_seq_by_digest(&d2).unwrap(), Some(2),);
    }
}

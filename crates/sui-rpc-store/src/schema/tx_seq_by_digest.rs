// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `TransactionDigest` → `tx_seq`.
//!
//! One half of the digest <-> sequence bijection. The inverse lives
//! in [`super::tx_metadata_by_seq`].
//!
//! Both `Key` and `Value` are thin newtypes (a 32-byte digest and a
//! varint-encoded `u64`), so no `store` helper is provided —
//! indexer pipelines stage writes directly via
//! `batch.put(&schema.tx_seq_by_digest, &Key(digest), &U64Varint(seq))`.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::digests::TransactionDigest;

use crate::schema::primitives::U64Varint;

pub const NAME: &str = "tx_seq_by_digest";

/// Wrapper around `TransactionDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub TransactionDigest);

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
        Ok(Key(TransactionDigest::new(bytes)))
    }
}

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the sequence number of the transaction identified
    /// by `digest`.
    pub fn get_tx_seq_by_digest(&self, digest: &TransactionDigest) -> Result<Option<u64>, Error> {
        Ok(self.tx_seq_by_digest.get(&Key(*digest))?.map(|v| v.0))
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
        let digest = TransactionDigest::random();
        assert!(schema.get_tx_seq_by_digest(&digest).unwrap().is_none());
    }

    #[test]
    fn put_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let digest = TransactionDigest::random();

        let mut batch = db.batch();
        batch
            .put(&schema.tx_seq_by_digest, &Key(digest), &U64Varint(42))
            .unwrap();
        batch.commit().unwrap();

        let seq = schema
            .get_tx_seq_by_digest(&digest)
            .unwrap()
            .expect("digest present");
        assert_eq!(seq, 42);
    }

    #[test]
    fn distinct_digests_dont_collide() {
        let (_dir, db, schema) = fresh_db();
        let d1 = TransactionDigest::random();
        let d2 = TransactionDigest::random();

        let mut batch = db.batch();
        batch
            .put(&schema.tx_seq_by_digest, &Key(d1), &U64Varint(1))
            .unwrap();
        batch
            .put(&schema.tx_seq_by_digest, &Key(d2), &U64Varint(2))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(schema.get_tx_seq_by_digest(&d1).unwrap(), Some(1));
        assert_eq!(schema.get_tx_seq_by_digest(&d2).unwrap(), Some(2));
    }
}

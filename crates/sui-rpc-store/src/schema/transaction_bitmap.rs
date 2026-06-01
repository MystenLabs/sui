// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(dimension_key, bucket)` → `BitmapBlob`.
//!
//! Inverted bitmap index over `tx_seq` space. The dimension key is
//! a variable-length opaque token (e.g. `[tag][sender]`); each
//! bucket holds the roaring bitmap of tx_seqs whose containing
//! transaction matches that dimension.
//!
//! Indexer pipelines stage merge operands carrying a small bitmap
//! (often a single bit per write); the merge operator unions every
//! operand against the existing on-disk bitmap and emits a single
//! consolidated value optimized for the on-disk encoding.
//!
//! A per-bucket compaction filter reads the shared `tx_seq`
//! pruning floor from
//! [`pruning_watermark::tx_seq_floor`](super::pruning_watermark::tx_seq_floor)
//! and drops buckets whose entire `tx_seq` range sits below the
//! floor.

use std::sync::atomic::Ordering;

use bytes::Buf;
use bytes::BufMut;
use prost::Message;
use roaring::RoaringBitmap;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;

use crate::proto::BitmapBlob;
use crate::schema::pruning_watermark::tx_seq_floor;

pub const NAME: &str = "transaction_bitmap";

/// Number of consecutive `tx_seq` values represented by one
/// bucket. Sized to keep individual bitmaps small (~8 KiB at
/// worst-case density) and the per-row read cost predictable.
pub const TX_BUCKET_SIZE: u64 = 65_536;

const _: () = assert!(TX_BUCKET_SIZE <= u32::MAX as u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub dimension_key: Vec<u8>,
    pub bucket: u64,
}

pub type Value = Protobuf<BitmapBlob>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.dimension_key);
        buf.put_slice(&self.bucket.to_be_bytes());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 8 {
            return Err(DecodeError::msg(format!(
                "{NAME} Key too short: {} bytes",
                buf.remaining(),
            )));
        }
        let dim_len = buf.remaining() - 8;
        let dim_bytes = buf.copy_to_bytes(dim_len);
        let bucket = buf.get_u64();
        Ok(Key {
            dimension_key: dim_bytes.to_vec(),
            bucket,
        })
    }
}

/// CF options: install the bitmap-union merge operator and a
/// per-bucket compaction filter that drops buckets whose entire
/// `tx_seq` range sits below the pruning floor.
pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    let mut opts = base_options.clone();
    opts.set_merge_operator_associative("transaction_bitmap_merge", merge);
    let floor = tx_seq_floor().clone();
    opts.set_compaction_filter("transaction_bitmap_pruning", move |_level, key, _value| {
        let pruned_exclusive = floor.load(Ordering::Relaxed);
        if should_remove_bucket(key, pruned_exclusive) {
            rocksdb::CompactionDecision::Remove
        } else {
            rocksdb::CompactionDecision::Keep
        }
    });
    opts
}

/// Pure logic of the compaction filter: decide whether the bucket
/// identified by `key`'s trailing 8-byte big-endian `bucket_id`
/// can be removed given the exclusive `tx_seq` pruning floor.
///
/// A bucket is removable iff every `tx_seq` it covers is strictly
/// below the floor — i.e. `(bucket_id + 1) * TX_BUCKET_SIZE <=
/// pruned_exclusive`. Arithmetic uses `checked_*` so a corrupted
/// `bucket_id` can't overflow and cause spurious removal.
///
/// Kept rather than removed on any malformed input — silent data
/// loss is worse than a stuck row.
pub(crate) fn should_remove_bucket(key: &[u8], pruned_exclusive: u64) -> bool {
    if key.len() < 8 {
        return false;
    }
    let bucket_id_bytes: [u8; 8] = key[key.len() - 8..].try_into().expect("slice length");
    let bucket_id = u64::from_be_bytes(bucket_id_bytes);
    bucket_id
        .checked_add(1)
        .and_then(|b| b.checked_mul(TX_BUCKET_SIZE))
        .is_some_and(|highest_plus_one| highest_plus_one <= pruned_exclusive)
}

/// The bucket that owns a given `tx_seq`.
pub fn bucket_of(tx_seq: u64) -> u64 {
    tx_seq / TX_BUCKET_SIZE
}

/// The bit position within a bucket for a given `tx_seq`. The
/// cast is safe because `TX_BUCKET_SIZE <= u32::MAX` (enforced
/// at compile time above).
pub fn bit_of(tx_seq: u64) -> u32 {
    (tx_seq % TX_BUCKET_SIZE) as u32
}

/// Build a `(Key, Value)` pair that adds `tx_seq` to the bitmap
/// for `(dimension_key, bucket_of(tx_seq))`. The merge operator
/// unions this single-bit operand with whatever's already on
/// disk.
pub fn store_match(dimension_key: Vec<u8>, tx_seq: u64) -> (Key, Value) {
    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(bit_of(tx_seq));
    store_bitmap(dimension_key, bucket_of(tx_seq), bitmap)
}

/// Build a `(Key, Value)` pair that stages the given bitmap as a
/// merge operand against the existing on-disk bitmap. Useful for
/// pipelines that batch many tx_seqs into one bucket per
/// dimension before writing.
pub fn store_bitmap(dimension_key: Vec<u8>, bucket: u64, bitmap: RoaringBitmap) -> (Key, Value) {
    (
        Key {
            dimension_key,
            bucket,
        },
        Protobuf(BitmapBlob {
            data: serialize_bitmap(&bitmap).into(),
        }),
    )
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the bitmap for `(dimension_key, bucket)` and
    /// return it deserialized.
    pub fn get_transaction_bitmap(
        &self,
        dimension_key: Vec<u8>,
        bucket: u64,
    ) -> Result<Option<RoaringBitmap>, Error> {
        let Some(stored) = self.transaction_bitmap.get(&Key {
            dimension_key,
            bucket,
        })?
        else {
            return Ok(None);
        };
        let bytes = stored.into_inner().data;
        let bitmap = RoaringBitmap::deserialize_from(bytes.as_ref())
            .map_err(|e| DecodeError::with_source("deserialize RoaringBitmap", e))?;
        Ok(Some(bitmap))
    }

    /// Iterate every bucket recorded against `dimension_key`, in
    /// ascending bucket order.
    pub fn iter_transaction_bitmap_buckets(
        &self,
        dimension_key: Vec<u8>,
    ) -> Result<Iter<'_, Key, Value>, Error> {
        self.transaction_bitmap
            .iter_prefix(&DimensionPrefix(dimension_key))
    }
}

/// Prefix encoder for "all buckets recorded against
/// `dimension_key`". Encodes as the raw dimension bytes — the
/// leading bytes of every `Key` whose `dimension_key` matches.
pub struct DimensionPrefix(pub Vec<u8>);

impl Encode for DimensionPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.0);
        Ok(())
    }
}

/// Serialize a roaring bitmap for on-disk storage. Run-encodes
/// dense containers first so a bucket that matches many
/// consecutive `tx_seq` values compresses well.
fn serialize_bitmap(bitmap: &RoaringBitmap) -> Vec<u8> {
    let mut buf = Vec::with_capacity(bitmap.serialized_size());
    bitmap
        .serialize_into(&mut buf)
        .expect("RoaringBitmap::serialize_into on Vec cannot fail");
    buf
}

/// Associative merge: union every operand bitmap with the
/// existing on-disk bitmap, then optimize the accumulator before
/// re-serializing.
///
/// Encode / decode failures panic — this CF is written only by
/// the crate's `store_*` helpers, so a parse failure indicates
/// corruption rather than a recoverable situation.
fn merge(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    let mut acc = match existing_val {
        Some(bytes) => decode_bitmap(bytes),
        None => RoaringBitmap::new(),
    };

    for operand in operands {
        let bitmap = decode_bitmap(operand);
        acc |= bitmap;
    }

    // Convert dense containers to runs before persisting. The
    // operands are typically tiny (one bit per call) so there's
    // nothing for run-encoding to collapse on them; the
    // accumulator is what RocksDB writes back to disk.
    acc.optimize();
    Some(encode_bitmap_blob(&acc))
}

fn decode_bitmap(bytes: &[u8]) -> RoaringBitmap {
    let blob = BitmapBlob::decode(bytes).expect("decode BitmapBlob");
    RoaringBitmap::deserialize_from(blob.data.as_ref()).expect("deserialize RoaringBitmap")
}

fn encode_bitmap_blob(bitmap: &RoaringBitmap) -> Vec<u8> {
    let blob = BitmapBlob {
        data: serialize_bitmap(bitmap).into(),
    };
    blob.encode_to_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

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
    fn bucket_and_bit_math() {
        assert_eq!(bucket_of(0), 0);
        assert_eq!(bit_of(0), 0);
        assert_eq!(bucket_of(TX_BUCKET_SIZE - 1), 0);
        assert_eq!(bit_of(TX_BUCKET_SIZE - 1), (TX_BUCKET_SIZE - 1) as u32);
        assert_eq!(bucket_of(TX_BUCKET_SIZE), 1);
        assert_eq!(bit_of(TX_BUCKET_SIZE), 0);
        assert_eq!(bucket_of(3 * TX_BUCKET_SIZE + 7), 3);
        assert_eq!(bit_of(3 * TX_BUCKET_SIZE + 7), 7);
    }

    #[test]
    fn get_returns_none_for_unknown_bucket() {
        let (_dir, _db, schema) = fresh_db();
        assert!(
            schema
                .get_transaction_bitmap(b"sender:alice".to_vec(), 0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn single_match_round_trips_through_merge() {
        let (_dir, db, schema) = fresh_db();
        let (k, v) = store_match(b"sender:alice".to_vec(), 42);

        let mut batch = db.batch();
        batch.merge(&schema.transaction_bitmap, &k, &v).unwrap();
        batch.commit().unwrap();

        let bitmap = schema
            .get_transaction_bitmap(b"sender:alice".to_vec(), bucket_of(42))
            .unwrap()
            .expect("bitmap present");
        let bits: Vec<u32> = bitmap.iter().collect();
        assert_eq!(bits, vec![42]);
    }

    #[test]
    fn many_matches_in_one_bucket_union() {
        let (_dir, db, schema) = fresh_db();
        let dim = b"sender:alice".to_vec();

        let mut batch = db.batch();
        for tx_seq in [1u64, 17, 256, 9_999] {
            let (k, v) = store_match(dim.clone(), tx_seq);
            batch.merge(&schema.transaction_bitmap, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let bitmap = schema
            .get_transaction_bitmap(dim, 0)
            .unwrap()
            .expect("bitmap present");
        let bits: BTreeSet<u32> = bitmap.iter().collect();
        assert_eq!(bits, BTreeSet::from([1, 17, 256, 9_999]));
    }

    #[test]
    fn distinct_dimensions_do_not_alias() {
        let (_dir, db, schema) = fresh_db();
        let (k_a, v_a) = store_match(b"sender:alice".to_vec(), 42);
        let (k_b, v_b) = store_match(b"sender:bob".to_vec(), 100);
        let mut batch = db.batch();
        batch.merge(&schema.transaction_bitmap, &k_a, &v_a).unwrap();
        batch.merge(&schema.transaction_bitmap, &k_b, &v_b).unwrap();
        batch.commit().unwrap();

        let alice = schema
            .get_transaction_bitmap(b"sender:alice".to_vec(), 0)
            .unwrap()
            .unwrap();
        let bob = schema
            .get_transaction_bitmap(b"sender:bob".to_vec(), 0)
            .unwrap()
            .unwrap();
        assert_eq!(alice.iter().collect::<Vec<u32>>(), vec![42]);
        assert_eq!(bob.iter().collect::<Vec<u32>>(), vec![100]);
    }

    #[test]
    fn should_remove_bucket_drops_only_fully_pruned_ranges() {
        let dim = b"sender:alice";

        // A bucket whose highest tx_seq is exactly at the floor:
        // the floor is *exclusive*, so this bucket is still
        // partially live and must not be removed.
        let just_at_floor_key = Key {
            dimension_key: dim.to_vec(),
            bucket: 0,
        }
        .encode()
        .unwrap();
        assert!(!should_remove_bucket(
            &just_at_floor_key,
            TX_BUCKET_SIZE - 1
        ));

        // Move the floor one past the bucket's highest tx_seq:
        // every entry it could hold is pruned, removable.
        assert!(should_remove_bucket(&just_at_floor_key, TX_BUCKET_SIZE));

        // Bucket 3 covers `tx_seq` in `[3 * BUCKET, 4 * BUCKET)`.
        // Floor sitting in the middle of the bucket keeps it.
        let middle_key = Key {
            dimension_key: dim.to_vec(),
            bucket: 3,
        }
        .encode()
        .unwrap();
        assert!(!should_remove_bucket(
            &middle_key,
            3 * TX_BUCKET_SIZE + (TX_BUCKET_SIZE / 2),
        ));

        // Floor past the bucket's high end → removable.
        assert!(should_remove_bucket(&middle_key, 4 * TX_BUCKET_SIZE));

        // Key shorter than 8 bytes → kept.
        assert!(!should_remove_bucket(&[0u8; 4], u64::MAX));

        // Floor of 0 → nothing removable.
        assert!(!should_remove_bucket(&middle_key, 0));
    }

    #[test]
    fn iter_walks_buckets_for_one_dimension_in_order() {
        let (_dir, db, schema) = fresh_db();
        let dim = b"sender:alice".to_vec();
        let other = b"sender:bob".to_vec();

        let mut batch = db.batch();
        for tx_seq in [1u64, TX_BUCKET_SIZE + 5, 3 * TX_BUCKET_SIZE + 9] {
            let (k, v) = store_match(dim.clone(), tx_seq);
            batch.merge(&schema.transaction_bitmap, &k, &v).unwrap();
        }
        // Unrelated dimension — must not appear in our iter.
        let (k_other, v_other) = store_match(other, 7);
        batch
            .merge(&schema.transaction_bitmap, &k_other, &v_other)
            .unwrap();
        batch.commit().unwrap();

        let buckets: Vec<u64> = schema
            .iter_transaction_bitmap_buckets(dim)
            .unwrap()
            .map(|res| res.unwrap().0.bucket)
            .collect();
        assert_eq!(buckets, vec![0, 1, 3]);
    }
}

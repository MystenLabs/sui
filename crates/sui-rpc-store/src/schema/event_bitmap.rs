// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(dimension_key, bucket)` → `BitmapBlob`.
//!
//! Same wire shape as [`super::transaction_bitmap`]
//! but indexes packed-event-seq space — each set bit identifies a
//! single event by `(tx_seq << EVENT_BITS) | event_idx`.
//!
//! The merge operator is identical in structure to the
//! transaction-bitmap one (union + optimize). The per-bucket
//! compaction filter translates the `tx_seq` pruning floor from
//! [`pruning_watermark::tx_seq_floor`](super::pruning_watermark::tx_seq_floor)
//! into packed-event-seq space (`tx_seq << EVENT_BITS`) and drops
//! buckets that fit entirely below it.

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

pub const NAME: &str = "event_bitmap";

/// Number of low-order bits in a `packed_event_seq` reserved for
/// the per-transaction `event_idx`. A transaction may emit up to
/// `1 << EVENT_BITS` events without colliding with the next
/// transaction's packed range.
pub const EVENT_BITS: u32 = 16;

/// Number of consecutive `packed_event_seq` values represented by
/// one bucket. Sized so each bucket covers
/// `EVENT_BUCKET_SIZE >> EVENT_BITS = 4096` consecutive
/// transactions worth of events.
pub const EVENT_BUCKET_SIZE: u64 = 1 << 28;

const _: () = assert!(EVENT_BUCKET_SIZE <= u32::MAX as u64);

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
/// packed-event-seq range sits below the pruning floor.
pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    let mut opts = base_options.clone();
    opts.set_merge_operator_associative("event_bitmap_merge", merge);
    let floor = tx_seq_floor().clone();
    opts.set_compaction_filter("event_bitmap_pruning", move |_level, key, _value| {
        let tx_seq_pruned = floor.load(Ordering::Relaxed);
        if should_remove_bucket(key, tx_seq_pruned) {
            rocksdb::CompactionDecision::Remove
        } else {
            rocksdb::CompactionDecision::Keep
        }
    });
    opts
}

/// Pure logic of the compaction filter.
///
/// Translates the `tx_seq` floor into packed-event-seq space and
/// asks whether every packed event the bucket could hold is
/// strictly below the translated floor. Kept on any malformed
/// input — silent data loss is worse than a stuck row.
pub(crate) fn should_remove_bucket(key: &[u8], tx_seq_pruned_exclusive: u64) -> bool {
    if key.len() < 8 {
        return false;
    }
    let bucket_id_bytes: [u8; 8] = key[key.len() - 8..].try_into().expect("slice length");
    let bucket_id = u64::from_be_bytes(bucket_id_bytes);
    let packed_floor = packed_pruning_floor(tx_seq_pruned_exclusive);
    bucket_id
        .checked_add(1)
        .and_then(|b| b.checked_mul(EVENT_BUCKET_SIZE))
        .is_some_and(|highest_plus_one| highest_plus_one <= packed_floor)
}

/// Translate the `tx_seq` floor into packed-event-seq space.
///
/// `packed_event_seq = tx_seq << EVENT_BITS`. For
/// `tx_seq >= 2^(64 - EVENT_BITS)` the shift would overflow a
/// `u64`; we saturate to `u64::MAX`, which represents "every
/// possible event has been pruned" — the conservative direction
/// for a removal decision.
fn packed_pruning_floor(tx_seq_pruned_exclusive: u64) -> u64 {
    const OVERFLOW_THRESHOLD: u64 = 1u64 << (64 - EVENT_BITS);
    if tx_seq_pruned_exclusive < OVERFLOW_THRESHOLD {
        tx_seq_pruned_exclusive << EVENT_BITS
    } else {
        u64::MAX
    }
}

/// Pack `(tx_seq, event_idx)` into a single 64-bit positional
/// identifier: `tx_seq << EVENT_BITS | event_idx`.
pub fn pack(tx_seq: u64, event_idx: u32) -> u64 {
    (tx_seq << EVENT_BITS) | u64::from(event_idx)
}

/// The bucket that owns a given packed event sequence.
pub fn bucket_of(packed: u64) -> u64 {
    packed / EVENT_BUCKET_SIZE
}

/// The bit position within a bucket for a given packed event
/// sequence. The cast is safe because `EVENT_BUCKET_SIZE`
/// fits in a `u32` (enforced at compile time above).
pub fn bit_of(packed: u64) -> u32 {
    (packed % EVENT_BUCKET_SIZE) as u32
}

/// Build a `(Key, Value)` pair that adds the event identified by
/// `(tx_seq, event_idx)` to the bitmap for its dimension and
/// bucket. The merge operator unions this single-bit operand
/// with whatever's already on disk.
pub fn store_match(dimension_key: Vec<u8>, tx_seq: u64, event_idx: u32) -> (Key, Value) {
    let packed = pack(tx_seq, event_idx);
    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(bit_of(packed));
    store_bitmap(dimension_key, bucket_of(packed), bitmap)
}

/// Build a `(Key, Value)` pair that stages the given bitmap as a
/// merge operand against the existing on-disk bitmap. Useful for
/// pipelines that batch many events into one bucket per dimension
/// before writing.
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
    /// Look up the event bitmap for `(dimension_key, bucket)` and
    /// return it deserialized.
    pub fn get_event_bitmap(
        &self,
        dimension_key: Vec<u8>,
        bucket: u64,
    ) -> Result<Option<RoaringBitmap>, Error> {
        let Some(stored) = self.event_bitmap.get(&Key {
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
    pub fn iter_event_bitmap_buckets(
        &self,
        dimension_key: Vec<u8>,
    ) -> Result<Iter<'_, Key, Value>, Error> {
        self.event_bitmap
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
/// consecutive packed event sequences compresses well.
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
    fn pack_bucket_and_bit_math() {
        // tx_seq=0, event_idx=0 → packed 0 → bucket 0 / bit 0.
        let p = pack(0, 0);
        assert_eq!(p, 0);
        assert_eq!(bucket_of(p), 0);
        assert_eq!(bit_of(p), 0);

        // tx_seq=1, event_idx=0 → packed `1 << 16` = 65_536.
        let p = pack(1, 0);
        assert_eq!(p, 1 << EVENT_BITS);
        assert_eq!(bucket_of(p), 0);
        assert_eq!(bit_of(p), 1 << EVENT_BITS);

        // The first packed value of the next bucket sits at the
        // boundary `EVENT_BUCKET_SIZE` — that's
        // `EVENT_BUCKET_SIZE >> EVENT_BITS = 4096` transactions in.
        let first_in_next_bucket = pack(EVENT_BUCKET_SIZE >> EVENT_BITS, 0);
        assert_eq!(first_in_next_bucket, EVENT_BUCKET_SIZE);
        assert_eq!(bucket_of(first_in_next_bucket), 1);
        assert_eq!(bit_of(first_in_next_bucket), 0);
    }

    #[test]
    fn get_returns_none_for_unknown_bucket() {
        let (_dir, _db, schema) = fresh_db();
        assert!(
            schema
                .get_event_bitmap(b"emitting_module:coin".to_vec(), 0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn single_match_round_trips_through_merge() {
        let (_dir, db, schema) = fresh_db();
        let (k, v) = store_match(b"emitting_module:coin".to_vec(), 42, 3);

        let mut batch = db.batch();
        batch.merge(&schema.event_bitmap, &k, &v).unwrap();
        batch.commit().unwrap();

        let packed = pack(42, 3);
        let bitmap = schema
            .get_event_bitmap(b"emitting_module:coin".to_vec(), bucket_of(packed))
            .unwrap()
            .expect("bitmap present");
        let bits: Vec<u32> = bitmap.iter().collect();
        assert_eq!(bits, vec![bit_of(packed)]);
    }

    #[test]
    fn many_matches_in_one_bucket_union() {
        let (_dir, db, schema) = fresh_db();
        let dim = b"emitting_module:coin".to_vec();
        let entries: Vec<(u64, u32)> = vec![(1, 0), (1, 7), (2, 0), (5, 12)];

        let mut batch = db.batch();
        for (tx, idx) in &entries {
            let (k, v) = store_match(dim.clone(), *tx, *idx);
            batch.merge(&schema.event_bitmap, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let bitmap = schema
            .get_event_bitmap(dim, 0)
            .unwrap()
            .expect("bitmap present");
        let bits: BTreeSet<u32> = bitmap.iter().collect();
        let expected: BTreeSet<u32> = entries
            .iter()
            .map(|(tx, idx)| bit_of(pack(*tx, *idx)))
            .collect();
        assert_eq!(bits, expected);
    }

    #[test]
    fn distinct_dimensions_do_not_alias() {
        let (_dir, db, schema) = fresh_db();
        let (k_a, v_a) = store_match(b"emitting_module:coin".to_vec(), 42, 1);
        let (k_b, v_b) = store_match(b"emitting_module:nft".to_vec(), 100, 2);
        let mut batch = db.batch();
        batch.merge(&schema.event_bitmap, &k_a, &v_a).unwrap();
        batch.merge(&schema.event_bitmap, &k_b, &v_b).unwrap();
        batch.commit().unwrap();

        let coin = schema
            .get_event_bitmap(b"emitting_module:coin".to_vec(), 0)
            .unwrap()
            .unwrap();
        let nft = schema
            .get_event_bitmap(b"emitting_module:nft".to_vec(), 0)
            .unwrap()
            .unwrap();
        assert_eq!(coin.iter().collect::<Vec<u32>>(), vec![bit_of(pack(42, 1))]);
        assert_eq!(nft.iter().collect::<Vec<u32>>(), vec![bit_of(pack(100, 2))]);
    }

    #[test]
    fn should_remove_bucket_drops_only_fully_pruned_ranges() {
        let dim = b"emitting_module:coin";
        let bucket_0_key = Key {
            dimension_key: dim.to_vec(),
            bucket: 0,
        }
        .encode()
        .unwrap();

        // Floor 0 → nothing pruned.
        assert!(!should_remove_bucket(&bucket_0_key, 0));

        // EVENT_BUCKET_SIZE in packed-event-seq space corresponds
        // to `EVENT_BUCKET_SIZE >> EVENT_BITS` transactions —
        // anything below that tx_seq floor keeps bucket 0 alive.
        let txs_per_bucket = EVENT_BUCKET_SIZE >> EVENT_BITS;
        assert!(!should_remove_bucket(&bucket_0_key, txs_per_bucket - 1));
        // At the tx_seq floor that translates to exactly
        // EVENT_BUCKET_SIZE in packed space, bucket 0 becomes
        // fully pruned.
        assert!(should_remove_bucket(&bucket_0_key, txs_per_bucket));

        // Bucket 5 needs floor past 6 * EVENT_BUCKET_SIZE in
        // packed space, i.e. tx_seq past 6 * txs_per_bucket.
        let bucket_5_key = Key {
            dimension_key: dim.to_vec(),
            bucket: 5,
        }
        .encode()
        .unwrap();
        assert!(!should_remove_bucket(&bucket_5_key, 6 * txs_per_bucket - 1));
        assert!(should_remove_bucket(&bucket_5_key, 6 * txs_per_bucket));

        // Key too short → kept.
        assert!(!should_remove_bucket(&[0u8; 4], u64::MAX));
    }

    #[test]
    fn packed_pruning_floor_saturates_on_overflow() {
        assert_eq!(packed_pruning_floor(0), 0);
        assert_eq!(packed_pruning_floor(1), 1u64 << EVENT_BITS);
        // Just below the overflow threshold.
        let just_below = (1u64 << (64 - EVENT_BITS)) - 1;
        assert_eq!(packed_pruning_floor(just_below), just_below << EVENT_BITS,);
        // At the threshold — `tx_seq << EVENT_BITS` would
        // overflow, so we saturate.
        assert_eq!(packed_pruning_floor(1u64 << (64 - EVENT_BITS)), u64::MAX);
        assert_eq!(packed_pruning_floor(u64::MAX), u64::MAX);
    }

    #[test]
    fn iter_walks_buckets_for_one_dimension_in_order() {
        let (_dir, db, schema) = fresh_db();
        let dim = b"emitting_module:coin".to_vec();
        let other = b"emitting_module:nft".to_vec();
        // Three events whose packed seqs land in distinct
        // buckets: bucket 0, bucket 1 (just past 4096 txs), and
        // bucket 3.
        let txs_per_bucket = EVENT_BUCKET_SIZE >> EVENT_BITS;
        let tx_seqs = [0u64, txs_per_bucket + 5, 3 * txs_per_bucket + 9];

        let mut batch = db.batch();
        for tx in tx_seqs {
            let (k, v) = store_match(dim.clone(), tx, 0);
            batch.merge(&schema.event_bitmap, &k, &v).unwrap();
        }
        // Unrelated dimension — must not appear in our iter.
        let (k_other, v_other) = store_match(other, 0, 0);
        batch
            .merge(&schema.event_bitmap, &k_other, &v_other)
            .unwrap();
        batch.commit().unwrap();

        let buckets: Vec<u64> = schema
            .iter_event_bitmap_buckets(dim)
            .unwrap()
            .map(|res| res.unwrap().0.bucket)
            .collect();
        assert_eq!(buckets, vec![0, 1, 3]);
    }
}

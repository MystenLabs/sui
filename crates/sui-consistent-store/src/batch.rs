// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Atomic write batches.
//!
//! [`Batch`] accumulates put, delete, and merge operations across one
//! or more column families and applies them atomically when
//! [`Batch::commit`] is called. RocksDB guarantees that the
//! operations in a single batch either all become visible to readers
//! or none do.
//!
//! Batches are constructed from a [`Db`] via [`Db::batch`]. Each
//! operation takes a [`DbMap`] handle whose key and value types are
//! encoded into bytes using the crate's encoding traits before being
//! handed to the underlying [`rocksdb::WriteBatch`].
//!
//! # Merge operations
//!
//! [`Batch::merge`] stages a merge operand against a key. The
//! merge operator that combines the operand with any existing value
//! is configured by the schema author on the column family's
//! [`rocksdb::Options`] (returned from
//! [`Schema::cfs`](crate::Schema::cfs)) at open time. RocksDB
//! applies the operator lazily at read or compaction time; this
//! crate simply forwards the bytes.
//!
//! # Examples
//!
//! ```
//! use sui_consistent_store::Db;
//! use sui_consistent_store::DbMap;
//! use sui_consistent_store::DbOptions;
//! use bytes::Buf;
//! use bytes::BufMut;
//!
//! use sui_consistent_store::Decode;
//! use sui_consistent_store::Encode;
//! use sui_consistent_store::Schema;
//! use sui_consistent_store::error::DecodeError;
//! use sui_consistent_store::error::EncodeError;
//! use sui_consistent_store::error::OpenError;
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct U64Be(u64);
//!
//! impl Encode for U64Be {
//!     fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
//!         buf.put_slice(&self.0.to_be_bytes());
//!         Ok(())
//!     }
//! }
//!
//! impl Decode for U64Be {
//!     fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
//!         if buf.remaining() != 8 {
//!             return Err(DecodeError::msg("expected 8 bytes"));
//!         }
//!         Ok(Self(buf.get_u64()))
//!     }
//! }
//!
//! struct MySchema {
//!     items: DbMap<U64Be, U64Be>,
//! }
//!
//! impl Schema for MySchema {
//!     fn cfs(opts: &sui_consistent_store::CfOptionsResolver) -> Vec<sui_consistent_store::CfDescriptor> {
//!         vec![sui_consistent_store::CfDescriptor::new("items", opts.options("items"))]
//!     }
//!
//!     fn open(db: &Db) -> Result<Self, OpenError> {
//!         Ok(Self {
//!             items: DbMap::new(db.clone(), "items")?,
//!         })
//!     }
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let (db, schema) = Db::open::<MySchema>(dir.path(), DbOptions::default()).unwrap();
//!
//! let mut batch = db.batch();
//! batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
//! batch.put(&schema.items, &U64Be(2), &U64Be(200)).unwrap();
//! batch.commit().unwrap();
//!
//! assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(100)));
//! ```

use std::fmt;

use crate::Encode;
use crate::db::Db;
use crate::encode_buf::with_encode_buf;
use crate::error::Error;
use crate::map::DbMap;

/// An accumulating, typed write batch.
///
/// `Batch` wraps a [`rocksdb::WriteBatch`] and stages put, delete,
/// and merge operations against typed [`DbMap`] handles. The keys
/// and values are encoded into bytes via the crate's encoding
/// traits before being handed to RocksDB.
///
/// All staged operations either become visible together or not at
/// all when [`commit`](Self::commit) succeeds. Encoding failures
/// during staging propagate as
/// [`crate::error::Error::Encode`]; the underlying
/// RocksDB write can fail with
/// [`crate::error::Error::Rocksdb`] at commit time.
pub struct Batch {
    db: Db,
    inner: rocksdb::WriteBatch,
}

impl Batch {
    pub(crate) fn new(db: Db) -> Self {
        Self {
            db,
            inner: rocksdb::WriteBatch::default(),
        }
    }

    /// Stage a put on the column family backing `map`.
    ///
    /// The key and value are encoded into a thread-local scratch
    /// buffer once per call; RocksDB copies the bytes into the
    /// batch's internal representation synchronously, so the
    /// scratch buffer is free for reuse on return.
    ///
    /// `map` is constrained to a [`Db`]-bound handle: writes always
    /// go to the live tip, and snapshot-bound projections (or
    /// borrowed-`&Db` projections) are statically refused.
    pub fn put<K, V>(
        &mut self,
        map: &DbMap<K, V, Db>,
        key: &K,
        value: &V,
    ) -> Result<&mut Self, Error>
    where
        K: Encode,
        V: Encode,
    {
        let cf = map
            .db()
            .cf_handle(map.cf_name())
            .ok_or_else(|| Error::MissingColumnFamily(map.cf_name().to_string()))?;
        with_encode_buf(|buf| -> Result<(), Error> {
            key.encode_into(buf)?;
            let k_end = buf.len();
            value.encode_into(buf)?;
            let bytes = buf.as_slice();
            self.inner.put_cf(&cf, &bytes[..k_end], &bytes[k_end..]);
            Ok(())
        })?;
        Ok(self)
    }

    /// Stage a delete on the column family backing `map`.
    ///
    /// `map` is constrained to a [`Db`]-bound handle.
    pub fn delete<K, V>(&mut self, map: &DbMap<K, V, Db>, key: &K) -> Result<&mut Self, Error>
    where
        K: Encode,
    {
        let cf = map
            .db()
            .cf_handle(map.cf_name())
            .ok_or_else(|| Error::MissingColumnFamily(map.cf_name().to_string()))?;
        with_encode_buf(|buf| -> Result<(), Error> {
            key.encode_into(buf)?;
            self.inner.delete_cf(&cf, buf.as_slice());
            Ok(())
        })?;
        Ok(self)
    }

    /// Stage a range delete on the column family backing `map`,
    /// removing every key in `[from, to_exclusive)`.
    ///
    /// Both bounds are encoded once into a thread-local scratch
    /// buffer and forwarded to
    /// [`rocksdb::WriteBatch::delete_range_cf`], which records a
    /// single range tombstone rather than one tombstone per key —
    /// cheap to stage regardless of how many keys fall in the range.
    ///
    /// The upper bound is *exclusive*: a key equal to `to_exclusive`
    /// is retained. Reads honor the range tombstone immediately,
    /// because this crate leaves RocksDB's `ignore_range_deletions`
    /// at its default of `false`.
    ///
    /// The range is interpreted over the encoded byte ordering, so a
    /// meaningful range delete requires `K`'s [`Encode`] to be
    /// order-preserving (e.g. big-endian fixed-width integers). A
    /// `from` that encodes to bytes greater than `to_exclusive`
    /// deletes nothing.
    ///
    /// `map` is constrained to a [`Db`]-bound handle.
    pub fn delete_range<K, V>(
        &mut self,
        map: &DbMap<K, V, Db>,
        from: &K,
        to_exclusive: &K,
    ) -> Result<&mut Self, Error>
    where
        K: Encode,
    {
        let cf = map
            .db()
            .cf_handle(map.cf_name())
            .ok_or_else(|| Error::MissingColumnFamily(map.cf_name().to_string()))?;
        with_encode_buf(|buf| -> Result<(), Error> {
            from.encode_into(buf)?;
            let from_end = buf.len();
            to_exclusive.encode_into(buf)?;
            let bytes = buf.as_slice();
            self.inner
                .delete_range_cf(&cf, &bytes[..from_end], &bytes[from_end..]);
            Ok(())
        })?;
        Ok(self)
    }

    /// Stage a merge operand on the column family backing `map`.
    ///
    /// The encoded `operand` bytes are passed to the merge operator
    /// the schema registered on this column family's
    /// [`rocksdb::Options`] at open time. The operator combines the
    /// operand with any existing value at `key` lazily, at the next
    /// read or during compaction.
    ///
    /// `operand` is constrained to the column family's value type
    /// `V`. Schemas whose merge semantics expect a different operand
    /// type than the stored value should encode the operand into a
    /// wrapper that round-trips through the same `Encode`
    /// implementation (or split the column family).
    ///
    /// If the column family has no merge operator configured,
    /// RocksDB rejects the batch at [`commit`](Self::commit) time.
    ///
    /// `map` is constrained to a [`Db`]-bound handle.
    pub fn merge<K, V>(
        &mut self,
        map: &DbMap<K, V, Db>,
        key: &K,
        operand: &V,
    ) -> Result<&mut Self, Error>
    where
        K: Encode,
        V: Encode,
    {
        let cf = map
            .db()
            .cf_handle(map.cf_name())
            .ok_or_else(|| Error::MissingColumnFamily(map.cf_name().to_string()))?;
        with_encode_buf(|buf| -> Result<(), Error> {
            key.encode_into(buf)?;
            let k_end = buf.len();
            operand.encode_into(buf)?;
            let bytes = buf.as_slice();
            self.inner.merge_cf(&cf, &bytes[..k_end], &bytes[k_end..]);
            Ok(())
        })?;
        Ok(self)
    }

    /// Commit the staged operations atomically.
    ///
    /// Consumes `self`. On success, all staged operations are
    /// visible to subsequent reads. On failure, the database is
    /// left in the state it was in before the commit was attempted.
    pub fn commit(self) -> Result<(), Error> {
        self.db.rocksdb().write(self.inner)?;
        Ok(())
    }

    /// Commit the staged operations atomically, with caller-supplied
    /// [`rocksdb::WriteOptions`].
    ///
    /// Useful for tuning write durability and WAL behavior on a
    /// per-batch basis (for example, disabling the WAL during a
    /// bulk load, or forcing an `fsync` on a critical commit).
    /// Defaults are appropriate for routine writes; consult the
    /// RocksDB docs for trade-offs.
    pub fn commit_opt(self, opts: rocksdb::WriteOptions) -> Result<(), Error> {
        self.db.rocksdb().write_opt(self.inner, &opts)?;
        Ok(())
    }

    /// Returns whether the batch has no staged operations.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the number of staged operations.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns the serialized size in bytes of the staged operations.
    pub fn size_in_bytes(&self) -> usize {
        self.inner.size_in_bytes()
    }
}

impl fmt::Debug for Batch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `rocksdb::WriteBatch` does not implement Debug, so
        // summarize.
        f.debug_struct("Batch")
            .field("ops", &self.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::DbOptions;
    use crate::Decode;
    use crate::Schema;
    use bytes::BufMut;

    use crate::error::DecodeError;
    use crate::error::EncodeError;
    use crate::error::OpenError;

    /// Hand-rolled big-endian `u64` for tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64Be(u64);

    impl Encode for U64Be {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for U64Be {
        fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg("expected 8 bytes"));
            }
            Ok(Self(buf.get_u64()))
        }
    }

    /// Type whose encode always fails. Used to assert that batch
    /// staging propagates encoding errors out of the closure that
    /// holds the thread-local scratch buffer.
    #[derive(Debug)]
    struct AlwaysFails;

    impl Encode for AlwaysFails {
        fn encode_into<B: BufMut>(&self, _: &mut B) -> Result<(), EncodeError> {
            Err(EncodeError::msg("always fails"))
        }
    }

    impl Decode for AlwaysFails {
        fn decode<B: bytes::Buf>(_: &mut B) -> Result<Self, DecodeError> {
            Err(DecodeError::msg("never reached"))
        }
    }

    #[derive(Debug)]
    struct TestSchema {
        items: DbMap<U64Be, U64Be>,
        other: DbMap<U64Be, U64Be>,
    }

    impl Schema for TestSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            vec![
                crate::CfDescriptor::new("items", opts.options("items")),
                crate::CfDescriptor::new("other", opts.options("other")),
            ]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                items: DbMap::new(db.clone(), "items")?,
                other: DbMap::new(db.clone(), "other")?,
            })
        }
    }

    fn open() -> (TempDir, Db, TestSchema) {
        let dir = TempDir::new().unwrap();
        let (db, schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    #[test]
    fn put_then_get_round_trips() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(100)));
    }

    #[test]
    fn delete_removes_existing_key() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
        batch.commit().unwrap();
        let mut batch = db.batch();
        batch.delete(&schema.items, &U64Be(1)).unwrap();
        batch.commit().unwrap();
        assert!(schema.items.get(&U64Be(1)).unwrap().is_none());
    }

    #[test]
    fn put_then_delete_in_same_batch_results_in_absent() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
        batch.delete(&schema.items, &U64Be(1)).unwrap();
        batch.commit().unwrap();
        assert!(schema.items.get(&U64Be(1)).unwrap().is_none());
    }

    #[test]
    fn delete_range_removes_keys_in_half_open_range() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        for k in 1..=5 {
            batch.put(&schema.items, &U64Be(k), &U64Be(k * 10)).unwrap();
        }
        batch.commit().unwrap();

        // Delete [2, 4): removes 2 and 3, keeps 1, 4, and 5. The
        // upper bound is exclusive.
        let mut batch = db.batch();
        batch
            .delete_range(&schema.items, &U64Be(2), &U64Be(4))
            .unwrap();
        batch.commit().unwrap();

        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
        assert!(schema.items.get(&U64Be(2)).unwrap().is_none());
        assert!(schema.items.get(&U64Be(3)).unwrap().is_none());
        assert_eq!(schema.items.get(&U64Be(4)).unwrap(), Some(U64Be(40)));
        assert_eq!(schema.items.get(&U64Be(5)).unwrap(), Some(U64Be(50)));
    }

    #[test]
    fn delete_range_is_honored_by_iteration() {
        // Range tombstones must be visible to scans immediately —
        // this crate leaves `ignore_range_deletions` at false.
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        for k in 0..10 {
            batch.put(&schema.items, &U64Be(k), &U64Be(k)).unwrap();
        }
        batch.commit().unwrap();

        let mut batch = db.batch();
        batch
            .delete_range(&schema.items, &U64Be(0), &U64Be(7))
            .unwrap();
        batch.commit().unwrap();

        let remaining: Vec<u64> = schema
            .items
            .iter(..)
            .unwrap()
            .map(|res| res.unwrap().0.0)
            .collect();
        assert_eq!(remaining, vec![7, 8, 9]);
    }

    #[test]
    fn delete_range_empty_range_is_a_no_op() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(3), &U64Be(30)).unwrap();
        batch.commit().unwrap();

        // from == to_exclusive deletes nothing.
        let mut batch = db.batch();
        batch
            .delete_range(&schema.items, &U64Be(3), &U64Be(3))
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.items.get(&U64Be(3)).unwrap(), Some(U64Be(30)));
    }

    #[test]
    fn delete_range_propagates_encode_error_for_bound() {
        let (_dir, db, _schema) = open();
        let bad: DbMap<AlwaysFails, U64Be> = DbMap::new(db.clone(), "items").unwrap();
        let mut batch = db.batch();
        let err = batch
            .delete_range(&bad, &AlwaysFails, &AlwaysFails)
            .unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn empty_batch_commits_without_error() {
        let (_dir, db, _schema) = open();
        db.batch().commit().unwrap();
    }

    #[test]
    fn empty_batch_observability() {
        let (_dir, db, _schema) = open();
        let batch = db.batch();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        // The WriteBatch carries a small fixed header even when
        // empty; what matters here is that adding operations grows
        // the size. See `populated_batch_observability`.
    }

    #[test]
    fn populated_batch_observability() {
        let (_dir, db, schema) = open();
        let empty_size = db.batch().size_in_bytes();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(10)).unwrap();
        batch.put(&schema.items, &U64Be(2), &U64Be(20)).unwrap();
        batch.delete(&schema.items, &U64Be(3)).unwrap();
        assert!(!batch.is_empty());
        assert_eq!(batch.len(), 3);
        assert!(batch.size_in_bytes() > empty_size);
    }

    #[test]
    fn commit_opt_respects_disable_wal_flag() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(10)).unwrap();
        let mut wopts = rocksdb::WriteOptions::default();
        wopts.disable_wal(true);
        batch.commit_opt(wopts).unwrap();
        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
    }

    #[test]
    fn batch_spans_multiple_cfs_atomically() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(100)).unwrap();
        batch.put(&schema.other, &U64Be(2), &U64Be(200)).unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(100)));
        assert_eq!(schema.other.get(&U64Be(2)).unwrap(), Some(U64Be(200)));
    }

    #[test]
    fn put_chains_via_mut_self() {
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch
            .put(&schema.items, &U64Be(1), &U64Be(10))
            .unwrap()
            .put(&schema.items, &U64Be(2), &U64Be(20))
            .unwrap()
            .delete(&schema.items, &U64Be(2))
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.items.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
        assert!(schema.items.get(&U64Be(2)).unwrap().is_none());
    }

    #[test]
    fn put_propagates_encode_error_for_key() {
        let (_dir, db, _schema) = open();
        // A throwaway DbMap whose key type always fails to encode.
        let bad: DbMap<AlwaysFails, U64Be> = DbMap::new(db.clone(), "items").unwrap();
        let mut batch = db.batch();
        let err = batch.put(&bad, &AlwaysFails, &U64Be(1)).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn put_propagates_encode_error_for_value() {
        let (_dir, db, _schema) = open();
        let bad: DbMap<U64Be, AlwaysFails> = DbMap::new(db.clone(), "items").unwrap();
        let mut batch = db.batch();
        let err = batch.put(&bad, &U64Be(1), &AlwaysFails).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn delete_propagates_encode_error_for_key() {
        let (_dir, db, _schema) = open();
        let bad: DbMap<AlwaysFails, U64Be> = DbMap::new(db.clone(), "items").unwrap();
        let mut batch = db.batch();
        let err = batch.delete(&bad, &AlwaysFails).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    /// Associative merge operator that interprets each operand and
    /// the existing value (if any) as eight big-endian bytes, sums
    /// them with saturation, and writes the result back in the same
    /// format. Operands and missing values that aren't exactly
    /// eight bytes are skipped.
    fn add_u64_merge_op(
        _key: &[u8],
        existing: Option<&[u8]>,
        operands: &rocksdb::MergeOperands,
    ) -> Option<Vec<u8>> {
        let mut total: u64 = existing
            .and_then(|b| <[u8; 8]>::try_from(b).ok())
            .map(u64::from_be_bytes)
            .unwrap_or(0);
        for op in operands {
            if let Ok(arr) = <[u8; 8]>::try_from(op) {
                total = total.saturating_add(u64::from_be_bytes(arr));
            }
        }
        Some(total.to_be_bytes().to_vec())
    }

    #[derive(Debug)]
    struct MergeSchema {
        counters: DbMap<U64Be, U64Be>,
    }

    impl Schema for MergeSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            let mut counter_opts = opts.options("counters");
            counter_opts.set_merge_operator_associative("u64-add", add_u64_merge_op);
            vec![crate::CfDescriptor::new("counters", counter_opts)]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                counters: DbMap::new(db.clone(), "counters")?,
            })
        }
    }

    fn open_merge() -> (TempDir, Db, MergeSchema) {
        let dir = TempDir::new().unwrap();
        let (db, schema) = Db::open::<MergeSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    #[test]
    fn merge_aggregates_via_registered_operator() {
        let (_dir, db, schema) = open_merge();
        let mut batch = db.batch();
        batch
            .merge(&schema.counters, &U64Be(1), &U64Be(10))
            .unwrap();
        batch
            .merge(&schema.counters, &U64Be(1), &U64Be(20))
            .unwrap();
        batch.merge(&schema.counters, &U64Be(1), &U64Be(7)).unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.counters.get(&U64Be(1)).unwrap(), Some(U64Be(37)));
    }

    #[test]
    fn merge_into_empty_key_starts_from_zero() {
        let (_dir, db, schema) = open_merge();
        let mut batch = db.batch();
        batch
            .merge(&schema.counters, &U64Be(1), &U64Be(42))
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.counters.get(&U64Be(1)).unwrap(), Some(U64Be(42)));
    }

    #[test]
    fn merge_combines_with_prior_put() {
        let (_dir, db, schema) = open_merge();
        let mut batch = db.batch();
        batch.put(&schema.counters, &U64Be(1), &U64Be(100)).unwrap();
        batch.commit().unwrap();
        let mut batch = db.batch();
        batch
            .merge(&schema.counters, &U64Be(1), &U64Be(50))
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.counters.get(&U64Be(1)).unwrap(), Some(U64Be(150)));
    }

    #[test]
    fn merge_independent_keys_do_not_combine() {
        let (_dir, db, schema) = open_merge();
        let mut batch = db.batch();
        batch
            .merge(&schema.counters, &U64Be(1), &U64Be(10))
            .unwrap();
        batch
            .merge(&schema.counters, &U64Be(2), &U64Be(20))
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(schema.counters.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
        assert_eq!(schema.counters.get(&U64Be(2)).unwrap(), Some(U64Be(20)));
    }

    #[test]
    fn merge_propagates_encode_error_for_key() {
        let (_dir, db, _schema) = open_merge();
        let bad: DbMap<AlwaysFails, U64Be> = DbMap::new(db.clone(), "counters").unwrap();
        let mut batch = db.batch();
        let err = batch.merge(&bad, &AlwaysFails, &U64Be(1)).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn merge_propagates_encode_error_for_operand() {
        let (_dir, db, _schema) = open_merge();
        let bad: DbMap<U64Be, AlwaysFails> = DbMap::new(db.clone(), "counters").unwrap();
        let mut batch = db.batch();
        let err = batch.merge(&bad, &U64Be(1), &AlwaysFails).unwrap_err();
        assert!(matches!(err, Error::Encode(_)));
    }

    #[test]
    fn merge_without_operator_errors_at_commit() {
        // The default `items` CF in `TestSchema` does not have a
        // merge operator. RocksDB rejects merges on it at write
        // time.
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.merge(&schema.items, &U64Be(1), &U64Be(10)).unwrap();
        let err = batch.commit().unwrap_err();
        assert!(matches!(err, Error::Rocksdb(_)));
    }
}

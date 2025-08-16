// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds, sync::Arc};

use bincode::{Decode, Encode};
use serde::{de::DeserializeOwned, Serialize};

use super::{error::Error, iter, key, Db};

/// A structured representation of a single RocksDB column family, providing snapshot-based reads
/// and a transactional write API.
pub(crate) struct DbMap<K, V> {
    db: Arc<Db>,
    cf: String,
    _data: PhantomData<fn(K) -> V>,
}

impl<K, V> DbMap<K, V>
where
    K: Encode + Decode<()>,
    V: Serialize + DeserializeOwned,
{
    /// Open a new `DbMap` for the column family `cf` in database `db`.
    pub(crate) fn new(db: Arc<Db>, cf: impl Into<String>) -> Self {
        Self {
            db,
            cf: cf.into(),
            _data: PhantomData,
        }
    }

    /// Point look-up at `checkpoint` for the given `key`.
    ///
    /// Fails if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn get(&self, checkpoint: u64, key: impl Borrow<K>) -> Result<Option<V>, Error> {
        self.db.get(checkpoint, &self.cf()?, key.borrow())
    }

    /// Multi-point look-up at `checkpoint` for the given `key`.
    ///
    /// Fails if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn multi_get<'k, J: Borrow<K> + 'k>(
        &self,
        checkpoint: u64,
        keys: impl IntoIterator<Item = &'k J>,
    ) -> Result<Vec<Result<Option<V>, Error>>, Error> {
        let keys = keys.into_iter().map(|k| k.borrow());
        self.db.multi_get(checkpoint, &self.cf()?, keys)
    }

    /// Create a forward iterator over the values in the map at the given `checkpoint`, optionally
    /// bounding the keys on either side by the given `range`. A forward iterator yields keys in
    /// ascending bincoded lexicographic order.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn iter(
        &self,
        checkpoint: u64,
        range: impl RangeBounds<K>,
    ) -> Result<iter::FwdIter<'_, K, V>, Error> {
        self.db.iter(checkpoint, &self.cf()?, range)
    }

    /// Create a reverse iterator over the values in the map at the given `checkpoint`, optionally
    /// bounding the keys on either side by the given `range`. A reverse iterator yields keys in
    /// descending bincoded lexicographic order.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn iter_rev(
        &self,
        checkpoint: u64,
        range: impl RangeBounds<K>,
    ) -> Result<iter::RevIter<'_, K, V>, Error> {
        self.db.iter_rev(checkpoint, &self.cf()?, range)
    }

    /// Create a forward iterator over the values in the map at the given `checkpoint`, where all
    /// the keys start with the given `prefix`. A forward iterator yields keys in ascending
    /// bincoded lexicographic order, and the predicate is applied on the bincoded key and the
    /// bincoded prefix.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn prefix(
        &self,
        checkpoint: u64,
        prefix: &impl Encode,
    ) -> Result<iter::FwdIter<'_, K, V>, Error> {
        self.db.prefix(checkpoint, &self.cf()?, prefix)
    }

    /// Create a reverse iterator over the values in the map at the given `checkpoint`, where all
    /// the keys start with the gven `prefix`. A reverse iterator yields keys in descending
    /// bincoded lexicographic order, and the predicate is applied on the bincoded key and the
    /// bincoded prefix.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn prefix_rev(
        &self,
        checkpoint: u64,
        prefix: &impl Encode,
    ) -> Result<iter::RevIter<'_, K, V>, Error> {
        self.db.prefix_rev(checkpoint, &self.cf()?, prefix)
    }

    /// Record the insertion of `k -> v` for the map's column family in the given `batch`. The
    /// write is not performed until the batch is written to the database, and its effects will not
    /// be visible until a snapshot is created after the batch is written.
    pub(crate) fn insert(
        &self,
        k: impl Borrow<K>,
        v: impl Borrow<V>,
        batch: &mut rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        batch.put_cf(
            &self.cf()?,
            key::encode(k.borrow()),
            bcs::to_bytes(v.borrow())?,
        );
        Ok(())
    }

    /// Add `v` as a merge operand to `k` for the map's column family in the given `batch`. The
    /// write is not performed until the batch is written to the database, and its effects will not
    /// be visible until a snapshot is created after the batch is written.
    ///
    /// ## Safety
    ///
    /// It is an error to add a merge operand to a column family that does not have a merge
    /// operator configured. This error will only be raised when the batch is written, and from
    /// that point all writes to that key and column family will fail because of the pending merge.
    pub(crate) fn merge(
        &self,
        k: impl Borrow<K>,
        v: impl Borrow<V>,
        batch: &mut rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        batch.merge_cf(
            &self.cf()?,
            key::encode(k.borrow()),
            bcs::to_bytes(v.borrow())?,
        );
        Ok(())
    }

    /// Record the removal of `k` from the map's column family in the given `batch`. The removal is
    /// not performed until the batch is written to the database, and its effects will not be
    /// visible until a snapshot is created after the batch is written.
    pub(crate) fn remove(
        &self,
        k: impl Borrow<K>,
        batch: &mut rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        batch.delete_cf(&self.cf()?, key::encode(k.borrow()));
        Ok(())
    }

    fn cf(&self) -> Result<Arc<rocksdb::BoundColumnFamily<'_>>, Error> {
        self.db
            .cf(&self.cf)
            .ok_or_else(|| Error::NoColumnFamily(self.cf.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::{tests::wm, Db};

    #[test]
    fn test_no_such_column_family() {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, vec![]).unwrap());
        let map: DbMap<u64, u64> = DbMap::new(db.clone(), "test");

        // Trying to access a column family that does not exist should return an error.
        let Err(err) = map.get(0, 42) else {
            panic!("expected error, got Ok");
        };

        assert!(matches!(err, Error::NoColumnFamily(_)))
    }

    #[test]
    fn test_column_family_deleted() {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let cfs = vec![("test", rocksdb::Options::default())];

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, cfs).unwrap());
        let map: DbMap<u64, u64> = DbMap::new(db.clone(), "test");
        db.snapshot(0);

        // Access succeeds at first, but will start to fail after the column family is dropped.
        assert!(map.get(0, 42).unwrap().is_none());

        db.drop_cf("test").unwrap();
        let Err(err) = map.get(0, 42) else {
            panic!("expected error, got Ok");
        };

        assert!(matches!(err, Error::NoColumnFamily(_)))
    }

    #[test]
    fn test_insert_remove() {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let cfs = vec![("test", rocksdb::Options::default())];

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, cfs).unwrap());
        let map: DbMap<u64, u64> = DbMap::new(db.clone(), "test");

        let mut batch = rocksdb::WriteBatch::default();
        map.insert(42, 43, &mut batch).unwrap();
        map.insert(44, 45, &mut batch).unwrap();
        db.write("batch", wm(0), batch).unwrap();
        db.snapshot(0);

        let mut batch = rocksdb::WriteBatch::default();
        map.remove(42, &mut batch).unwrap();
        map.insert(43, 42, &mut batch).unwrap();
        db.write("batch", wm(1), batch).unwrap();
        db.snapshot(1);

        // Point look-ups
        assert_eq!(map.get(0, 42).unwrap(), Some(43));
        assert_eq!(map.get(0, 43).unwrap(), None);
        assert_eq!(map.get(0, 44).unwrap(), Some(45));

        // Multi-gets
        assert_eq!(
            map.multi_get(0, &[42, 43, 44])
                .unwrap()
                .into_iter()
                .map(|r| r.unwrap())
                .collect::<Vec<_>>(),
            vec![Some(43), None, Some(45)],
        );

        // Iteration
        assert_eq!(
            map.iter(0, ..)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(42, 43), (44, 45)],
        );

        assert_eq!(
            map.iter_rev(0, ..)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(44, 45), (42, 43)],
        );

        // Point look-ups
        assert_eq!(map.get(1, 42).unwrap(), None);
        assert_eq!(map.get(1, 43).unwrap(), Some(42));
        assert_eq!(map.get(1, 44).unwrap(), Some(45));

        // Multi-gets
        assert_eq!(
            map.multi_get(1, &[42, 43, 44])
                .unwrap()
                .into_iter()
                .map(|r| r.unwrap())
                .collect::<Vec<_>>(),
            vec![None, Some(42), Some(45)],
        );

        // Iteration
        assert_eq!(
            map.iter(1, ..)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(43, 42), (44, 45)],
        );

        assert_eq!(
            map.iter_rev(1, ..)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(44, 45), (43, 42)],
        );
    }

    #[test]
    fn test_merge() {
        let d = tempfile::tempdir().unwrap();

        let mut db_opts = rocksdb::Options::default();
        db_opts.create_if_missing(true);

        let mut counter_opts = rocksdb::Options::default();
        counter_opts.set_merge_operator_associative("add", |_, val, rands| {
            let mut sum = val.map_or(0u64, |v| bcs::from_bytes(v).unwrap());
            for rand in rands {
                sum += bcs::from_bytes::<u64>(rand).unwrap();
            }

            Some(bcs::to_bytes(&sum).unwrap())
        });

        let cfs = vec![("counts", counter_opts)];

        let db = Arc::new(Db::open(d.path().join("db"), db_opts, 4, cfs).unwrap());
        let counts: DbMap<u64, u64> = DbMap::new(db.clone(), "counts");

        // Successfully perform a merge to counts.
        let mut batch = rocksdb::WriteBatch::default();
        counts.merge(42, 1, &mut batch).unwrap();
        db.write("counts", wm(0), batch).unwrap();
        db.snapshot(0);

        // Merges allow for the accumulation of values.
        let mut batch = rocksdb::WriteBatch::default();
        counts.merge(42, 2, &mut batch).unwrap();
        db.write("counts", wm(1), batch).unwrap();
        db.snapshot(1);

        // A single batch can include multiple merges for the same key.
        let mut batch = rocksdb::WriteBatch::default();
        counts.merge(42, 3, &mut batch).unwrap();
        counts.merge(42, 4, &mut batch).unwrap();
        db.write("counts", wm(2), batch).unwrap();
        db.snapshot(2);

        assert_eq!(counts.get(0, 42).unwrap(), Some(1));
        assert_eq!(counts.get(1, 42).unwrap(), Some(3));
        assert_eq!(counts.get(2, 42).unwrap(), Some(10));
    }

    #[test]
    fn test_bad_merge() {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let cfs = vec![("test", rocksdb::Options::default())];

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, cfs).unwrap());
        let values: DbMap<u64, u64> = DbMap::new(db.clone(), "test");

        let mut batch = rocksdb::WriteBatch::default();
        values.insert(42, 1, &mut batch).unwrap();
        db.write("values", wm(0), batch).unwrap();

        // Trying to merge to a map that has not had a merge operator set-up will fail.
        let mut batch = rocksdb::WriteBatch::default();
        values.merge(42, 2, &mut batch).unwrap();
        assert!(db.write("values", wm(0), batch).is_err());

        // Subsequent writes will also fail, as the invalid merge has been written to the database.
        let mut batch = rocksdb::WriteBatch::default();
        values.insert(43, 3, &mut batch).unwrap();
        assert!(db.write("values", wm(1), batch).is_err());
    }

    #[test]
    fn test_prefix_iter() {
        let d = tempfile::tempdir().unwrap();

        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);

        let cfs = vec![("test", rocksdb::Options::default())];

        let db = Arc::new(Db::open(d.path().join("db"), opts, 4, cfs).unwrap());
        let map: DbMap<u32, u64> = DbMap::new(db.clone(), "test");

        let mut batch = rocksdb::WriteBatch::default();
        map.insert(0x0000_0001, 10, &mut batch).unwrap();
        map.insert(0xffff_0002, 20, &mut batch).unwrap();
        map.insert(0x0000_0003, 30, &mut batch).unwrap();
        map.insert(0xffff_0004, 40, &mut batch).unwrap();
        map.insert(0x0000_0005, 50, &mut batch).unwrap();
        db.write("batch", wm(0), batch).unwrap();
        db.snapshot(0);

        assert_eq!(
            map.prefix(0, &0x0000u16)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(0x0000_0001, 10), (0x0000_0003, 30), (0x0000_0005, 50)],
        );

        assert_eq!(
            map.prefix_rev(0, &0xffffu16)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(0xffff_0004, 40), (0xffff_0002, 20)],
        );
    }
}

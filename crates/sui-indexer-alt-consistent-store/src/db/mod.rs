// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    ops::{Bound, RangeBounds, RangeInclusive},
    path::Path,
    sync::{Arc, RwLock},
};

use anyhow::Context;
use bincode::Encode;
use rocksdb::AsColumnFamilyRef;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sui_indexer_alt_framework::store::CommitterWatermark;

use self::error::Error;

pub(crate) mod config;
mod error;
mod iter;
mod key;
pub(crate) mod map;

/// Name of the column family the database adds, to manage the checkpoint watermark.
const WATERMARK_CF: &str = "$watermark";

/// A wrapper around RocksDB that provides arbitrary writes and snapshot-based reads (reads must
/// specify the checkpoint they want to read from). Keys and values are encoded (using Bincode and
/// BCS respectively) to provide a type-safe API.
///
/// ## Watermarks and Atomicity
///
/// Every write is associated with a watermark -- the checkpoint that the write corresponds to and
/// a label for the pipeline that the write is associated with -- which is written atomically to
/// the database along with the write itself. This can be used to resume processing in the event of
/// a restart (planned or otherwise).
///
/// Checkpoint order per pipeline is the writer's responsibility -- the database does not enforce
/// monotonicity and only stores the latest checkpoint written per pipeline.
///
/// ## Snapshot Consistency
///
/// Snapshots can be taken on demand, and are associated with a checkpoint. They are stored in a
/// fixed size in-memory ordered buffer where the oldest (by checkpoint) snapshots are dropped to
/// ensure the buffer remains at capacity.
///
/// The snapshot buffer is empty once the database is first opened, meaning data reads will fail
/// until a snapshot is made, but watermark reads will always succeed.
///
/// It is the writer's responsibility to synchronize checkpoints in watermarks with checkpoints in
/// snapshots and otherwise maintain ordering. The database maintains snapshot order and a max
/// size, but does not require snapshots to be contiguous.
///
/// ## Persistence
///
/// Writes and watermarks persist between sessions, but snapshots do not.
///
/// ## Concurrency
///
/// Most of the Db's internals are held in a self-referential data structure, protected by a
/// read-write lock. This allows for concurrent reading, writing and snapshotting. Exclusive access
/// is only required to create a new snapshot, reads and writes to RocksDB can proceed
/// concurrently.
pub(crate) struct Db(RwLock<Inner>);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Watermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
}

/// Database internals in a self-referential struct that owns the database as well as handles from
/// that databases (for column families and snapshots). This data structure is not inherently
/// thread-safe, but access to it is protected by [`Db`]'s API.
#[ouroboros::self_referencing]
struct Inner {
    /// Maximum number of snapshots to keep in memory.
    capacity: usize,

    /// The underlying RocksDB database.
    db: rocksdb::DB,

    /// ColumnFamily in `db` that watermarks are written to.
    #[borrows(db)]
    #[covariant]
    watermark_cf: Arc<rocksdb::BoundColumnFamily<'this>>,

    /// Snapshots from `db`, ordered by checkpoint sequence number.
    #[borrows()]
    #[covariant]
    snapshots: BTreeMap<u64, Arc<rocksdb::Snapshot<'this>>>,
}

/// A raw iterator along with its encoded upper and lower bounds.
#[derive(Default)]
struct IterBounds<'d>(
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<rocksdb::DBRawIterator<'d>>,
);

impl Db {
    /// Open the database at `path`, with the given `capacity` for snapshots.
    ///
    /// `options` are passed to RocksDB to configure the database, and `cfs` denotes the column
    /// families to open. The database will inject its own column family for watermarks, and set
    /// the option to create missing column families.
    pub(crate) fn open<'c>(
        path: impl AsRef<Path>,
        mut options: rocksdb::Options,
        capacity: usize,
        cfs: impl IntoIterator<Item = (&'c str, rocksdb::Options)>,
    ) -> Result<Self, Error> {
        // Add a column family for watermarks, which are managed by the database.
        let mut cfs: Vec<_> = cfs.into_iter().collect();
        cfs.push((WATERMARK_CF, rocksdb::Options::default()));
        options.create_missing_column_families(true);

        let db = rocksdb::DB::open_cf_with_opts(&options, path, cfs)?;
        let inner = Inner::try_new(
            capacity,
            db,
            |db| db.cf_handle(WATERMARK_CF).context("WATERMARK_CF not found"),
            BTreeMap::new(),
        )?;

        Ok(Self(RwLock::new(inner)))
    }

    /// Write a batch of updates to the database atomically, along with a `watermark` with key
    /// `pipeline`.
    pub(crate) fn write(
        &self,
        pipeline: &str,
        watermark: Watermark,
        mut batch: rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        let checkpoint = bcs::to_bytes(&watermark).context("Failed to serialize watermark")?;

        let i = self.0.read().expect("poisoned");
        batch.put_cf(i.borrow_watermark_cf(), pipeline.as_bytes(), checkpoint);
        i.borrow_db().write(batch)?;
        Ok(())
    }

    /// Register a new snapshot at `checkpoint`. This could result in the oldest (by checkpoint)
    /// snapshot being dropped to ensure the number of snapshots remain at or below `capacity`.
    pub(crate) fn snapshot(&self, checkpoint: u64) {
        self.0.write().expect("poisoned").with_mut(|f| {
            f.snapshots.insert(checkpoint, Arc::new(f.db.snapshot()));
            if f.snapshots.len() > *f.capacity {
                f.snapshots.pop_first();
            }
        });
    }

    /// Return a handle for the column family with the given `name`, if it exists.
    pub(crate) fn cf(&self, name: &str) -> Option<Arc<rocksdb::BoundColumnFamily<'_>>> {
        let i = self.0.read().expect("poisoned");
        // SAFETY: Decouple the lifetime of the ColumnFamily from the lifetime of the
        // RwLockReadGuard.
        //
        // The lifetime annotation on BoundColumnFamily couples its lifetime with the DB it came
        // from, which is owned by `self` through `Inner`, so it is safe to extend the lifetime of
        // the column family from that of the read guard, to that of `self` using `transmute`.
        unsafe { std::mem::transmute(i.borrow_db().cf_handle(name)) }
    }

    /// Drop the column family with the given `name`, if it exists.
    pub(crate) fn drop_cf(&self, name: &str) -> Result<(), Error> {
        let i = self.0.read().expect("poisoned");
        Ok(i.borrow_db().drop_cf(name)?)
    }

    /// Return the watermark that was written for the given `pipeline`, or `None` if no checkpoint
    /// has been written for that pipeline yet.
    pub(crate) fn watermark(&self, pipeline: &str) -> Result<Option<Watermark>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let Some(watermark) = f.db.get_pinned_cf(f.watermark_cf, pipeline.as_bytes())? else {
                return Ok(None);
            };

            Ok(Some(
                bcs::from_bytes(&watermark).context("Failed to deserialize watermark")?,
            ))
        })
    }

    /// The number of snapshots this database has.
    pub(crate) fn snapshots(&self) -> usize {
        self.0.read().expect("poisoned").with_snapshots(|s| s.len())
    }

    /// The range of checkpoints that the database has snapshots for, or `None` if there are no
    /// snapshots.
    pub(crate) fn snapshot_range(&self) -> Option<RangeInclusive<u64>> {
        self.0.read().expect("poisoned").with_snapshots(|s| {
            let (&lo, _) = s.first_key_value()?;
            let (&hi, _) = s.last_key_value()?;
            Some(lo..=hi)
        })
    }

    /// Point look-up at `checkpoint` for the given `key`, in the column family `cf`.
    ///
    /// Fails if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn get<K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        key: &K,
    ) -> Result<Option<V>, Error>
    where
        K: Encode,
        V: DeserializeOwned,
    {
        let s = self.at_snapshot(checkpoint)?;
        let k = key::encode(key);

        let Some(bytes) = s.get_pinned_cf(cf, k)? else {
            return Ok(None);
        };

        Ok(Some(bcs::from_bytes(&bytes)?))
    }

    /// Multi-point look-up at `checkpoint` for the given `key`, in the column family `cf`.
    ///
    /// Fails if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn multi_get<'k, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        keys: impl IntoIterator<Item = &'k K>,
    ) -> Result<Vec<Result<Option<V>, Error>>, Error>
    where
        K: Encode + 'k,
        V: DeserializeOwned,
    {
        let s = self.at_snapshot(checkpoint)?;
        let ks: Vec<_> = keys.into_iter().map(key::encode).collect();

        let mut opt = rocksdb::ReadOptions::default();
        opt.set_snapshot(s.as_ref());

        let i = self.0.read().expect("poisoned");
        let sorted_input = false;
        Ok(i.borrow_db()
            .batched_multi_get_cf_opt(cf, &ks, sorted_input, &opt)
            .into_iter()
            .map(|res| match res {
                Ok(Some(bytes)) => Ok(Some(bcs::from_bytes(&bytes)?)),
                Ok(None) => Ok(None),
                Err(e) => Err(Error::Storage(e)),
            })
            .collect())
    }

    /// Create a forward iterator over the values in column family `cf` at the given `checkpoint`,
    /// optionally bounding the keys on either side by the given `range`. A forward iterator yields
    /// keys in ascending bincoded lexicographic order.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn iter<J, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        range: impl RangeBounds<J>,
    ) -> Result<iter::FwdIter<'_, K, V>, Error>
    where
        J: Encode,
    {
        let IterBounds(lo, _, Some(mut inner)) = self.iter_raw(checkpoint, cf, range)? else {
            return Ok(iter::FwdIter::new(None));
        };

        if let Some(lo) = &lo {
            inner.seek(lo);
        } else {
            inner.seek_to_first();
        }

        Ok(iter::FwdIter::new(Some(inner)))
    }

    /// Create a reverse iterator over the values in column family `cf` at the given `checkpoint`,
    /// optionally bounding the keys on either side by the given `range`. A reverse iterator yields
    /// keys in descending bincoded lexicographic order.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn iter_rev<J, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        range: impl RangeBounds<J>,
    ) -> Result<iter::RevIter<'_, K, V>, Error>
    where
        J: Encode,
    {
        let IterBounds(_, hi, Some(mut inner)) = self.iter_raw(checkpoint, cf, range)? else {
            return Ok(iter::RevIter::new(None));
        };

        if let Some(hi) = &hi {
            inner.seek_for_prev(hi);
        } else {
            inner.seek_to_last();
        }

        Ok(iter::RevIter::new(Some(inner)))
    }

    #[inline]
    fn at_snapshot(&self, checkpoint: u64) -> Result<Arc<rocksdb::Snapshot<'_>>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let Some(snapshot) = f.snapshots.get(&checkpoint).cloned() else {
                return Err(Error::NotInRange { checkpoint });
            };

            // SAFETY: Decouple the lifetime of the Snapshot from the lifetime of the
            // RwLockReadGuard.
            //
            // The lifetime annotation on Snapshot couples its lifetime with the DB it came from,
            // which is owned by `self` through `Inner`, so it is safe to extend the lifetime of
            // the column family from that of the read guard, to that of `self` using `transmute`.
            let snapshot: Arc<rocksdb::Snapshot<'_>> = unsafe { std::mem::transmute(snapshot) };

            Ok(snapshot)
        })
    }

    #[inline]
    fn iter_raw<J>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        range: impl RangeBounds<J>,
    ) -> Result<IterBounds<'_>, Error>
    where
        J: Encode,
    {
        let s = self.at_snapshot(checkpoint)?;

        let lo = match range.start_bound() {
            Bound::Unbounded => None,
            Bound::Included(start) => Some(key::encode(start)),
            Bound::Excluded(start) => {
                let mut start = key::encode(start);
                if !key::next(&mut start) {
                    return Ok(IterBounds::default());
                }
                Some(start)
            }
        };

        let hi = match range.end_bound() {
            Bound::Unbounded => None,
            Bound::Included(end) => {
                let mut end = key::encode(end);
                key::next(&mut end).then_some(end)
            }
            Bound::Excluded(end) => {
                let end = key::encode(end);
                if end.iter().all(|&b| b == 0) {
                    return Ok(IterBounds::default());
                }
                Some(end)
            }
        };

        let mut opts = rocksdb::ReadOptions::default();

        if let Some(lo) = &lo {
            opts.set_iterate_lower_bound(lo.clone());
        }

        if let Some(hi) = &hi {
            opts.set_iterate_upper_bound(hi.clone());
        }

        // SAFETY: Decouple the lifetime of the DBRawIterator from the lifetime of the reference
        // into the snapshot that it came from.
        //
        // The lifetime annotation is used to couple the lifetime of the iterator with that of the
        // database it is from, (via its snapshot). The iterator internally keeps the snapshot it
        // is from alive, so it is safe to extend its lifetime to that of `self` (which owns the
        // database, through `Inner`), using `transmute`.
        //
        // The lifetime annotation on Snapshot couples its lifetime with the DB it came from,
        // which is owned by `self` through `Inner`, so it is safe to extend the lifetime of
        // the column family from that of the read guard, to that of `self` using `transmute`.
        let inner: rocksdb::DBRawIterator<'_> =
            unsafe { std::mem::transmute(s.raw_iterator_cf_opt(cf, opts)) };

        Ok(IterBounds(lo, hi, Some(inner)))
    }
}

/// SAFETY: [`Db`] wraps an `RwLock` which protects access to its internals.
unsafe impl std::marker::Sync for Db {}
unsafe impl std::marker::Send for Db {}

impl From<Watermark> for CommitterWatermark {
    fn from(w: Watermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

impl From<CommitterWatermark> for Watermark {
    fn from(w: CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn cfs() -> Vec<(&'static str, rocksdb::Options)> {
        vec![("test", rocksdb::Options::default())]
    }

    fn opts() -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts
    }

    pub(crate) fn wm(cp: u64) -> Watermark {
        Watermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: cp,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        }
    }

    #[test]
    fn test_open() {
        let d = tempfile::tempdir().unwrap();
        Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
    }

    #[test]
    fn test_reopen() {
        let d = tempfile::tempdir().unwrap();
        Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();

        // Reopen with default options which will only work if the database and column families
        // already exist.
        Db::open(d.path().join("db"), rocksdb::Options::default(), 4, cfs()).unwrap();
    }

    #[test]
    fn test_multiple_opens() {
        let d = tempfile::tempdir().unwrap();

        // Open the database once.
        let _db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();

        // Opening the same database again should fail.
        assert!(Db::open(d.path().join("db"), opts(), 4, cfs()).is_err());
    }

    #[test]
    fn test_read_empty() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        db.snapshot(0);
        assert!(db.get::<u64, u64>(0, &cf, &42u64).unwrap().is_none());
    }

    #[test]
    fn test_snapshot_circular_buffer() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        for i in 0..10 {
            db.snapshot(i);
        }

        // The first 6 snapshots should be dropped.
        for i in 0..6 {
            let err = db.get::<u64, u64>(i, &cf, &42u64).unwrap_err();
            assert!(
                matches!(err, Error::NotInRange { checkpoint } if checkpoint == i),
                "Unexpected error: {err:?}"
            );
        }

        // The remaining snapshots should be accessible (but contain no data).
        for i in 6..10 {
            assert!(db.get::<u64, u64>(i, &cf, &42u64).unwrap().is_none());
        }
    }

    #[test]
    fn test_write_snapshot_read() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        // Register an empty snapshot.
        db.snapshot(0);

        let k = 42u64;
        let v0 = 43u64;
        let v1 = 44u64;

        // Write a value.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k), bcs::to_bytes(&v0).unwrap());
        db.write("test", wm(1), batch).unwrap();

        {
            // The snapshot that the write would be in has not been taken yet -- attempting to read it
            // fails.
            let err = db.get::<u64, u64>(1, &cf, &k).unwrap_err();
            assert!(
                matches!(err, Error::NotInRange { checkpoint: 1 }),
                "Unexpected error: {err:?}"
            );
        }

        {
            // A snapshot does exist, from before the write, but it will not be updated to reflect
            // the write.
            assert_eq!(db.get(0, &cf, &k).unwrap(), None::<u64>);
        }

        {
            // Once the snapshot has been taken, the write is visible.
            db.snapshot(1);
            assert_eq!(db.get(1, &cf, &k).unwrap(), Some(v0));
        }

        {
            // The value is still not present in the previous snapshot.
            assert_eq!(db.get(0, &cf, &k).unwrap(), None::<u64>);
        }

        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k), bcs::to_bytes(&v1).unwrap());
        db.write("test", wm(2), batch).unwrap();
        db.snapshot(2);

        {
            // A new value has been written, and a snapshot taken, we can now read the value at
            // every point in history.
            assert_eq!(db.get(0, &cf, &k).unwrap(), None::<u64>);
            assert_eq!(db.get(1, &cf, &k).unwrap(), Some(v0));
            assert_eq!(db.get(2, &cf, &k).unwrap(), Some(v1));
        }
    }

    #[test]
    fn test_multi_get() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let (k0, v0) = (42u64, 43u64);
        let (k1, v1) = (44u64, 45u64); // not written in the first batch
        let (k2, v2) = (46u64, 47u64);
        let (k3, v3) = (48u64, 49u32);

        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k0), bcs::to_bytes(&v0).unwrap());
        batch.put_cf(&cf, key::encode(&k2), bcs::to_bytes(&v2).unwrap());
        batch.put_cf(&cf, key::encode(&k3), bcs::to_bytes(&v3).unwrap());
        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        let mut res = db
            .multi_get(0, &cf, [&k0, &k1, &k2, &k3])
            .unwrap()
            .into_iter();

        assert_eq!(res.next().unwrap().unwrap(), Some(v0), "Key: {k0}");
        assert_eq!(res.next().unwrap().unwrap(), None, "Key: {k1}");
        assert_eq!(res.next().unwrap().unwrap(), Some(v2), "Key: {k2}");
        assert!(
            matches!(res.next().unwrap().unwrap_err(), Error::Bcs(_)),
            "Key: {k3}"
        );

        // Perform another batch of writes correcting the mistakes from the previous batch.
        let v3 = 49u64;

        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k1), bcs::to_bytes(&v1).unwrap());
        batch.put_cf(&cf, key::encode(&k3), bcs::to_bytes(&v3).unwrap());
        db.write("test", wm(1), batch).unwrap();
        db.snapshot(1);

        let mut res = db
            .multi_get(1, &cf, [&k0, &k1, &k2, &k3])
            .unwrap()
            .into_iter();

        assert_eq!(res.next().unwrap().unwrap(), Some(v0), "Key: {k0}");
        assert_eq!(res.next().unwrap().unwrap(), Some(v1), "Key: {k1}");
        assert_eq!(res.next().unwrap().unwrap(), Some(v2), "Key: {k2}");
        assert_eq!(res.next().unwrap().unwrap(), Some(v3), "Key: {k3}");

        // Making the same query as before should yield the same results again.
        let mut res = db
            .multi_get(0, &cf, [&k0, &k1, &k2, &k3])
            .unwrap()
            .into_iter();

        assert_eq!(res.next().unwrap().unwrap(), Some(v0), "Key: {k0}");
        assert_eq!(res.next().unwrap().unwrap(), None, "Key: {k1}");
        assert_eq!(res.next().unwrap().unwrap(), Some(v2), "Key: {k2}");
        assert!(
            matches!(res.next().unwrap().unwrap_err(), Error::Bcs(_)),
            "Key: {k3}"
        );
    }

    #[test]
    fn test_watermark() {
        let cfs = vec![
            ("p0", rocksdb::Options::default()),
            ("p1", rocksdb::Options::default()),
        ];

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs).unwrap();
        let p0 = db.cf("p0").unwrap();
        let p1 = db.cf("p1").unwrap();

        // Haven't written anything yet, so no last checkpoint.
        assert_eq!(db.watermark("p0").unwrap(), None);
        assert_eq!(db.watermark("p1").unwrap(), None);

        // Write a batch for the pipeline p0.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p0, key::encode(&42u64), bcs::to_bytes(&43u64).unwrap());
        db.write("p0", wm(0), batch).unwrap();

        // Wrote to one pipeline, but not the other, unlike the data itself, watermarks are not
        // read from snapshots.
        assert_eq!(db.watermark("p0").unwrap(), Some(wm(0)));
        assert_eq!(db.watermark("p1").unwrap(), None);

        // Write a batch for the pipeline p1.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p1, key::encode(&44u64), bcs::to_bytes(&45u64).unwrap());
        db.write("p1", wm(1), batch).unwrap();

        // Wrote to both pipelines.
        assert_eq!(db.watermark("p0").unwrap(), Some(wm(0)));
        assert_eq!(db.watermark("p1").unwrap(), Some(wm(1)));
    }

    #[test]
    fn test_persistence() {
        let d = tempfile::tempdir().unwrap();

        {
            // Create a fresh database and write some data into it.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            let cf = db.cf("test").unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            batch.put_cf(&cf, key::encode(&42u64), bcs::to_bytes(&43u64).unwrap());
            db.write("test", wm(1), batch).unwrap();

            // Check that the watermark was written.
            assert_eq!(db.watermark("test").unwrap(), Some(wm(1)));

            // ...and once there is a snapshot, the data can be read.
            db.snapshot(1);
            assert_eq!(db.get(1, &cf, &42u64).unwrap(), Some(43u64));
        }

        {
            // Re-open the database.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            let cf = db.cf("test").unwrap();

            // The `watermark` persists.
            assert_eq!(db.watermark("test").unwrap(), Some(wm(1)));

            // The snapshots do not, however, so reads will fail.
            let err = db.get::<u64, u64>(1, &cf, &42u64).unwrap_err();
            assert!(
                matches!(err, Error::NotInRange { checkpoint: 1 }),
                "Unexpected error: {err:?}"
            );

            // But once the snapshot has been taken, the data is still there.
            db.snapshot(1);
            assert_eq!(db.get(1, &cf, &42u64).unwrap(), Some(43u64));
        }
    }

    #[test]
    fn test_forward_iteration() {
        use Bound::{Excluded as E, Unbounded as U};

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&i), bcs::to_bytes(&(i + 1)).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, (U::<u64>, U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)],
            "full range"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, 4u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(4, 5), (6, 7), (8, 9)],
            "exact match, inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, 3u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(4, 5), (6, 7), (8, 9)],
            "inexact match, inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, (E(4u64), U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(6, 7), (8, 9)],
            "exact match, exclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, (E(3u64), U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(4, 5), (6, 7), (8, 9)],
            "inexact match, exclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, 0u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)],
            "redundant inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, 100u64..).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "empty inclusive lowerbound");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, (E(u64::MAX), U)).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "vacuous exclusive lowerbound");

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..=4u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5)],
            "exact match, inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..=5u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5)],
            "inexact match, inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..4u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3)],
            "exact match, exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..5u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5)],
            "inexact match, exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..0u64).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "vacuous exclusive upperbound");

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, ..100u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)],
            "non-filtering exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, ..=u64::MAX).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)],
            "redundant inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter(0, &cf, 0u64..4).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3)],
            "bounded above and below"
        );
    }

    #[test]
    fn test_forward_iteration_seek() {
        use Bound::Unbounded as U;

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&i), bcs::to_bytes(&(i + 1)).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(&4u64);
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "exact seek");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(&3u64);
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "inexact seek");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(0, 1), (2, 3), (4, 5)], "take 3");
        iter.seek(&2u64);
        assert_eq!(iter.next().unwrap().unwrap(), (2, 3), "rewind");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(0, 1), (2, 3), (4, 5)], "take 3");
        iter.seek(&7u64);
        assert_eq!(iter.next().unwrap().unwrap(), (8, 9), "fast forward");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, 4u64..8).unwrap();
        iter.seek(&1u64);
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "underflow");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, 4u64..8).unwrap();
        iter.seek(&8u64);
        assert!(iter.next().is_none(), "overflow");
    }

    #[test]
    fn test_iteration_consistency() {
        use Bound::Unbounded as U;

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&i), bcs::to_bytes(&(i + 1)).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        // Create an iterator from the first snapshot.
        let mut i0: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();

        // Start iterating through it.
        let kv0: Result<Vec<(u64, u64)>, Error> = (&mut i0).take(3).collect();
        assert_eq!(kv0.unwrap(), vec![(0, 1), (2, 3), (4, 5)], "i0: first 3");

        // Write some more data, in the next snapshot.
        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&(i + 1)), bcs::to_bytes(&i).unwrap());
        }

        db.write("test", wm(1), batch).unwrap();
        db.snapshot(1);

        // Create an iterator from the next snapshot.
        let mut i1: iter::FwdIter<u64, u64> = db.iter(1, &cf, (U::<u64>, U)).unwrap();

        // Finish iterating through the first iterator.
        let kv0: Result<Vec<(u64, u64)>, Error> = (&mut i0).collect();
        assert_eq!(kv0.unwrap(), vec![(6, 7), (8, 9)], "i0: rest");

        // Start iterating through the second iterator.
        let kv1: Result<Vec<(u64, u64)>, Error> = (&mut i1).take(3).collect();
        assert_eq!(kv1.unwrap(), vec![(0, 1), (1, 0), (2, 3)], "i1: first 3");

        // Delete the data from the original batch.
        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.delete_cf(&cf, key::encode(&i));
        }

        db.write("test", wm(2), batch).unwrap();
        db.snapshot(2);

        // Finish iterating through the second iterator.
        let kv1: Result<Vec<(u64, u64)>, Error> = (&mut i1).collect();
        assert_eq!(
            kv1.unwrap(),
            vec![(3, 2), (4, 5), (5, 4), (6, 7), (7, 6), (8, 9), (9, 8)],
            "i1: rest"
        );

        // Create new iterators at each snapshot, and ensure they still yield the same results.
        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(0, &cf, (U::<u64>, U)).unwrap().collect();
        let expect: Vec<_> = (0..10).step_by(2).map(|i| (i, i + 1)).collect();
        assert_eq!(actual.unwrap(), expect, "i0: full");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(1, &cf, (U::<u64>, U)).unwrap().collect();
        let expect: Vec<_> = (0..10)
            .step_by(2)
            .flat_map(|i| [(i, i + 1), (i + 1, i)])
            .collect();
        assert_eq!(actual.unwrap(), expect, "i1: full");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter(2, &cf, (U::<u64>, U)).unwrap().collect();
        let expect: Vec<_> = (0..10).step_by(2).map(|i| (i + 1, i)).collect();
        assert_eq!(actual.unwrap(), expect, "i2: full");
    }

    #[test]
    fn test_iteration_snapshot_keepalive() {
        use Bound::Unbounded as U;

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&i), bcs::to_bytes(&(i + 1)).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        // Create an iterator from the first snapshot.
        let iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();

        // Create more snapshots...
        for i in 1..5 {
            db.snapshot(i);
        }

        // ...such that the first snapshot gets dropped.
        assert!(matches!(
            db.get::<u64, u64>(0, &cf, &0u64).unwrap_err(),
            Error::NotInRange { checkpoint: 0 },
        ));

        // Iterate through the iterator, which should have kept the snapshot alive.
        let actual: Result<Vec<(u64, u64)>, Error> = iter.collect();
        assert_eq!(
            actual.unwrap(),
            vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)],
        );
    }

    #[test]
    fn test_reverse_iteration() {
        use Bound::{Excluded as E, Unbounded as U};

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&(i + 1)), bcs::to_bytes(&i).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, (U::<u64>, U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4), (3, 2), (1, 0)],
            "full range"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, 5u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4)],
            "exact match, inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, 4u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4)],
            "inexact match, inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, (E(5u64), U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6)],
            "exact match, exclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, (E(4u64), U)).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4)],
            "inexact match, exclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, 0u64..).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4), (3, 2), (1, 0)],
            "redundant inclusive lowerbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, 100u64..).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "empty inclusive lowerbound");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, (E(u64::MAX), U)).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "vacuous exclusive lowerbound");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, ..=5u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(5, 4), (3, 2), (1, 0)],
            "exact match, inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, ..=6u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(5, 4), (3, 2), (1, 0)],
            "inexact match, inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, ..5u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(3, 2), (1, 0)],
            "exact match, exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, ..6u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(5, 4), (3, 2), (1, 0)],
            "inexact match, exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> = db.iter_rev(0, &cf, ..0u64).unwrap().collect();
        assert_eq!(actual.unwrap(), vec![], "vacuous exclusive upperbound");

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, ..100u64).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4), (3, 2), (1, 0)],
            "non-filtering exclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, ..=u64::MAX).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(9, 8), (7, 6), (5, 4), (3, 2), (1, 0)],
            "redundant inclusive upperbound"
        );

        let actual: Result<Vec<(u64, u64)>, Error> =
            db.iter_rev(0, &cf, 0u64..5).unwrap().collect();
        assert_eq!(
            actual.unwrap(),
            vec![(3, 2), (1, 0)],
            "bounded above and below"
        );
    }

    #[test]
    fn test_reverse_iteration_seek() {
        use Bound::Unbounded as U;

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let mut batch = rocksdb::WriteBatch::default();
        for i in (0u64..10).step_by(2) {
            batch.put_cf(&cf, key::encode(&(i + 1)), bcs::to_bytes(&i).unwrap());
        }

        db.write("test", wm(0), batch).unwrap();
        db.snapshot(0);

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(&5u64);
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "exact seek");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(&6u64);
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "inexact seek");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(9, 8), (7, 6), (5, 4)], "take 3");
        iter.seek(&7u64);
        assert_eq!(iter.next().unwrap().unwrap(), (7, 6), "rewind");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(9, 8), (7, 6), (5, 4)], "take 3");
        iter.seek(&1u64);
        assert_eq!(iter.next().unwrap().unwrap(), (1, 0), "fast forward");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, 3u64..7).unwrap();
        iter.seek(&9u64);
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "underflow");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, 3u64..7).unwrap();
        iter.seek(&1u64);
        assert!(iter.next().is_none(), "overflow");
    }
}

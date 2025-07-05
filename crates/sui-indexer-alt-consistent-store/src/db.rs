// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, RwLock},
};

use anyhow::Context;
use rocksdb::AsColumnFamilyRef;
use serde::{de::DeserializeOwned, Serialize};

use crate::key;

/// Name of the column family the database adds, to manage the checkpoint watermark.
const WATERMARK_CF: &str = "$watermark";

/// A wrapper around RocksDB that provides arbitrary writes and snapshot-based reads (reads must
/// specify the checkpoint they want to read from). Keys and values are encoded (using Bincode and
/// BCS respectively) to provide a type-safe API.
///
/// ## Watermarks
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
/// ## Atomicity
///
/// Writes are applied atomically, with a watermark update attached, to ensure updates remain
/// fault-tolerant in the event of a crash.
///
/// ## Concurrency
///
/// Most of the Db's internals are held in a self-referential data structure, protected by a
/// read-write lock. This allows for concurrent reading, writing and snapshotting. Exclusive access
/// is only required to create a new snapshot, reads and writes to RocksDB can proceed
/// concurrently.
pub(crate) struct Db(RwLock<Inner>);

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

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("BCS error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Database has no capacity for snapshots")]
    Empty,

    #[error("Internal error: {0:?}")]
    Internal(#[from] anyhow::Error),

    #[error("Checkpoint {checkpoint} not in consistent range")]
    NotInRange { checkpoint: u64 },

    #[error("Storage error: {0}")]
    Storage(#[from] rocksdb::Error),
}

impl Db {
    /// Open the database at `path`, with the given `capacity` for snapshots.
    ///
    /// `options` are passed to RocksDB to configure the database, and `cfs` denotes the column
    /// families to open. The database will inject its own column family for watermarks, and set
    /// the option to create missing column families.
    pub(crate) fn open<'c>(
        capacity: usize,
        path: impl AsRef<Path>,
        mut options: rocksdb::Options,
        cfs: impl IntoIterator<Item = (&'c str, rocksdb::Options)>,
    ) -> Result<Arc<Self>, Error> {
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

        Ok(Arc::new(Self(RwLock::new(inner))))
    }

    /// Write a batch of updates to the database atomically, along with a watermark with key
    /// `pipeline` and value `checkpoint`.
    pub(crate) fn write(
        &self,
        pipeline: &str,
        checkpoint: u64,
        mut batch: rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        let checkpoint = bcs::to_bytes(&checkpoint).context("Failed to serialize watermark")?;

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

    /// Return the last checkpoint that was written for the given `pipeline`, or `None` if no
    /// checkpoint has been written for that pipeline yet.
    pub(crate) fn last_checkpoint(&self, pipeline: &str) -> Result<Option<u64>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let Some(cp) = f.db.get_pinned_cf(f.watermark_cf, pipeline.as_bytes())? else {
                return Ok(None);
            };

            let cp = bcs::from_bytes(&cp).context("Failed to deserialize checkpoint")?;
            Ok(Some(cp))
        })
    }

    /// Point look-up at `checkpoint` for the given `key`, in the column family `cf`.
    pub(crate) fn get<K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        key: &K,
    ) -> Result<Option<V>, Error>
    where
        K: Serialize,
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
    pub(crate) fn multi_get<'k, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        keys: impl IntoIterator<Item = &'k K>,
    ) -> Result<Vec<Result<Option<V>, Error>>, Error>
    where
        K: Serialize + 'k,
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
}

/// SAFETY: [`Db`] wraps an `RwLock` which protects access to its internals.
unsafe impl std::marker::Sync for Db {}
unsafe impl std::marker::Send for Db {}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfs() -> Vec<(&'static str, rocksdb::Options)> {
        vec![("test", rocksdb::Options::default())]
    }

    fn opts() -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts
    }

    #[test]
    fn test_open() {
        let d = tempfile::tempdir().unwrap();
        Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
    }

    #[test]
    fn test_reopen() {
        let d = tempfile::tempdir().unwrap();
        Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();

        // Reopen with default options which will only work if the database and column families
        // already exist.
        Db::open(4, d.path().join("db"), rocksdb::Options::default(), cfs()).unwrap();
    }

    #[test]
    fn test_multiple_opens() {
        let d = tempfile::tempdir().unwrap();

        // Open the database once.
        let _db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();

        // Opening the same database again should fail.
        assert!(Db::open(4, d.path().join("db"), opts(), cfs()).is_err());
    }

    #[test]
    fn test_read_empty() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        db.snapshot(0);
        assert!(db.get::<u64, u64>(0, &cf, &42u64).unwrap().is_none());
    }

    #[test]
    fn test_snapshot_circular_buffer() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
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
        let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        // Register an empty snapshot.
        db.snapshot(0);

        let k = 42u64;
        let v0 = 43u64;
        let v1 = 44u64;

        // Write a value.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k), bcs::to_bytes(&v0).unwrap());
        db.write("test", 1, batch).unwrap();

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
        db.write("test", 2, batch).unwrap();
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
        let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        let (k0, v0) = (42u64, 43u64);
        let (k1, v1) = (44u64, 45u64); // not written in the first batch
        let (k2, v2) = (46u64, 47u64);
        let (k3, v3) = (48u64, 49u32);

        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k0), bcs::to_bytes(&v0).unwrap());
        batch.put_cf(&cf, key::encode(&k2), bcs::to_bytes(&v2).unwrap());
        batch.put_cf(&cf, key::encode(&k3), bcs::to_bytes(&v3).unwrap());
        db.write("test", 0, batch).unwrap();
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
        db.write("test", 1, batch).unwrap();
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
    fn test_last_checkpoint() {
        let cfs = vec![
            ("p0", rocksdb::Options::default()),
            ("p1", rocksdb::Options::default()),
        ];

        let d = tempfile::tempdir().unwrap();
        let db = Db::open(4, d.path().join("db"), opts(), cfs).unwrap();
        let p0 = db.cf("p0").unwrap();
        let p1 = db.cf("p1").unwrap();

        // Haven't written anything yet, so no last checkpoint.
        assert_eq!(db.last_checkpoint("p0").unwrap(), None);
        assert_eq!(db.last_checkpoint("p1").unwrap(), None);

        // Write a batch for the pipeline p0.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p0, key::encode(&42u64), bcs::to_bytes(&43u64).unwrap());
        db.write("p0", 0, batch).unwrap();

        // Wrote to one pipeline, but not the other, unlike the data itself, watermarks are not
        // read from snapshots.
        assert_eq!(db.last_checkpoint("p0").unwrap(), Some(0));
        assert_eq!(db.last_checkpoint("p1").unwrap(), None);

        // Write a batch for the pipeline p1.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p1, key::encode(&44u64), bcs::to_bytes(&45u64).unwrap());
        db.write("p1", 1, batch).unwrap();

        // Wrote to both pipelines.
        assert_eq!(db.last_checkpoint("p0").unwrap(), Some(0));
        assert_eq!(db.last_checkpoint("p1").unwrap(), Some(1));
    }

    #[test]
    fn test_persistence() {
        let d = tempfile::tempdir().unwrap();

        {
            // Create a fresh database and write some data into it.
            let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
            let cf = db.cf("test").unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            batch.put_cf(&cf, key::encode(&42u64), bcs::to_bytes(&43u64).unwrap());
            db.write("test", 1, batch).unwrap();

            // Check that the watermark was written.
            assert_eq!(db.last_checkpoint("test").unwrap(), Some(1));

            // ...and once there is a snapshot, the data can be read.
            db.snapshot(1);
            assert_eq!(db.get(1, &cf, &42u64).unwrap(), Some(43u64));
        }

        {
            // Re-open the database.
            let db = Db::open(4, d.path().join("db"), opts(), cfs()).unwrap();
            let cf = db.cf("test").unwrap();

            // The `last_checkpoint` persists.
            assert_eq!(db.last_checkpoint("test").unwrap(), Some(1));

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
}

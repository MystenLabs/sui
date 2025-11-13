// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{
    cmp,
    collections::BTreeMap,
    marker,
    ops::{Bound, RangeBounds, RangeInclusive},
    path::Path,
    sync::{Arc, RwLock},
};

use anyhow::Context;
use bincode::Encode;
use rocksdb::{AsColumnFamilyRef, properties};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sui_indexer_alt_framework::store::CommitterWatermark;

use self::error::Error;

pub(crate) mod config;
pub(crate) mod error;
pub(crate) mod iter;
pub(crate) mod key;
pub(crate) mod map;

/// Name of the column family the database adds, to manage the checkpoint watermark.
const WATERMARK_CF: &str = "$watermark";

/// Name of the column family the database adds, to track restoration progress.
const RESTORE_CF: &str = "$restore";

// Constants for periodic metrics reporting
const METRICS_ERROR: i64 = -1;

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

/// Identifier for a particular shard of the live object set that a pipeline has successfully
/// restored from.
#[derive(Encode, PartialEq, Eq)]
pub(crate) struct Restored<'p> {
    pipeline: &'p str,
    bucket: u32,
    partition: u32,
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

    /// ColumnFamily in `db` that restoration progress is written to.
    #[borrows(db)]
    #[covariant]
    restore_cf: Arc<rocksdb::BoundColumnFamily<'this>>,

    /// Snapshots from `db`, ordered by checkpoint sequence number, along with their watermarks.
    #[borrows()]
    #[covariant]
    snapshots: BTreeMap<u64, (Arc<rocksdb::Snapshot<'this>>, Watermark)>,
}

/// A raw iterator along with its encoded upper and lower bounds.
#[derive(Default)]
struct IterBounds<'d>(
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<rocksdb::DBRawIterator<'d>>,
);

/// Metrics related to memory usage and backpressure.
pub struct RocksMetrics {
    /// Size of the active memtable in bytes.
    pub current_size_active_mem_tables: i64,
    /// Size of active, unflushed immutable, and pinned memtable in bytes.
    pub size_all_mem_tables: i64,
    /// Memory size for the entries residing in the block cache.
    pub block_cache_usage: i64,
    /// Memory size of entries pinned in the block cache.
    pub block_cache_pinned_usage: i64,
    /// Estimated memory used by SST table readers, not including memory used.
    pub estimate_table_readers_mem: i64,
    /// Total number of bytes that need to be compacted to get all levels down to under target size.
    pub estimate_pending_compaction_bytes: i64,
    /// Number of L0 files.
    pub num_level0_files: i64,
    /// Number of immutable memtables that have not yet been flushed.
    pub num_immutable_mem_tables: i64,
    /// Boolean flag (0/1) indicating whether a memtable flush is pending.
    pub mem_table_flush_pending: i64,
    /// Boolean flag (0/1) indicating whether a compaction is pending.
    pub compaction_pending: i64,
    /// Number of snapshots.
    pub num_snapshots: i64,
    /// Number of running compactions.
    pub num_running_compactions: i64,
    /// Number of running flushes.
    pub num_running_flushes: i64,
    /// The current delayed write rate. 0 means no delay.
    pub actual_delayed_write_rate: i64,
    /// Boolean flag (0/1) indicating whether RocksDB has stopped all writes.
    pub is_write_stopped: i64,
}

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
        cfs.push((RESTORE_CF, rocksdb::Options::default()));
        options.create_missing_column_families(true);

        let db = rocksdb::DB::open_cf_with_opts(&options, path, cfs)?;
        let inner = Inner::try_new(
            capacity,
            db,
            |db| db.cf_handle(WATERMARK_CF).context("WATERMARK_CF not found"),
            |db| db.cf_handle(RESTORE_CF).context("RESTORE_CF not found"),
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
        let p = key::encode(pipeline.as_bytes());
        batch.put_cf(i.borrow_watermark_cf(), &p, checkpoint);
        i.borrow_db().write(batch)?;
        Ok(())
    }

    /// Try to start restoration for a given `pipeline` at a given `watermark`. This operation will
    /// fail if a restoration is already in-progress for that pipeline at a different watermark, or
    /// if there is already a commit watermark for the pipeline (meaning restoration has completed
    /// and/or indexing has started).
    pub(crate) fn restore_at(&self, pipeline: &str, watermark: Watermark) -> Result<(), Error> {
        self.0.read().expect("poisoned").with(|f| {
            let p = key::encode(pipeline.as_bytes());

            if f.db.get_pinned_cf(f.watermark_cf, &p)?.is_some() {
                return Err(Error::RestoreOverwrite);
            }

            let Some(existing) = f.db.get_pinned_cf(f.restore_cf, &p)? else {
                f.db.put_cf(f.restore_cf, &p, bcs::to_bytes(&watermark)?)?;
                return Ok(());
            };

            let existing: Watermark = bcs::from_bytes(&existing)
                .context("Failed to deserialize existing restore watermark")?;

            if existing != watermark {
                Err(Error::RestoreInProgress(existing.epoch_hi_inclusive))
            } else {
                Ok(())
            }
        })
    }

    /// Write a batch of updates to the database atomically, along with a record that this comes
    /// from restoring objects from `bucket` and `partition` into `pipeline`.
    pub(crate) fn restore(
        &self,
        bucket: u32,
        partition: u32,
        pipeline: &str,
        mut batch: rocksdb::WriteBatch,
    ) -> Result<(), Error> {
        let key = key::encode(&Restored {
            pipeline,
            bucket,
            partition,
        });

        let i = self.0.read().expect("poisoned");
        batch.put_cf(i.borrow_restore_cf(), key, []);
        i.borrow_db().write(batch)?;
        Ok(())
    }

    /// Given a sequence of pipelines, return a vector indicating which of them have already
    /// restored `bucket` and `partition`.
    pub(crate) fn is_restored<'p>(
        &self,
        bucket: u32,
        partition: u32,
        pipelines: impl IntoIterator<Item = &'p str>,
    ) -> Result<Vec<bool>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let keys: Vec<_> = pipelines
                .into_iter()
                .map(|pipeline| {
                    key::encode(&Restored {
                        pipeline,
                        bucket,
                        partition,
                    })
                })
                .collect();

            let sorted_input = false;
            f.db.batched_multi_get_cf(f.restore_cf, &keys, sorted_input)
                .into_iter()
                .map(|res| match res {
                    Ok(val) => Ok(val.is_some()),
                    Err(e) => Err(Error::Storage(e)),
                })
                .collect()
        })
    }

    /// Record the restoration of `pipeline` as completed by setting its watermark and removing its
    /// restoration state.
    pub(crate) fn complete_restore(&self, pipeline: &str) -> Result<(), Error> {
        self.0.read().expect("poisoned").with(|f| {
            let p = key::encode(pipeline.as_bytes());

            let Some(existing) = f.db.get_cf(f.restore_cf, &p)? else {
                return Ok(());
            };

            let mut q = p.clone();
            let q = if key::next(&mut q) { &q[..] } else { &[][..] };

            let mut batch = rocksdb::WriteBatch::default();
            batch.put_cf(f.watermark_cf, &p, &existing);
            batch.delete_range_cf(f.restore_cf, &p[..], q);

            Ok(f.db.write(batch)?)
        })
    }

    /// Register a new snapshot at the checkpoint specified in the watermark. This could result in
    /// the oldest (by checkpoint) snapshot being dropped to ensure the number of snapshots remain
    /// at or below `capacity`.
    pub(crate) fn take_snapshot(&self, watermark: Watermark) {
        self.0.write().expect("poisoned").with_mut(|f| {
            f.snapshots.insert(
                watermark.checkpoint_hi_inclusive,
                (Arc::new(f.db.snapshot()), watermark),
            );

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

    /// Return the last watermark that was written for the given `pipeline` during indexing, or
    /// `None` if the pipeline hasn't been indexed yet.
    pub(crate) fn commit_watermark(&self, pipeline: &str) -> Result<Option<Watermark>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let p = key::encode(pipeline.as_bytes());
            let Some(watermark) = f.db.get_pinned_cf(f.watermark_cf, &p)? else {
                return Ok(None);
            };

            Ok(Some(
                bcs::from_bytes(&watermark).context("Failed to deserialize watermark")?,
            ))
        })
    }

    /// Return the watermark that was written at the start of restoration for the given `pipeline`,
    /// or `None` if no restoration is in progress for that pipeline.
    pub(crate) fn restore_watermark(&self, pipeline: &str) -> Result<Option<Watermark>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let p = key::encode(pipeline.as_bytes());
            let Some(watermark) = f.db.get_pinned_cf(f.restore_cf, &p)? else {
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

    /// The range of checkpoints that the database has snapshots for, at or below
    /// `cp_hi_inclusive`, or `None` if there are no snapshots. Returns the watermark range.
    pub(crate) fn snapshot_range(&self, cp_hi_inclusive: u64) -> Option<RangeInclusive<Watermark>> {
        self.0.read().expect("poisoned").with_snapshots(|s| {
            let (_, (_, lo)) = s.first_key_value()?;
            let (_, (_, hi)) = s.range(..=cp_hi_inclusive).next_back()?;

            Some(*lo..=*hi)
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
        let lo = match range.start_bound() {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(start) => Bound::Included(key::encode(start)),
            Bound::Excluded(start) => Bound::Excluded(key::encode(start)),
        };

        let hi = match range.end_bound() {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(end) => Bound::Included(key::encode(end)),
            Bound::Excluded(end) => Bound::Excluded(key::encode(end)),
        };

        let IterBounds(lo, _, Some(mut inner)) = self.iter_raw(checkpoint, cf, lo, hi)? else {
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
        let lo = match range.start_bound() {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(start) => Bound::Included(key::encode(start)),
            Bound::Excluded(start) => Bound::Excluded(key::encode(start)),
        };

        let hi = match range.end_bound() {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(end) => Bound::Included(key::encode(end)),
            Bound::Excluded(end) => Bound::Excluded(key::encode(end)),
        };

        let IterBounds(_, hi, Some(mut inner)) = self.iter_raw(checkpoint, cf, lo, hi)? else {
            return Ok(iter::RevIter::new(None));
        };

        if let Some(hi) = &hi {
            inner.seek_for_prev(hi);
        } else {
            inner.seek_to_last();
        }

        Ok(iter::RevIter::new(Some(inner)))
    }

    /// Create a forward iterator over the values in column family `cf` at the given `checkpoint`,
    /// where all the keys start with the given `prefix`. A forward iterator yields keys in
    /// ascending bincoded lexicographic order, and the predicate is applied on the bincoded key
    /// and the bincoded prefix.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn prefix<J, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        prefix: &J,
    ) -> Result<iter::FwdIter<'_, K, V>, Error>
    where
        J: Encode,
    {
        let mut key = key::encode(prefix);
        let lo = Bound::Included(key.clone());
        let hi = if !key::next(&mut key) {
            Bound::Unbounded
        } else {
            Bound::Excluded(key)
        };

        let IterBounds(lo, _, Some(mut inner)) = self.iter_raw(checkpoint, cf, lo, hi)? else {
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
    /// where all the keys start with the gven `prefix`. A reverse iterator yields keys in
    /// descending bincoded lexicographic order, and the predicate is applied on the bincoded key
    /// and the bincoded prefix.
    ///
    /// This operation can fail if the database does not have a snapshot at `checkpoint`.
    pub(crate) fn prefix_rev<J, K, V>(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        prefix: &J,
    ) -> Result<iter::RevIter<'_, K, V>, Error>
    where
        J: Encode,
    {
        let mut key = key::encode(prefix);
        let lo = Bound::Included(key.clone());
        let hi = if !key::next(&mut key) {
            Bound::Unbounded
        } else {
            Bound::Excluded(key)
        };

        let IterBounds(_, hi, Some(mut inner)) = self.iter_raw(checkpoint, cf, lo, hi)? else {
            return Ok(iter::RevIter::new(None));
        };

        if let Some(hi) = &hi {
            inner.seek_for_prev(hi);
        } else {
            inner.seek_to_last();
        }

        Ok(iter::RevIter::new(Some(inner)))
    }

    pub(crate) fn column_family_metrics(&self, cf_name: &str) -> RocksMetrics {
        let i = self.0.read().expect("poisoned");
        let db = i.borrow_db();
        let Some(cf) = db.cf_handle(cf_name) else {
            return RocksMetrics::default();
        };

        RocksMetrics {
            current_size_active_mem_tables: cf_property_int_to_metric(
                db,
                &cf,
                properties::CUR_SIZE_ACTIVE_MEM_TABLE,
            ),
            size_all_mem_tables: cf_property_int_to_metric(
                db,
                &cf,
                properties::SIZE_ALL_MEM_TABLES,
            ),
            block_cache_usage: cf_property_int_to_metric(db, &cf, properties::BLOCK_CACHE_USAGE),
            block_cache_pinned_usage: cf_property_int_to_metric(
                db,
                &cf,
                properties::BLOCK_CACHE_PINNED_USAGE,
            ),
            estimate_table_readers_mem: cf_property_int_to_metric(
                db,
                &cf,
                properties::ESTIMATE_TABLE_READERS_MEM,
            ),
            estimate_pending_compaction_bytes: cf_property_int_to_metric(
                db,
                &cf,
                properties::ESTIMATE_PENDING_COMPACTION_BYTES,
            ),
            num_level0_files: cf_property_int_to_metric(
                db,
                &cf,
                &properties::num_files_at_level(0),
            ),
            actual_delayed_write_rate: cf_property_int_to_metric(
                db,
                &cf,
                properties::ACTUAL_DELAYED_WRITE_RATE,
            ),
            is_write_stopped: cf_property_int_to_metric(db, &cf, properties::IS_WRITE_STOPPED),
            num_immutable_mem_tables: cf_property_int_to_metric(
                db,
                &cf,
                properties::NUM_IMMUTABLE_MEM_TABLE,
            ),
            mem_table_flush_pending: cf_property_int_to_metric(
                db,
                &cf,
                properties::MEM_TABLE_FLUSH_PENDING,
            ),
            compaction_pending: cf_property_int_to_metric(db, &cf, properties::COMPACTION_PENDING),
            num_snapshots: cf_property_int_to_metric(db, &cf, properties::NUM_SNAPSHOTS),
            num_running_compactions: cf_property_int_to_metric(
                db,
                &cf,
                properties::NUM_RUNNING_COMPACTIONS,
            ),
            num_running_flushes: cf_property_int_to_metric(
                db,
                &cf,
                properties::NUM_RUNNING_FLUSHES,
            ),
        }
    }

    fn at_snapshot(&self, checkpoint: u64) -> Result<Arc<rocksdb::Snapshot<'_>>, Error> {
        self.0.read().expect("poisoned").with(|f| {
            let Some((snapshot, _)) = f.snapshots.get(&checkpoint) else {
                return Err(Error::NotInRange { checkpoint });
            };

            // SAFETY: Decouple the lifetime of the Snapshot from the lifetime of the
            // RwLockReadGuard.
            //
            // The lifetime annotation on Snapshot couples its lifetime with the DB it came from,
            // which is owned by `self` through `Inner`, so it is safe to extend the lifetime of
            // the column family from that of the read guard, to that of `self` using `transmute`.
            let snapshot: Arc<rocksdb::Snapshot<'_>> =
                unsafe { std::mem::transmute(snapshot.clone()) };

            Ok(snapshot)
        })
    }

    fn iter_raw(
        &self,
        checkpoint: u64,
        cf: &impl AsColumnFamilyRef,
        lo: Bound<Vec<u8>>,
        hi: Bound<Vec<u8>>,
    ) -> Result<IterBounds<'_>, Error> {
        let s = self.at_snapshot(checkpoint)?;

        let lo = match lo {
            Bound::Unbounded => None,
            Bound::Included(start) => Some(start),
            Bound::Excluded(mut start) => {
                if !key::next(&mut start) {
                    return Ok(IterBounds::default());
                }
                Some(start)
            }
        };

        let hi = match hi {
            Bound::Unbounded => None,
            Bound::Included(mut end) => key::next(&mut end).then_some(end),
            Bound::Excluded(end) => {
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
unsafe impl marker::Sync for Db {}
unsafe impl marker::Send for Db {}

/// Watermarks are identified by their checkpoint, so comparison can be limited to them.
impl Ord for Watermark {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.checkpoint_hi_inclusive
            .cmp(&other.checkpoint_hi_inclusive)
    }
}

impl PartialOrd for Watermark {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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

/// Retrieves a RocksDB property from db and maps it to a metric value.
fn cf_property_int_to_metric(
    db: &rocksdb::DB,
    cf: &impl AsColumnFamilyRef,
    property_name: &std::ffi::CStr,
) -> i64 {
    match db.property_int_value_cf(cf, property_name) {
        Ok(Some(value)) => value.min(i64::MAX as u64) as i64,
        Ok(None) | Err(_) => METRICS_ERROR,
    }
}

impl Default for RocksMetrics {
    fn default() -> Self {
        Self {
            current_size_active_mem_tables: METRICS_ERROR,
            size_all_mem_tables: METRICS_ERROR,
            block_cache_usage: METRICS_ERROR,
            block_cache_pinned_usage: METRICS_ERROR,
            estimate_table_readers_mem: METRICS_ERROR,
            estimate_pending_compaction_bytes: METRICS_ERROR,
            num_level0_files: METRICS_ERROR,
            actual_delayed_write_rate: METRICS_ERROR,
            is_write_stopped: METRICS_ERROR,
            num_immutable_mem_tables: METRICS_ERROR,
            mem_table_flush_pending: METRICS_ERROR,
            compaction_pending: METRICS_ERROR,
            num_snapshots: METRICS_ERROR,
            num_running_compactions: METRICS_ERROR,
            num_running_flushes: METRICS_ERROR,
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

        db.take_snapshot(wm(0));
        assert!(db.get::<u64, u64>(0, &cf, &42u64).unwrap().is_none());
    }

    #[test]
    fn test_snapshot_circular_buffer() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
        let cf = db.cf("test").unwrap();

        for i in 0..10 {
            db.take_snapshot(wm(i));
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
        db.take_snapshot(wm(0));

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
            db.take_snapshot(wm(1));
            assert_eq!(db.get(1, &cf, &k).unwrap(), Some(v0));
        }

        {
            // The value is still not present in the previous snapshot.
            assert_eq!(db.get(0, &cf, &k).unwrap(), None::<u64>);
        }

        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&cf, key::encode(&k), bcs::to_bytes(&v1).unwrap());
        db.write("test", wm(2), batch).unwrap();
        db.take_snapshot(wm(2));

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
        db.take_snapshot(wm(0));

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
        db.take_snapshot(wm(1));

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
        assert_eq!(db.commit_watermark("p0").unwrap(), None);
        assert_eq!(db.commit_watermark("p1").unwrap(), None);

        // Write a batch for the pipeline p0.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p0, key::encode(&42u64), bcs::to_bytes(&43u64).unwrap());
        db.write("p0", wm(0), batch).unwrap();

        // Wrote to one pipeline, but not the other, unlike the data itself, watermarks are not
        // read from snapshots.
        assert_eq!(db.commit_watermark("p0").unwrap(), Some(wm(0)));
        assert_eq!(db.commit_watermark("p1").unwrap(), None);

        // Write a batch for the pipeline p1.
        let mut batch = rocksdb::WriteBatch::default();
        batch.put_cf(&p1, key::encode(&44u64), bcs::to_bytes(&45u64).unwrap());
        db.write("p1", wm(1), batch).unwrap();

        // Wrote to both pipelines.
        assert_eq!(db.commit_watermark("p0").unwrap(), Some(wm(0)));
        assert_eq!(db.commit_watermark("p1").unwrap(), Some(wm(1)));
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
            assert_eq!(db.commit_watermark("test").unwrap(), Some(wm(1)));

            // ...and once there is a snapshot, the data can be read.
            db.take_snapshot(wm(1));
            assert_eq!(db.get(1, &cf, &42u64).unwrap(), Some(43u64));
        }

        {
            // Re-open the database.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            let cf = db.cf("test").unwrap();

            // The `watermark` persists.
            assert_eq!(db.commit_watermark("test").unwrap(), Some(wm(1)));

            // The snapshots do not, however, so reads will fail.
            let err = db.get::<u64, u64>(1, &cf, &42u64).unwrap_err();
            assert!(
                matches!(err, Error::NotInRange { checkpoint: 1 }),
                "Unexpected error: {err:?}"
            );

            // But once the snapshot has been taken, the data is still there.
            db.take_snapshot(wm(1));
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
        db.take_snapshot(wm(0));

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

        // Raw values
        let mut iter = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        for i in (0u64..=8).step_by(2) {
            let k = key::encode(&i);
            let v = bcs::to_bytes(&(i + 1)).unwrap();

            assert_eq!(iter.raw_key(), Some(k.as_ref()), "key {i}");
            assert_eq!(iter.raw_value(), Some(v.as_ref()), "value {}", i + 1);
            assert_eq!(iter.next().unwrap().unwrap(), (i, i + 1));
        }
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
        db.take_snapshot(wm(0));

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(key::encode(&4u64));
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "exact seek");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(key::encode(&3u64));
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "inexact seek");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(0, 1), (2, 3), (4, 5)], "take 3");
        iter.seek(key::encode(&2u64));
        assert_eq!(iter.next().unwrap().unwrap(), (2, 3), "rewind");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(0, 1), (2, 3), (4, 5)], "take 3");
        iter.seek(key::encode(&7u64));
        assert_eq!(iter.next().unwrap().unwrap(), (8, 9), "fast forward");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, 4u64..8).unwrap();
        iter.seek(key::encode(&1u64));
        assert_eq!(iter.next().unwrap().unwrap(), (4, 5), "underflow");

        let mut iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, 4u64..8).unwrap();
        iter.seek(key::encode(&8u64));
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
        db.take_snapshot(wm(0));

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
        db.take_snapshot(wm(1));

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
        db.take_snapshot(wm(2));

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
        db.take_snapshot(wm(0));

        // Create an iterator from the first snapshot.
        let iter: iter::FwdIter<u64, u64> = db.iter(0, &cf, (U::<u64>, U)).unwrap();

        // Create more snapshots...
        for i in 1..5 {
            db.take_snapshot(wm(i));
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
        db.take_snapshot(wm(0));

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

        // Raw values
        let mut iter = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        for i in (1u64..=9).rev().step_by(2) {
            let k = key::encode(&i);
            let v = bcs::to_bytes(&(i - 1)).unwrap();

            assert_eq!(iter.raw_key(), Some(k.as_ref()), "key {i}");
            assert_eq!(iter.raw_value(), Some(v.as_ref()), "value {}", i - 1);
            assert_eq!(iter.next().unwrap().unwrap(), (i, i - 1));
        }
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
        db.take_snapshot(wm(0));

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(key::encode(&5u64));
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "exact seek");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        iter.seek(key::encode(&6u64));
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "inexact seek");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(9, 8), (7, 6), (5, 4)], "take 3");
        iter.seek(key::encode(&7u64));
        assert_eq!(iter.next().unwrap().unwrap(), (7, 6), "rewind");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, (U::<u64>, U)).unwrap();
        let prefix: Result<Vec<(u64, u64)>, Error> = (&mut iter).take(3).collect();
        assert_eq!(prefix.unwrap(), vec![(9, 8), (7, 6), (5, 4)], "take 3");
        iter.seek(key::encode(&1u64));
        assert_eq!(iter.next().unwrap().unwrap(), (1, 0), "fast forward");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, 3u64..7).unwrap();
        iter.seek(key::encode(&9u64));
        assert_eq!(iter.next().unwrap().unwrap(), (5, 4), "underflow");

        let mut iter: iter::RevIter<u64, u64> = db.iter_rev(0, &cf, 3u64..7).unwrap();
        iter.seek(key::encode(&1u64));
        assert!(iter.next().is_none(), "overflow");
    }

    #[test]
    fn test_start_restoration() {
        let d = tempfile::tempdir().unwrap();

        {
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            db.restore_at("p0", wm(10)).unwrap();
        }

        {
            // Restoration is idempotent.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            db.restore_at("p0", wm(10)).unwrap();
        }

        {
            // Cannot start restoration at a new watermark while one is in progress.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            let err = db.restore_at("p0", wm(20)).unwrap_err();
            assert!(
                matches!(err, Error::RestoreInProgress(_)),
                "Unexpected error: {err:?}"
            );
        }

        {
            // Different pipelines may be restored at different watermarks.
            let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();
            db.restore_at("p1", wm(20)).unwrap();
        }
    }

    #[test]
    fn test_is_restored() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();

        // Start restoration for two pipelines
        db.restore_at("p0", wm(10)).unwrap();
        db.restore_at("p1", wm(10)).unwrap();

        // Both should require restoration for (0, 0), and (1, 1)
        let r = db.is_restored(0, 0, ["p0", "p1"]).unwrap();
        assert_eq!(r, vec![false, false]);

        let r = db.is_restored(1, 1, ["p0", "p1"]).unwrap();
        assert_eq!(r, vec![false, false]);

        // Write to one of the pipelines for (0, 0)
        let batch = rocksdb::WriteBatch::default();
        db.restore(0, 0, "p0", batch).unwrap();

        // p0 has restored (0, 0), p1 has not. (1, 1) is empty for both.
        let r = db.is_restored(0, 0, ["p0", "p1"]).unwrap();
        assert_eq!(r, vec![true, false]);

        let r = db.is_restored(1, 1, ["p0", "p1"]).unwrap();
        assert_eq!(r, vec![false, false]);

        // Write to the other pipeline for (0, 0)
        let batch = rocksdb::WriteBatch::default();
        db.restore(0, 0, "p1", batch).unwrap();

        // (0, 0) is fully restored, and (1, 1) is not.
        let result = db.is_restored(0, 0, ["p0", "p1"]).unwrap();
        assert_eq!(result, vec![true, true]);

        let result = db.is_restored(1, 1, ["p0", "p1"]).unwrap();
        assert_eq!(result, vec![false, false]);
    }

    #[test]
    fn test_complete_restore() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(d.path().join("db"), opts(), 4, cfs()).unwrap();

        // Create restoration markers for several pipeline that sit next to each other in the
        // database.
        db.restore_at("tess", wm(10)).unwrap();
        db.restore_at("test", wm(20)).unwrap();
        db.restore_at("tesu", wm(30)).unwrap();
        assert_eq!(db.restore_watermark("tess").unwrap(), Some(wm(10)));
        assert_eq!(db.restore_watermark("test").unwrap(), Some(wm(20)));
        assert_eq!(db.restore_watermark("tesu").unwrap(), Some(wm(30)));

        // Mark several buckets/partitions as restored for all pipelines
        for (bucket, partition) in [(0, 0), (0, 1)] {
            for pipeline in ["tess", "test", "tesu"] {
                let batch = rocksdb::WriteBatch::default();
                db.restore(bucket, partition, pipeline, batch).unwrap();
            }
        }

        // Verify the restoration markers exist
        let restored = db.is_restored(0, 0, ["tess", "test", "tesu"]).unwrap();
        assert_eq!(restored, vec![true; 3]);
        let restored = db.is_restored(0, 1, ["tess", "test", "tesu"]).unwrap();
        assert_eq!(restored, vec![true; 3]);

        // Complete the restoration and verify restoration happened for just the requested
        // pipeline, while the other pipelines are unaffected.
        db.complete_restore("test").unwrap();
        assert_eq!(db.restore_watermark("tess").unwrap(), Some(wm(10)));
        assert_eq!(db.restore_watermark("test").unwrap(), None);
        assert_eq!(db.restore_watermark("tesu").unwrap(), Some(wm(30)));

        // Verify all restoration markers are gone for `test` pipeline, while the others remain.
        let restored = db.is_restored(0, 0, ["tess", "test", "tesu"]).unwrap();
        assert_eq!(restored, vec![true, false, true]);
        let restored = db.is_restored(0, 1, ["tess", "test", "tesu"]).unwrap();
        assert_eq!(restored, vec![true, false, true]);

        // Verify commit watermark is now set to the restoration watermark
        assert_eq!(db.commit_watermark("test").unwrap(), Some(wm(20)));

        // Verify it's no longer possible to run another restore
        let err = db.restore_at("test", wm(20)).unwrap_err();
        assert!(
            matches!(err, Error::RestoreOverwrite),
            "Expected RestoreOverwrite, got: {err:?}"
        );

        // But for the other pipelines, it's possible to resume restore.
        db.restore_at("tess", wm(10)).unwrap();
        db.restore_at("tesu", wm(30)).unwrap();
    }
}

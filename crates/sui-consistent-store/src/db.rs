// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The [`Db`] handle wrapping an opened RocksDB database.
//!
//! [`Db`] is a cheap-to-clone handle: every clone is one `Arc` bump
//! against a shared inner that holds the actual RocksDB
//! database. Clones are shared freely across typed column-family
//! handles and across threads; the database itself stays alive
//! until the last clone drops, at which point RocksDB's own
//! shutdown sequence (flush plus close) runs.
//!
//! `Db` also holds the in-memory snapshot buffer used to serve
//! consistent reads at a given checkpoint. See [`take_snapshot`],
//! [`at_snapshot`], and [`Snapshot`].
//!
//! RocksDB is internally thread-safe; the only external locking the
//! crate adds is a [`parking_lot::RwLock`] over the snapshot buffer.
//!
//! [`take_snapshot`]: Db::take_snapshot
//! [`at_snapshot`]: Db::at_snapshot

use std::collections::BTreeMap;
use std::fmt;
use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::Arc;
use std::sync::Weak;

use parking_lot::RwLock;
use rocksdb::BoundColumnFamily;

use crate::batch::Batch;
use crate::error::Error;
use crate::error::OpenError;
use crate::framework::FRAMEWORK_CFS;
use crate::framework::FrameworkSchema;
use crate::framework::Watermark;
use crate::options::CfOptionsResolver;
use crate::options::RocksDbConfig;
use crate::schema::CfDescriptor;
use crate::schema::Schema;
use crate::snapshot::Snapshot;

/// Configuration for opening a [`Db`].
///
/// The default value leaves every RocksDB knob at its native default
/// (besides `create_if_missing` / `create_missing_column_families`,
/// which [`Db::open`] always sets). Populate [`rocksdb`](Self::rocksdb)
/// to tune database-wide and per-column-family options; see
/// [`RocksDbConfig`].
///
/// # Examples
///
/// ```
/// use sui_consistent_store::DbOptions;
///
/// let mut opts = DbOptions::default();
/// // More background threads for compactions and flushes.
/// opts.rocksdb.db.parallelism = Some(8);
/// ```
pub struct DbOptions {
    /// Tunable RocksDB options applied database-wide and per column
    /// family when the database is opened.
    pub rocksdb: RocksDbConfig,

    /// Maximum number of in-memory snapshots retained on the database
    /// at any one time. When [`Db::take_snapshot`] is called and the
    /// buffer is at capacity, the snapshot with the lowest checkpoint
    /// number is evicted. Set high enough to retain the consistency
    /// window the application requires; long-lived snapshots pressure
    /// RocksDB compaction, so this is not free.
    ///
    /// Set to `0` to disable snapshotting entirely: [`Db::take_snapshot`]
    /// becomes a no-op and no snapshot-related work is performed.
    pub snapshot_capacity: usize,
}

/// An opened RocksDB database.
///
/// `Db` is not constructed directly; obtain one via [`Db::open`],
/// which also constructs the typed schema struct that names its
/// column families. `Db` is `Clone` and cheap to clone — every
/// clone shares the same underlying database via an internal
/// [`Arc`], so handles can be freely passed by value to typed
/// column-family wrappers and across threads.
///
/// # Examples
///
/// ```
/// use sui_consistent_store::CfDescriptor;
/// use sui_consistent_store::Db;
/// use sui_consistent_store::DbOptions;
/// use sui_consistent_store::Schema;
/// use sui_consistent_store::error::OpenError;
///
/// struct MySchema {
///     _db: Db,
/// }
///
/// impl Schema for MySchema {
///     fn cfs(opts: &sui_consistent_store::CfOptionsResolver) -> Vec<CfDescriptor> {
///         vec![CfDescriptor::new("my_cf", opts.options("my_cf"))]
///     }
///
///     fn open(db: &Db) -> Result<Self, OpenError> {
///         Ok(Self { _db: db.clone() })
///     }
/// }
///
/// let dir = tempfile::tempdir().unwrap();
/// let (_db, _schema) = Db::open::<MySchema>(dir.path(), DbOptions::default()).unwrap();
/// ```
#[derive(Clone)]
pub struct Db {
    inner: Arc<DbInner>,
}

/// A weak handle to a [`Db`] that does not keep the underlying
/// database open.
///
/// Construct with [`Db::downgrade`]. Promote back to a strong
/// [`Db`] handle with [`upgrade`](DbRef::upgrade), which returns
/// `None` once every strong [`Db`] has been dropped.
///
/// Intended for long-lived observers (Prometheus collectors,
/// background tasks) that should not pin the database alive after
/// the application has released it. Cloning is cheap: one `Arc`
/// weak-count bump.
#[derive(Clone)]
pub struct DbRef {
    inner: Weak<DbInner>,
}

impl DbRef {
    /// Try to obtain a strong [`Db`] handle. Returns `None` if
    /// every strong handle has already been dropped (the database
    /// is closed or closing).
    pub fn upgrade(&self) -> Option<Db> {
        self.inner.upgrade().map(|inner| Db { inner })
    }
}

impl fmt::Debug for DbRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbRef")
            .field("alive", &(self.inner.strong_count() > 0))
            .finish_non_exhaustive()
    }
}

/// The shared storage backing a [`Db`].
///
/// Held inside an [`Arc`] inside [`Db`] so every clone of the
/// public handle co-owns the same underlying database. The last
/// clone's drop triggers RocksDB's own shutdown sequence.
///
/// Field declaration order is load-bearing: `snapshots` is
/// declared *before* `db` so, on drop, every retained snapshot
/// drops (and releases its borrow on `db`) before `db` itself is
/// freed.
struct DbInner {
    snapshots: RwLock<BTreeMap<u64, Arc<SnapshotEntry>>>,
    snapshot_capacity: usize,
    /// The set of column-family names registered when this database
    /// was opened. Used by [`Db::cf_names`] so observability
    /// helpers (e.g. the Prometheus column-family stats collector)
    /// can iterate the CFs the database knows about without having
    /// to enumerate them off disk. Stored as `&'static str`
    /// because [`CfDescriptor::name`] already requires that.
    cf_names: Vec<&'static str>,
    db: rocksdb::DB,
}

/// Storage for a single snapshot. The contained [`rocksdb::Snapshot`]
/// borrows from [`DbInner::db`]; the borrow's lifetime is extended
/// to `'static` via [`std::mem::transmute`] inside
/// [`Db::take_snapshot`] so the snapshot can be stored in a
/// long-lived map. Two invariants make this sound:
///
/// 1. `DbInner::db` is declared after `DbInner::snapshots`, so `db`
///    is dropped only after every retained snapshot has dropped
///    (and released its borrow).
/// 2. Outstanding [`Snapshot`](crate::Snapshot) values co-own the
///    same [`Db`] handle (and therefore the same [`Arc<DbInner>`]),
///    so `DbInner` cannot drop while a `Snapshot` exists. Field
///    ordering inside `Snapshot` ensures the `Arc<SnapshotEntry>`
///    drops before the `Db`.
pub(crate) struct SnapshotEntry {
    snapshot: rocksdb::Snapshot<'static>,
    watermark: Watermark,
}

impl SnapshotEntry {
    pub(crate) fn as_snapshot(&self) -> &rocksdb::Snapshot<'static> {
        &self.snapshot
    }

    pub(crate) fn watermark(&self) -> Watermark {
        self.watermark
    }
}

impl Default for DbOptions {
    fn default() -> Self {
        Self {
            rocksdb: RocksDbConfig::default(),
            snapshot_capacity: 32,
        }
    }
}

impl fmt::Debug for DbOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `rocksdb::Options` does not implement Debug, so summarize.
        f.debug_struct("DbOptions").finish_non_exhaustive()
    }
}

impl fmt::Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `rocksdb::DB` does not implement Debug; print only the path.
        f.debug_struct("Db")
            .field("path", &self.inner.db.path())
            .finish_non_exhaustive()
    }
}

impl Db {
    /// Open a database at `path` with the given schema.
    ///
    /// Column families named by [`Schema::cfs`] that do not yet exist
    /// on disk are created — [`Db::open`] always sets
    /// `create_if_missing` and `create_missing_column_families`.
    /// RocksDB opens its mandatory `default` column family on its own;
    /// the schema neither declares nor uses it.
    ///
    /// The framework's bookkeeping column families (`__restore`,
    /// `__watermark`, `__chain_id`) are registered automatically. A
    /// schema that declares one of these names is rejected, since its
    /// handle would shadow the framework's own.
    ///
    /// Per-CF and database-wide RocksDB options come from
    /// [`DbOptions::rocksdb`]; they are validated before the database
    /// is opened, and a per-CF override naming a column family the
    /// schema does not declare is rejected.
    ///
    /// On success, returns the database handle (cheap to clone via
    /// the inner [`Arc`], so it can be shared with column-family
    /// wrappers) and the constructed schema.
    pub fn open<S: Schema>(
        path: impl AsRef<Path>,
        opts: DbOptions,
    ) -> Result<(Self, S), OpenError> {
        let DbOptions {
            rocksdb,
            snapshot_capacity,
        } = opts;

        let resolver = CfOptionsResolver::new(rocksdb)?;
        let db_options = resolver.db_options();

        let mut cfs = S::cfs(&resolver);

        // The framework owns its bookkeeping CFs (restore, watermark,
        // chain-id); a consumer schema must not declare one itself, or
        // its handle would shadow the framework's and [`Db::framework`]
        // would read and write the wrong column family. RocksDB opens
        // its mandatory `default` column family on its own, so the
        // schema never declares that either.
        for cf in &cfs {
            if FRAMEWORK_CFS.contains(&cf.name) {
                return Err(OpenError::msg(format!(
                    "schema column family `{}` is reserved by the framework",
                    cf.name
                )));
            }
        }

        // Auto-register the framework's bookkeeping CFs so
        // [`FrameworkSchema`] is always available via [`Db::framework`]
        // without the consumer having to declare it.
        for name in FRAMEWORK_CFS {
            cfs.push(CfDescriptor::new(name, resolver.options(name)));
        }

        let cf_names: Vec<&'static str> = cfs.iter().map(|cf| cf.name).collect();

        // Reject per-CF overrides that name a column family this schema
        // does not declare — almost always a typo in configuration.
        for configured in resolver.configured_cf_names() {
            if !cf_names.contains(&configured) {
                return Err(OpenError::msg(format!(
                    "rocksdb config names unknown column family `{configured}`"
                )));
            }
        }

        let descriptors = cfs
            .into_iter()
            .map(|cf| rocksdb::ColumnFamilyDescriptor::new(cf.name, cf.options));
        let path = path.as_ref();
        let db = rocksdb::DB::open_cf_descriptors(&db_options, path, descriptors)?;
        tracing::info!(path = %path.display(), "opened consistent-store database");

        let db = Self {
            inner: Arc::new(DbInner {
                snapshots: RwLock::new(BTreeMap::new()),
                snapshot_capacity,
                cf_names,
                db,
            }),
        };
        let schema = S::open(&db)?;
        Ok((db, schema))
    }

    /// Names of the column families that were registered when this
    /// database was opened — both schema-declared CFs and the
    /// framework-internal ones (`__restore`, `__watermark`,
    /// `__chain_id`). Order matches what [`Schema::cfs`] returned,
    /// with the framework CFs appended.
    ///
    /// RocksDB's mandatory `default` column family is not included:
    /// the store never uses it, so it is left untracked.
    pub fn cf_names(&self) -> &[&'static str] {
        &self.inner.cf_names
    }

    /// Returns `true` if `self` and `other` are handles to the same
    /// underlying database (i.e. clones of each other), `false`
    /// otherwise. The comparison is a single pointer equality on
    /// the shared inner [`Arc`].
    pub fn ptr_eq(&self, other: &Db) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Build a weak handle to this database. The returned [`DbRef`]
    /// does not contribute to the owner count; the underlying
    /// database closes when the last strong [`Db`] handle drops,
    /// independent of how many [`DbRef`]s remain.
    ///
    /// Use this for long-lived observers (Prometheus collectors,
    /// background log scrapers) that should not pin the database
    /// alive after the rest of the application has released it.
    pub fn downgrade(&self) -> DbRef {
        DbRef {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Borrowed handle to the auto-registered [`FrameworkSchema`].
    ///
    /// Returns a `FrameworkSchema<&Db>` borrowing `self`. Zero
    /// `Arc` bumps; the returned schema is scoped to the borrow.
    /// Use for ad-hoc reads (or read-mostly access). For an owned
    /// handle to hold inside a longer-lived struct, construct one
    /// with [`FrameworkSchema::new(db.clone())`](FrameworkSchema::new).
    pub fn framework(&self) -> FrameworkSchema<&Db> {
        FrameworkSchema::new(self)
    }

    /// Look up a column family handle by name.
    ///
    /// Returns `None` if no column family with the given name was
    /// registered when the database was opened. The returned handle
    /// borrows from `self`; callers must not retain it beyond that
    /// borrow.
    pub(crate) fn cf_handle(&self, name: &str) -> Option<Arc<BoundColumnFamily<'_>>> {
        self.inner.db.cf_handle(name)
    }

    /// Borrow the underlying RocksDB handle.
    ///
    /// Used by typed wrappers (`DbMap`) to call read and write methods
    /// on the database. Not part of the public API.
    pub(crate) fn rocksdb(&self) -> &rocksdb::DB {
        &self.inner.db
    }

    /// Construct an empty atomic write batch tied to this database.
    ///
    /// Stage operations against the returned [`Batch`] using
    /// [`Batch::put`] and [`Batch::delete`], then call
    /// [`Batch::commit`] to apply them atomically.
    pub fn batch(&self) -> Batch {
        Batch::new(self.clone())
    }

    /// Take a snapshot of the database state and store it under
    /// `watermark.checkpoint_hi_inclusive`.
    ///
    /// Snapshots are point-in-time consistent views of the data; a
    /// subsequent [`at_snapshot`](Self::at_snapshot) lookup at the
    /// same checkpoint returns a handle that reads from this state
    /// regardless of any writes that happen after this call returns.
    /// The full `watermark` is retained alongside the snapshot and
    /// can be recovered via [`Snapshot::watermark`](crate::Snapshot::watermark);
    /// downstream readers use it to recover the chain state
    /// (epoch / tx count / timestamp) the snapshot was taken at.
    ///
    /// Both the snapshot capture and the buffer insertion happen
    /// while holding the snapshot buffer's write lock, so concurrent
    /// `take_snapshot` calls are fully serialized: the snapshot's
    /// observed state and its order in the buffer are committed
    /// together. Callers who require a strict ordering between
    /// writes and the snapshot's state still need to fence external
    /// writers themselves; this method only ensures internal
    /// consistency between competing `take_snapshot` callers.
    ///
    /// If a snapshot already exists at this checkpoint, it is
    /// replaced. If the buffer is at
    /// [`DbOptions::snapshot_capacity`](crate::DbOptions::snapshot_capacity),
    /// the snapshot with the lowest checkpoint number is evicted. If
    /// capacity is `0`, snapshotting is disabled and this call is a
    /// no-op.
    pub fn take_snapshot(&self, watermark: Watermark) {
        // The rocksdb snapshot is captured *inside* the lock so that
        // two concurrent `take_snapshot(N)` calls cannot land in
        // checkpoint-N → older-state order: with the capture outside
        // the lock, a thread that captures earlier may insert later
        // and overwrite a fresher snapshot.
        let mut snaps = self.inner.snapshots.write();
        let snapshot = self.inner.db.snapshot();
        // SAFETY: `rocksdb::Snapshot<'_>` is a borrow of
        // `self.inner.db`. The transmute to `'static` is sound
        // because (1) `DbInner::db` is declared after
        // `DbInner::snapshots`, so `db` outlives every snapshot
        // retained in the map; and (2) `Snapshot`s co-own the same
        // [`Db`] handle (and therefore the same `Arc<DbInner>`) and
        // drop their `Arc<SnapshotEntry>` before their `Db`, so no
        // snapshot can survive a `DbInner` drop.
        let snapshot: rocksdb::Snapshot<'static> = unsafe { std::mem::transmute(snapshot) };
        let entry = Arc::new(SnapshotEntry {
            snapshot,
            watermark,
        });

        snaps.insert(watermark.checkpoint_hi_inclusive, entry);
        while snaps.len() > self.inner.snapshot_capacity {
            snaps.pop_first();
        }
    }

    /// Look up the snapshot stored at `checkpoint`.
    ///
    /// Returns `None` if no snapshot exists at that checkpoint.
    /// Cloning the returned [`Snapshot`] is cheap;
    /// clones share the same underlying snapshot.
    pub fn at_snapshot(&self, checkpoint: u64) -> Option<Snapshot> {
        let snaps = self.inner.snapshots.read();
        let entry = snaps.get(&checkpoint)?.clone();
        Some(Snapshot::new(self.clone(), entry))
    }

    /// Look up the snapshot with the highest checkpoint number in
    /// the buffer.
    ///
    /// Returns `None` if no snapshots have been taken (or all have
    /// been evicted or dropped). Equivalent to
    /// [`at_snapshot`](Self::at_snapshot) called with the upper
    /// bound of [`snapshot_range`](Self::snapshot_range).
    pub fn latest_snapshot(&self) -> Option<Snapshot> {
        let snaps = self.inner.snapshots.read();
        let (_, entry) = snaps.iter().next_back()?;
        Some(Snapshot::new(self.clone(), entry.clone()))
    }

    /// Returns the inclusive range of checkpoints covered by the
    /// snapshot buffer, or `None` if the buffer is empty.
    pub fn snapshot_range(&self) -> Option<RangeInclusive<u64>> {
        let snaps = self.inner.snapshots.read();
        let lo = *snaps.keys().next()?;
        let hi = *snaps.keys().next_back()?;
        Some(lo..=hi)
    }

    /// Apply caller-supplied runtime-mutable options to `cf_name`.
    ///
    /// Thin typed wrapper around
    /// [`rocksdb::DB::set_options_cf`] that surfaces an unknown
    /// column-family name as
    /// [`crate::error::Error::MissingColumnFamily`]
    /// rather than as a generic RocksDB error.
    ///
    /// `opts` is a slice of `(name, value)` pairs. Names must come
    /// from RocksDB's runtime-mutable options list (see
    /// [`advanced_options.h`]); unknown or non-mutable names fail
    /// with [`crate::error::Error::Rocksdb`].
    ///
    /// An empty `opts` slice is a no-op (returns `Ok(())` without
    /// touching RocksDB); the unknown-CF check is still applied.
    /// RocksDB itself rejects an empty option set with an
    /// `Invalid argument: empty input` error, but at the typed
    /// surface "apply nothing" is a sensible default for callers
    /// that build option lists dynamically.
    ///
    /// [`advanced_options.h`]: https://github.com/facebook/rocksdb/blob/main/include/rocksdb/advanced_options.h
    pub fn set_options_cf(&self, cf_name: &str, opts: &[(&str, &str)]) -> Result<(), Error> {
        let cf = self
            .cf_handle(cf_name)
            .ok_or_else(|| Error::MissingColumnFamily(cf_name.to_string()))?;
        if opts.is_empty() {
            return Ok(());
        }
        self.inner.db.set_options_cf(&cf, opts)?;
        Ok(())
    }

    /// Apply restore-friendly compaction settings to `cf_name`.
    ///
    /// Disables auto-compaction and raises the three L0 triggers to
    /// ceiling values so a bulk load that produces many L0 files
    /// does not stall on slowdown / stop thresholds.
    ///
    /// Pairs with [`set_tip_options_cf`](Self::set_tip_options_cf):
    /// the application transitions restore → tip with the matching
    /// toggle and no reopen. Both toggles touch only runtime-mutable
    /// option keys, so this method can be called at any time on an
    /// open database.
    ///
    /// The keys touched are:
    /// - `disable_auto_compactions = true`
    /// - `level0_file_num_compaction_trigger = i32::MAX`
    /// - `level0_slowdown_writes_trigger = -1` (the disabled
    ///   sentinel)
    /// - `level0_stop_writes_trigger = i32::MAX`
    ///
    /// # Schema-set tip defaults are not preserved
    ///
    /// Per-CF tip-mode values for these specific keys set via
    /// [`crate::Schema::cfs`] at open time are *not*
    /// captured for reversal by
    /// [`set_tip_options_cf`](Self::set_tip_options_cf), which
    /// applies RocksDB defaults. A schema that needs non-default
    /// values for these knobs at tip should re-apply them via
    /// [`set_options_cf`](Self::set_options_cf) after the tip-mode
    /// toggle.
    pub fn set_restore_options_cf(&self, cf_name: &str) -> Result<(), Error> {
        // The string values are RocksDB option-parser inputs:
        // bools are "true"/"false", integers are decimal strings.
        let max = i32::MAX.to_string();
        self.set_options_cf(
            cf_name,
            &[
                ("disable_auto_compactions", "true"),
                ("level0_file_num_compaction_trigger", &max),
                ("level0_slowdown_writes_trigger", "-1"),
                ("level0_stop_writes_trigger", &max),
            ],
        )
    }

    /// Apply tip-mode compaction settings to `cf_name`.
    ///
    /// Restores RocksDB defaults for the four compaction-trigger
    /// knobs raised by
    /// [`set_restore_options_cf`](Self::set_restore_options_cf).
    /// The values applied are the upstream defaults from
    /// [`advanced_options.h`]:
    ///
    /// - `disable_auto_compactions = false`
    /// - `level0_file_num_compaction_trigger = 4`
    /// - `level0_slowdown_writes_trigger = 20`
    /// - `level0_stop_writes_trigger = 36`
    ///
    /// [`advanced_options.h`]: https://github.com/facebook/rocksdb/blob/main/include/rocksdb/advanced_options.h
    pub fn set_tip_options_cf(&self, cf_name: &str) -> Result<(), Error> {
        self.set_options_cf(
            cf_name,
            &[
                ("disable_auto_compactions", "false"),
                ("level0_file_num_compaction_trigger", "4"),
                ("level0_slowdown_writes_trigger", "20"),
                ("level0_stop_writes_trigger", "36"),
            ],
        )
    }

    /// Flush every registered column family's memtable to disk,
    /// blocking until each flush completes.
    ///
    /// Useful before a graceful shutdown or before opening a
    /// [filesystem checkpoint][rocksdb::checkpoint::Checkpoint] of
    /// the database. Routine writes do not require this call;
    /// RocksDB flushes automatically as memtables fill.
    ///
    /// RocksDB's `DB::flush` C API targets only the default column
    /// family; this method walks the full set of column families
    /// registered at open time (see [`Db::cf_names`]) and issues
    /// `flush_cfs_opt` so that every CF is flushed. If
    /// [`Options::set_atomic_flush`](rocksdb::Options::set_atomic_flush)
    /// was enabled on the database options, the flushes are atomic
    /// (all CFs at one shared sequence number); otherwise the call
    /// is equivalent to flushing each CF in sequence.
    ///
    /// CFs that were dropped at runtime via [`drop_cf`](Self::drop_cf)
    /// are silently skipped; their entries remain in `cf_names` for
    /// the database's lifetime but no longer have a live handle.
    pub fn flush(&self) -> Result<(), Error> {
        let handles: Vec<Arc<BoundColumnFamily<'_>>> = self
            .inner
            .cf_names
            .iter()
            .filter_map(|name| self.inner.db.cf_handle(name))
            .collect();
        let refs: Vec<&Arc<BoundColumnFamily<'_>>> = handles.iter().collect();
        let opts = rocksdb::FlushOptions::default();
        self.inner.db.flush_cfs_opt(&refs, &opts)?;
        Ok(())
    }

    /// Trigger a manual compaction over `[start, end]` of `cf_name`.
    ///
    /// Forces RocksDB to compact the given key range now, which runs
    /// the column family's compaction filter (if one is configured)
    /// over the data and applies any pending point or range
    /// tombstones. Passing `None` for a bound compacts from the
    /// beginning / to the end of the CF; `None, None` compacts the
    /// whole CF.
    ///
    /// Routine writes do not need this — RocksDB compacts in the
    /// background. It is useful to promptly evict rows that a
    /// compaction filter would otherwise only drop on the next
    /// natural sweep (for example, after advancing a pruning floor),
    /// at the cost of the compaction work it forces.
    ///
    /// Unknown `cf_name` surfaces as
    /// [`crate::error::Error::MissingColumnFamily`].
    pub fn compact_range_cf(
        &self,
        cf_name: &str,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<(), Error> {
        let cf = self
            .cf_handle(cf_name)
            .ok_or_else(|| Error::MissingColumnFamily(cf_name.to_string()))?;
        self.inner.db.compact_range_cf(&cf, start, end);
        Ok(())
    }

    /// Wipe every column family back to empty, preserving each CF's
    /// configured options (merge operators, compaction filters stay
    /// attached because the CFs themselves are not dropped or
    /// recreated). The framework bookkeeping CFs (`__restore`,
    /// `__watermark`, `__chain_id`) are cleared too, so a subsequent
    /// restore observes a never-restored database, and any in-memory
    /// snapshots are discarded.
    ///
    /// This is the wipe-then-restore primitive: when a fullnode finds
    /// its embedded rpc-store initialized but with watermarks outside
    /// the perpetual store's available range, it cannot trust the
    /// live-object indexes and must rebuild them from a clean slate.
    ///
    /// The caller must hold no live [`Snapshot`]s and run no concurrent
    /// reads or writes; it is intended for single-threaded startup
    /// before the indexer begins. Each CF is emptied with a single
    /// range delete spanning its whole keyspace and then compacted so
    /// the range tombstones and old data do not linger into the
    /// re-restore.
    ///
    /// Possible optimization: the compaction reads every overlapping
    /// SST to produce empty output, so this is O(data size) I/O. A
    /// faster wipe would `drop_cf` each column family and recreate it,
    /// which unlinks SSTs in O(file count). That requires re-deriving
    /// each CF's options from the schema (merge operators and
    /// compaction filters live in `Schema::cfs`, not on the open
    /// `Db`), so it would make this method generic over the schema and
    /// take the [`RocksDbConfig`]. Worth pursuing
    /// if the wipe-then-restore path proves slow on large databases.
    pub fn clear_all(&self) -> Result<(), Error> {
        let mut batch = rocksdb::WriteBatch::default();
        let mut cleared = Vec::new();
        for name in &self.inner.cf_names {
            // CFs dropped at runtime via `drop_cf` have no handle; their
            // data is already gone, so there is nothing to clear.
            let Some(cf) = self.inner.db.cf_handle(name) else {
                continue;
            };
            // `delete_range_cf` is a half-open `[from, to)`; appending a
            // zero byte to the last key yields a bound strictly greater
            // than every key present, so the whole keyspace is covered.
            let mut iter = self.inner.db.raw_iterator_cf(&cf);
            iter.seek_to_last();
            if let Some(last) = iter.key() {
                let mut end = last.to_vec();
                end.push(0);
                let empty: &[u8] = &[];
                batch.delete_range_cf(&cf, empty, end.as_slice());
            }
            drop(iter);
            cleared.push(cf);
        }
        self.inner.db.write(batch)?;
        for cf in &cleared {
            self.inner
                .db
                .compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
        self.inner.snapshots.write().clear();
        Ok(())
    }

    /// Drop the snapshot at `checkpoint`. Returns `true` if a
    /// snapshot was removed.
    ///
    /// Outstanding [`Snapshot`] values for this
    /// checkpoint remain usable until they themselves drop; only the
    /// buffer's reference is released.
    pub fn drop_snapshot(&self, checkpoint: u64) -> bool {
        self.inner.snapshots.write().remove(&checkpoint).is_some()
    }

    /// Drop a column family at runtime.
    ///
    /// Returns an [`Error`] if the column family does not exist or
    /// the underlying RocksDB call fails. After a successful drop,
    /// any outstanding [`DbMap`](crate::DbMap) handle that targeted
    /// the dropped CF will fail subsequent operations with
    /// [`Error::MissingColumnFamily`].
    ///
    /// The caller is responsible for ensuring no other thread is
    /// concurrently issuing reads or writes against the CF being
    /// dropped — a concurrent operation is technically synchronized
    /// by RocksDB but may surface as a `MissingColumnFamily` error
    /// at an unpredictable moment.
    pub fn drop_cf(&self, cf_name: &str) -> Result<(), Error> {
        self.inner.db.drop_cf(cf_name)?;
        Ok(())
    }

    /// Read RocksDB's per-column-family runtime properties for
    /// `cf_name`.
    ///
    /// Returns a [`RocksMetrics`] struct populated from RocksDB's
    /// `property_int_value_cf` API. Fields default to `-1` when the
    /// column family is not registered or RocksDB cannot report a
    /// value (some properties depend on subsystems that are not
    /// always active, for example blob-file totals on a CF without
    /// blob storage configured).
    pub fn cf_metrics(&self, cf_name: &str) -> RocksMetrics {
        let Some(cf) = self.cf_handle(cf_name) else {
            return RocksMetrics::default();
        };
        let read = |property: &str| -> i64 {
            self.inner
                .db
                .property_int_value_cf(&cf, property)
                .ok()
                .flatten()
                .map(|v| v as i64)
                .unwrap_or(METRICS_ERROR)
        };
        RocksMetrics {
            block_cache_capacity: read("rocksdb.block-cache-capacity"),
            block_cache_usage: read("rocksdb.block-cache-usage"),
            block_cache_pinned_usage: read("rocksdb.block-cache-pinned-usage"),
            current_size_active_mem_tables: read("rocksdb.cur-size-active-mem-table"),
            size_all_mem_tables: read("rocksdb.size-all-mem-tables"),
            num_immutable_mem_tables: read("rocksdb.num-immutable-mem-table"),
            mem_table_flush_pending: read("rocksdb.mem-table-flush-pending"),
            estimate_table_readers_mem: read("rocksdb.estimate-table-readers-mem"),
            num_level0_files: read("rocksdb.num-files-at-level0"),
            base_level: read("rocksdb.base-level"),
            compaction_pending: read("rocksdb.compaction-pending"),
            num_running_compactions: read("rocksdb.num-running-compactions"),
            num_running_flushes: read("rocksdb.num-running-flushes"),
            estimate_pending_compaction_bytes: read("rocksdb.estimate-pending-compaction-bytes"),
            num_snapshots: read("rocksdb.num-snapshots"),
            oldest_snapshot_time: read("rocksdb.oldest-snapshot-time"),
            estimate_oldest_key_time: read("rocksdb.estimate-oldest-key-time"),
            estimated_num_keys: read("rocksdb.estimate-num-keys"),
            background_errors: read("rocksdb.background-errors"),
            total_sst_files_size: read("rocksdb.total-sst-files-size"),
            total_blob_files_size: read("rocksdb.total-blob-file-size"),
            actual_delayed_write_rate: read("rocksdb.actual-delayed-write-rate"),
            is_write_stopped: read("rocksdb.is-write-stopped"),
        }
    }
}

/// Sentinel value used in [`RocksMetrics`] when a property is
/// unavailable: the column family is not registered, or RocksDB
/// returned an error or an empty result for the property.
const METRICS_ERROR: i64 = -1;

/// Per-column-family runtime metrics read from RocksDB on demand.
///
/// Populated by [`Db::cf_metrics`]. Each field corresponds to a
/// `rocksdb.*` integer property; fields hold `-1` when the
/// property cannot be read. The struct is plain
/// data; consumers are expected to convert it into whatever their
/// monitoring stack wants (Prometheus gauges, structured logs,
/// etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RocksMetrics {
    /// `rocksdb.block-cache-capacity` — configured size (bytes).
    pub block_cache_capacity: i64,
    /// `rocksdb.block-cache-usage` — current size (bytes).
    pub block_cache_usage: i64,
    /// `rocksdb.block-cache-pinned-usage` — bytes currently pinned
    /// (held alive by outstanding readers).
    pub block_cache_pinned_usage: i64,
    /// `rocksdb.cur-size-active-mem-table` — active memtable bytes.
    pub current_size_active_mem_tables: i64,
    /// `rocksdb.size-all-mem-tables` — active plus immutable
    /// memtable bytes.
    pub size_all_mem_tables: i64,
    /// `rocksdb.num-immutable-mem-table` — count of immutable
    /// memtables waiting to be flushed.
    pub num_immutable_mem_tables: i64,
    /// `rocksdb.mem-table-flush-pending` — `1` if a flush is
    /// pending, else `0`.
    pub mem_table_flush_pending: i64,
    /// `rocksdb.estimate-table-readers-mem` — approximate memory
    /// used by table readers (excluding the block cache).
    pub estimate_table_readers_mem: i64,
    /// `rocksdb.num-files-at-level0` — number of level-0 SST files.
    pub num_level0_files: i64,
    /// `rocksdb.base-level` — RocksDB's current base level.
    pub base_level: i64,
    /// `rocksdb.compaction-pending` — `1` if compaction is pending.
    pub compaction_pending: i64,
    /// `rocksdb.num-running-compactions` — currently running
    /// compactions.
    pub num_running_compactions: i64,
    /// `rocksdb.num-running-flushes` — currently running flushes.
    pub num_running_flushes: i64,
    /// `rocksdb.estimate-pending-compaction-bytes` — bytes the
    /// compaction backlog will rewrite.
    pub estimate_pending_compaction_bytes: i64,
    /// `rocksdb.num-snapshots` — count of unreleased
    /// `rocksdb::Snapshot` handles.
    pub num_snapshots: i64,
    /// `rocksdb.oldest-snapshot-time` — unix-time of the oldest
    /// live snapshot.
    pub oldest_snapshot_time: i64,
    /// `rocksdb.estimate-oldest-key-time` — unix-time estimate of
    /// the oldest live key.
    pub estimate_oldest_key_time: i64,
    /// `rocksdb.estimate-num-keys` — approximate live key count.
    pub estimated_num_keys: i64,
    /// `rocksdb.background-errors` — accumulated background errors.
    pub background_errors: i64,
    /// `rocksdb.total-sst-files-size` — bytes occupied by SST files.
    pub total_sst_files_size: i64,
    /// `rocksdb.total-blob-file-size` — bytes occupied by blob
    /// files.
    pub total_blob_files_size: i64,
    /// `rocksdb.actual-delayed-write-rate` — current write-rate
    /// throttling level (bytes/sec, `0` when not throttled).
    pub actual_delayed_write_rate: i64,
    /// `rocksdb.is-write-stopped` — `1` if writes are stopped.
    pub is_write_stopped: i64,
}

impl Default for RocksMetrics {
    fn default() -> Self {
        Self {
            block_cache_capacity: METRICS_ERROR,
            block_cache_usage: METRICS_ERROR,
            block_cache_pinned_usage: METRICS_ERROR,
            current_size_active_mem_tables: METRICS_ERROR,
            size_all_mem_tables: METRICS_ERROR,
            num_immutable_mem_tables: METRICS_ERROR,
            mem_table_flush_pending: METRICS_ERROR,
            estimate_table_readers_mem: METRICS_ERROR,
            num_level0_files: METRICS_ERROR,
            base_level: METRICS_ERROR,
            compaction_pending: METRICS_ERROR,
            num_running_compactions: METRICS_ERROR,
            num_running_flushes: METRICS_ERROR,
            estimate_pending_compaction_bytes: METRICS_ERROR,
            num_snapshots: METRICS_ERROR,
            oldest_snapshot_time: METRICS_ERROR,
            estimate_oldest_key_time: METRICS_ERROR,
            estimated_num_keys: METRICS_ERROR,
            background_errors: METRICS_ERROR,
            total_sst_files_size: METRICS_ERROR,
            total_blob_files_size: METRICS_ERROR,
            actual_delayed_write_rate: METRICS_ERROR,
            is_write_stopped: METRICS_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    /// Two-CF schema used by the open/close tests in this module.
    #[derive(Debug)]
    struct TestSchema {
        _db: Db,
    }

    impl Schema for TestSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
            vec![
                CfDescriptor::new("foo", opts.options("foo")),
                CfDescriptor::new("bar", opts.options("bar")),
            ]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self { _db: db.clone() })
        }
    }

    #[test]
    fn open_creates_database_with_schema_cfs() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        assert!(db.cf_handle("foo").is_some());
        assert!(db.cf_handle("bar").is_some());
    }

    #[test]
    fn default_cf_is_accessible_but_not_tracked() {
        // RocksDB always opens its mandatory `default` column family,
        // so a handle is available even though the store neither
        // declares nor tracks it.
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        assert!(db.cf_handle("default").is_some());
        assert!(!db.cf_names().contains(&"default"));
    }

    #[test]
    fn open_rejects_schema_declaring_framework_cf() {
        // A schema that declares one of the framework-internal CFs
        // must be rejected: its handle would otherwise shadow the
        // framework's own column family.
        #[derive(Debug)]
        struct ShadowSchema;

        impl Schema for ShadowSchema {
            fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
                vec![CfDescriptor::new(
                    "__watermark",
                    opts.options("__watermark"),
                )]
            }

            fn open(_: &Db) -> Result<Self, OpenError> {
                Ok(Self)
            }
        }

        let dir = TempDir::new().unwrap();
        let err = Db::open::<ShadowSchema>(dir.path(), DbOptions::default())
            .expect_err("schema declaring a framework CF must be rejected");
        assert!(err.to_string().contains("__watermark"), "{err}");
    }

    #[test]
    fn cf_handle_returns_none_for_unknown_cf() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        assert!(db.cf_handle("not_in_schema").is_none());
    }

    #[test]
    fn reopen_existing_database() {
        let dir = TempDir::new().unwrap();
        {
            let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
            assert!(db.cf_handle("foo").is_some());
        }
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        assert!(db.cf_handle("foo").is_some());
        assert!(db.cf_handle("bar").is_some());
    }

    #[test]
    fn open_on_non_directory_path_errors_with_source() {
        // `Db::open` always enables `create_if_missing`, so a missing
        // path is created rather than rejected. To exercise the
        // error path we point it at a regular file, which RocksDB
        // cannot open as a database; the resulting `OpenError` must
        // carry the underlying RocksDB error as its source.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a_regular_file");
        std::fs::write(&path, b"not a database").unwrap();
        let result = Db::open::<TestSchema>(&path, DbOptions::default());
        let err = result.expect_err("open should fail when the path is a regular file");
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn clear_all_empties_every_cf_including_framework() {
        use crate::framework::FrameworkSchema;
        use crate::framework::PipelineTaskKey;
        use crate::framework::Watermark;

        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();

        // Seed the user CFs with a spread of raw keys.
        for name in ["foo", "bar"] {
            let cf = db.cf_handle(name).unwrap();
            for k in 0u8..8 {
                db.rocksdb().put_cf(&cf, [k], [k]).unwrap();
            }
        }
        // Seed a framework watermark to confirm bookkeeping is reset too.
        let framework = FrameworkSchema::new(db.clone());
        let key = PipelineTaskKey::new("some_pipeline");
        let mut batch = db.batch();
        batch
            .put(&framework.watermarks, &key, &Watermark::for_checkpoint(42))
            .unwrap();
        batch.commit().unwrap();
        assert!(framework.watermarks.get(&key).unwrap().is_some());

        db.clear_all().unwrap();

        for name in ["foo", "bar", "__watermark", "__restore", "__chain_id"] {
            let cf = db.cf_handle(name).unwrap();
            let mut iter = db.rocksdb().raw_iterator_cf(&cf);
            iter.seek_to_first();
            assert!(!iter.valid(), "cf `{name}` should be empty after clear_all");
        }
        assert!(framework.watermarks.get(&key).unwrap().is_none());
    }

    #[test]
    fn clear_all_on_empty_db_is_a_noop() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        db.clear_all().unwrap();
        let cf = db.cf_handle("foo").unwrap();
        let mut iter = db.rocksdb().raw_iterator_cf(&cf);
        iter.seek_to_first();
        assert!(!iter.valid());
    }

    #[test]
    fn flush_succeeds_on_open_db() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        db.flush().unwrap();
    }

    #[test]
    fn flush_drains_non_default_cf_memtables() {
        // Write into a non-default CF, flush, and confirm an L0
        // SST file landed for that CF. Guards against the
        // regression where `Db::flush` only flushed the default
        // CF (the C API's default behavior) and silently left
        // writes to schema CFs sitting in memory. A direct check
        // on memtable size doesn't work — RocksDB pre-allocates
        // the next active memtable on flush, so
        // `cur-size-active-mem-table` does not snap to zero —
        // but L0 file count rising from 0 to >= 1 is an
        // unambiguous signal the flush hit disk for the CF.
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let cf = db.cf_handle("foo").unwrap();
        let l0 = |cf: &Arc<BoundColumnFamily<'_>>| {
            db.rocksdb()
                .property_int_value_cf(cf, "rocksdb.num-files-at-level0")
                .unwrap()
                .unwrap()
        };
        assert_eq!(l0(&cf), 0, "expected no L0 files before any writes");
        for i in 0..32u64 {
            db.rocksdb()
                .put_cf(&cf, i.to_be_bytes(), i.to_be_bytes())
                .unwrap();
        }
        assert_eq!(
            l0(&cf),
            0,
            "expected writes to sit in the memtable before flush",
        );
        db.flush().unwrap();
        assert!(
            l0(&cf) >= 1,
            "expected at least one L0 SST file after flush, got {}",
            l0(&cf),
        );
    }

    #[test]
    fn compact_range_cf_succeeds_on_known_cf() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let cf = db.cf_handle("foo").unwrap();
        for i in 0..32u64 {
            db.rocksdb()
                .put_cf(&cf, i.to_be_bytes(), i.to_be_bytes())
                .unwrap();
        }
        db.flush().unwrap();
        // Whole-CF compaction (None, None) runs without error; the
        // data survives since this CF has no compaction filter.
        db.compact_range_cf("foo", None, None).unwrap();
        assert_eq!(
            db.rocksdb().get_cf(&cf, 5u64.to_be_bytes()).unwrap(),
            Some(5u64.to_be_bytes().to_vec()),
        );
    }

    #[test]
    fn compact_range_cf_errors_for_unknown_cf() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let err = db
            .compact_range_cf("not_in_schema", None, None)
            .unwrap_err();
        assert!(matches!(err, Error::MissingColumnFamily(_)));
    }

    #[test]
    fn cf_metrics_returns_default_for_unknown_cf() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let metrics = db.cf_metrics("not_in_schema");
        // All sentinel values for an unknown CF.
        assert_eq!(metrics, RocksMetrics::default());
    }

    #[test]
    fn cf_metrics_reports_real_values_for_known_cf() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let metrics = db.cf_metrics("foo");
        // We don't assert specific numbers (they depend on RocksDB
        // internals), but a known CF should yield non-sentinel
        // values for at least the always-available properties:
        // block cache state and memtable sizes.
        assert!(metrics.block_cache_capacity >= 0);
        assert!(metrics.size_all_mem_tables >= 0);
        assert!(metrics.num_immutable_mem_tables >= 0);
        assert!(metrics.is_write_stopped >= 0);
    }

    #[test]
    fn open_propagates_rocksdb_lock_error() {
        let dir = TempDir::new().unwrap();
        let (_db1, _schema1) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let result = Db::open::<TestSchema>(dir.path(), DbOptions::default());
        let err = result.expect_err("second open of the same path should fail");
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn drop_cf_removes_the_cf_at_runtime() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        assert!(db.cf_handle("foo").is_some());
        db.drop_cf("foo").unwrap();
        assert!(db.cf_handle("foo").is_none());
    }

    #[test]
    fn drop_cf_unknown_cf_is_an_error() {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let err = db.drop_cf("not_in_schema").unwrap_err();
        assert!(matches!(err, Error::Rocksdb(_)));
    }

    #[test]
    fn data_persists_across_db_close_and_reopen() {
        // Mirrors alt's test_persistence (minus the framework's
        // watermark concerns). Writes through Batch survive a Db
        // drop and a fresh open at the same path; in-memory
        // snapshots do not (the buffer starts empty after reopen).
        use crate::DbMap;
        use crate::Encode;
        use crate::error::DecodeError;
        use crate::error::EncodeError;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct U64Be(u64);

        impl Encode for U64Be {
            fn encode_into<B: bytes::BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
                buf.put_slice(&self.0.to_be_bytes());
                Ok(())
            }
        }

        impl crate::Decode for U64Be {
            fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
                if buf.remaining() != 8 {
                    return Err(DecodeError::msg("expected 8 bytes"));
                }
                Ok(Self(buf.get_u64()))
            }
        }

        #[derive(Debug)]
        struct PersistSchema {
            items: DbMap<U64Be, U64Be>,
        }

        impl Schema for PersistSchema {
            fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
                vec![CfDescriptor::new("items", opts.options("items"))]
            }

            fn open(db: &Db) -> Result<Self, OpenError> {
                Ok(Self {
                    items: DbMap::new(db.clone(), "items")?,
                })
            }
        }

        let dir = TempDir::new().unwrap();
        {
            let (db, schema) = Db::open::<PersistSchema>(dir.path(), DbOptions::default()).unwrap();
            let mut batch = db.batch();
            batch.put(&schema.items, &U64Be(42), &U64Be(43)).unwrap();
            batch.commit().unwrap();
            // Live-tip read confirms before close.
            assert_eq!(schema.items.get(&U64Be(42)).unwrap(), Some(U64Be(43)));
        }
        // Drop everything, then reopen at the same path.
        let (_db2, schema2) = Db::open::<PersistSchema>(dir.path(), DbOptions::default()).unwrap();
        assert_eq!(schema2.items.get(&U64Be(42)).unwrap(), Some(U64Be(43)));
    }

    #[test]
    fn dbmap_reads_fail_with_missing_cf_after_drop_cf() {
        use crate::DbMap;
        use crate::Encode;
        use crate::error::EncodeError;

        struct Bytes;
        impl Encode for Bytes {
            fn encode_into<B: bytes::BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
                buf.put_slice(b"k");
                Ok(())
            }
        }

        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        let map: DbMap<Bytes, Vec<u8>> = DbMap::new(db.clone(), "foo").unwrap();

        // Dropping the CF underneath an existing handle: subsequent
        // reads surface MissingColumnFamily rather than panicking or
        // succeeding silently.
        db.drop_cf("foo").unwrap();
        let err = map.get(&Bytes).unwrap_err();
        assert!(matches!(err, Error::MissingColumnFamily(_)));
    }

    mod options_toggle {
        //! Tests for [`Db::set_options_cf`],
        //! [`Db::set_restore_options_cf`], and
        //! [`Db::set_tip_options_cf`].

        use bytes::BufMut;
        use tempfile::TempDir;

        use super::*;
        use crate::DbMap;
        use crate::Encode;
        use crate::error::EncodeError;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct U64Be(u64);

        impl Encode for U64Be {
            fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
                buf.put_slice(&self.0.to_be_bytes());
                Ok(())
            }
        }

        impl crate::Decode for U64Be {
            fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, crate::error::DecodeError> {
                if buf.remaining() != 8 {
                    return Err(crate::error::DecodeError::msg("expected 8 bytes"));
                }
                Ok(Self(buf.get_u64()))
            }
        }

        #[derive(Debug)]
        struct ItemsSchema {
            items: DbMap<U64Be, U64Be>,
        }

        impl Schema for ItemsSchema {
            fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
                vec![CfDescriptor::new("items", opts.options("items"))]
            }

            fn open(db: &Db) -> Result<Self, OpenError> {
                Ok(Self {
                    items: DbMap::new(db.clone(), "items")?,
                })
            }
        }

        #[test]
        fn set_options_cf_applies_known_mutable_options() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            // `disable_auto_compactions` is a documented mutable
            // option; setting both true and false must succeed.
            db.set_options_cf("items", &[("disable_auto_compactions", "true")])
                .unwrap();
            db.set_options_cf("items", &[("disable_auto_compactions", "false")])
                .unwrap();
        }

        #[test]
        fn set_options_cf_unknown_key_returns_rocksdb_error() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            let err = db
                .set_options_cf("items", &[("not_a_real_option", "1")])
                .unwrap_err();
            assert!(matches!(err, Error::Rocksdb(_)));
        }

        #[test]
        fn set_options_cf_unknown_cf_returns_missing_column_family() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            let err = db
                .set_options_cf("not_in_schema", &[("disable_auto_compactions", "true")])
                .unwrap_err();
            assert!(matches!(err, Error::MissingColumnFamily(_)));
        }

        #[test]
        fn set_options_cf_empty_slice_is_ok() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            db.set_options_cf("items", &[]).unwrap();
        }

        #[test]
        fn restore_then_tip_toggles_succeed() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            db.set_restore_options_cf("items").unwrap();
            db.set_tip_options_cf("items").unwrap();
        }

        #[test]
        fn restore_options_unknown_cf_errors() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            let err = db.set_restore_options_cf("not_in_schema").unwrap_err();
            assert!(matches!(err, Error::MissingColumnFamily(_)));
        }

        #[test]
        fn tip_options_unknown_cf_errors() {
            let dir = TempDir::new().unwrap();
            let (db, _schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            let err = db.set_tip_options_cf("not_in_schema").unwrap_err();
            assert!(matches!(err, Error::MissingColumnFamily(_)));
        }

        #[test]
        fn writes_proceed_under_restore_options() {
            // Bulk-load shape: many writes into a CF whose
            // compaction has been frozen. The L0 stop trigger is at
            // i32::MAX so no slowdown / stop fires; writes complete
            // without error.
            let dir = TempDir::new().unwrap();
            let (db, schema) = Db::open::<ItemsSchema>(dir.path(), DbOptions::default()).unwrap();
            db.set_restore_options_cf("items").unwrap();

            // Enough writes plus flushes to produce multiple L0
            // files. Under default tip options the L0 stop trigger
            // (36) would not be reached either, but the test still
            // exercises that writes go through the path we've
            // toggled.
            for batch_id in 0..8u64 {
                let mut batch = db.batch();
                for i in 0..256u64 {
                    let k = batch_id * 1024 + i;
                    batch.put(&schema.items, &U64Be(k), &U64Be(k)).unwrap();
                }
                batch.commit().unwrap();
                db.flush().unwrap();
            }

            db.set_tip_options_cf("items").unwrap();

            // Continuing writes under tip options also succeed.
            let mut batch = db.batch();
            batch
                .put(&schema.items, &U64Be(9999), &U64Be(9999))
                .unwrap();
            batch.commit().unwrap();

            assert_eq!(
                schema.items.get(&U64Be(0)).unwrap(),
                Some(U64Be(0)),
                "data written under restore options must still be readable after tip toggle",
            );
            assert_eq!(schema.items.get(&U64Be(9999)).unwrap(), Some(U64Be(9999)),);
        }

        #[test]
        fn restore_toggle_is_per_cf() {
            // Two CFs, restore-mode on one only; the other keeps
            // its tip defaults. Verified by exercising both CFs and
            // confirming both write paths succeed independently.
            #[derive(Debug)]
            struct TwoCfSchema {
                a: DbMap<U64Be, U64Be>,
                b: DbMap<U64Be, U64Be>,
            }

            impl Schema for TwoCfSchema {
                fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
                    vec![
                        CfDescriptor::new("a", opts.options("a")),
                        CfDescriptor::new("b", opts.options("b")),
                    ]
                }

                fn open(db: &Db) -> Result<Self, OpenError> {
                    Ok(Self {
                        a: DbMap::new(db.clone(), "a")?,
                        b: DbMap::new(db.clone(), "b")?,
                    })
                }
            }

            let dir = TempDir::new().unwrap();
            let (db, schema) = Db::open::<TwoCfSchema>(dir.path(), DbOptions::default()).unwrap();
            db.set_restore_options_cf("a").unwrap();

            let mut batch = db.batch();
            batch.put(&schema.a, &U64Be(1), &U64Be(10)).unwrap();
            batch.put(&schema.b, &U64Be(2), &U64Be(20)).unwrap();
            batch.commit().unwrap();

            db.set_tip_options_cf("a").unwrap();
            assert_eq!(schema.a.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
            assert_eq!(schema.b.get(&U64Be(2)).unwrap(), Some(U64Be(20)));
        }
    }
}

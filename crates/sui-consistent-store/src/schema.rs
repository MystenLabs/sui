// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The [`Schema`] trait used to register column families with the
//! database and to construct typed handles into them, plus the
//! [`SchemaAtSnapshot`] companion trait used to re-bind a schema at
//! a captured snapshot.
//!
//! Schemas are hand-written Rust structs whose fields are typed
//! handles into individual column families ([`DbMap<K, V, R>`](crate::DbMap)).
//! The struct is parameterized by a [`Reader`](crate::Reader)
//! (defaulted to [`Db`]) so the same schema body serves both the
//! live tip and snapshot-bound projections.
//!
//! [`Schema`] is implemented for the live variant (`MySchema<Db>`)
//! and owns the database-open sequence for its static set of column
//! families and typed handles.
//!
//! [`SchemaAtSnapshot`] is a separate trait the schema author opts
//! into; it declares a `MySchema<Snapshot>` projection and a
//! one-line constructor that re-binds each field via
//! [`DbMap::at`](crate::DbMap::at).
//!
//! # Examples
//!
//! ```
//! use sui_consistent_store::CfDescriptor;
//! use sui_consistent_store::Db;
//! use sui_consistent_store::DbMap;
//! use sui_consistent_store::DbOptions;
//! use sui_consistent_store::Reader;
//! use sui_consistent_store::Schema;
//! use sui_consistent_store::SchemaAtSnapshot;
//! use sui_consistent_store::Snapshot;
//! use sui_consistent_store::error::OpenError;
//!
//! struct MySchema<R: Reader = Db> {
//!     _reader: std::marker::PhantomData<R>,
//!     _db: Db,
//! }
//!
//! impl Schema for MySchema {
//!     fn open(
//!         path: &std::path::Path,
//!         opts: &sui_consistent_store::CfOptionsResolver,
//!         snapshot_capacity: usize,
//!     ) -> Result<(Db, Self), OpenError> {
//!         let db = Db::open_cfs(
//!             path,
//!             opts,
//!             snapshot_capacity,
//!             vec![CfDescriptor::new("my_cf", opts.options("my_cf"))],
//!         )?;
//!         let schema = Self {
//!             _reader: std::marker::PhantomData,
//!             _db: db.clone(),
//!         };
//!         Ok((db, schema))
//!     }
//! }
//!
//! impl SchemaAtSnapshot for MySchema {
//!     type At = MySchema<Snapshot>;
//!     fn at(&self, _snap: &Snapshot) -> Self::At {
//!         MySchema {
//!             _reader: std::marker::PhantomData,
//!             _db: self._db.clone(),
//!         }
//!     }
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let (_db, _schema) = Db::open::<MySchema>(dir.path(), DbOptions::default()).unwrap();
//! ```

use std::path::Path;

use crate::db::Db;
use crate::error::OpenError;
use crate::options::CfOptionsResolver;
use crate::snapshot::Snapshot;

/// Opens a database with its column-family layout and constructs the
/// typed handle struct against that database at the live tip.
///
/// Implementations are typically hand-written structs parameterized
/// by a [`Reader`](crate::Reader) (defaulted to [`Db`]) whose fields
/// are typed column-family handles. The trait itself is implemented
/// only for the [`Db`]-bound variant of the schema; snapshot-bound
/// variants are constructed by re-binding, not by re-opening.
///
/// [`Schema::open`] defines the schema's column families, opens them
/// through [`Db::open_cfs`], and constructs the typed handles. A
/// schema can create private state before opening the database when
/// its RocksDB callbacks and typed handles must share that state.
pub trait Schema: Sized {
    /// Open the schema's column families and construct the typed
    /// handle struct.
    ///
    /// Build one [`CfDescriptor`] for each schema-owned column family.
    /// Each descriptor pairs a compile-time `&'static str` name with
    /// the [`rocksdb::Options`] applied when the database is opened.
    /// Start with `opts.options(name)`, which applies the configured
    /// performance settings, then attach correctness-bearing merge
    /// operators and compaction filters before calling
    /// [`Db::open_cfs`].
    ///
    /// Pass only schema-owned column families to [`Db::open_cfs`].
    /// RocksDB opens its mandatory `default` column family, and the
    /// framework registers `__restore`, `__watermark`, and `__chain_id`.
    ///
    /// Implementations must forward `snapshot_capacity` unchanged to
    /// [`Db::open_cfs`] so the framework's bookkeeping column families,
    /// configuration validation, and snapshot retention are applied
    /// consistently. The returned schema must be bound to the returned
    /// database.
    fn open(
        path: &Path,
        opts: &CfOptionsResolver,
        snapshot_capacity: usize,
    ) -> Result<(Db, Self), OpenError>;
}

/// Describes one column family in a [`Schema`].
///
/// Construct via [`CfDescriptor::new`].
///
/// # Examples
///
/// ```
/// use sui_consistent_store::CfDescriptor;
///
/// let opts = rocksdb::Options::default();
/// let owners = CfDescriptor::new("owners", opts);
/// ```
pub struct CfDescriptor {
    /// Column-family name.
    pub name: &'static str,
    /// Per-CF RocksDB options applied at create time.
    pub options: rocksdb::Options,
}

impl std::fmt::Debug for CfDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `rocksdb::Options` does not implement Debug, so summarize.
        f.debug_struct("CfDescriptor")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl CfDescriptor {
    /// Construct a descriptor.
    pub fn new(name: &'static str, options: rocksdb::Options) -> Self {
        Self { name, options }
    }
}

/// Re-binds a [`Schema`] at a captured [`Snapshot`].
///
/// The schema author declares the projection's body type as `At`
/// and writes a one-line constructor that re-binds each field via
/// [`DbMap::at`](crate::DbMap::at). The trait is independent of
/// [`Schema`] so authors who never need snapshot-bound reads can
/// skip the impl entirely.
///
/// # Cost
///
/// Each call to [`at`](Self::at) constructs a fresh schema struct
/// containing a [`DbMap<_, _, Snapshot>`](crate::DbMap) per field.
/// Each per-field re-bind clones the column-family name (a
/// `Box<str>` allocation) and clones the [`Snapshot`] (two `Arc`
/// bumps). For an N-CF schema this is N allocations and 2N `Arc`
/// bumps per re-bind. Re-bind once per request handler and read
/// many times against the same projection.
pub trait SchemaAtSnapshot {
    /// The projected schema body — typically `MySchema<Snapshot>`
    /// when the schema is parameterized by a [`Reader`](crate::Reader).
    ///
    /// Because [`Snapshot`] is an owned, lifetime-free reader, the
    /// projection is self-contained: it can be stored in a struct
    /// or moved into a spawned task without dragging a borrow on
    /// the originating [`Snapshot`] value.
    type At;

    /// Re-bind this schema at `snap`.
    ///
    /// The returned projection's reads see the database state
    /// captured by the snapshot, regardless of writes that occur
    /// after [`Db::take_snapshot`](crate::Db::take_snapshot) was
    /// called. The projection owns clones of `snap` (one per field)
    /// and is independent of `self` after construction.
    fn at(&self, snap: &Snapshot) -> Self::At;
}

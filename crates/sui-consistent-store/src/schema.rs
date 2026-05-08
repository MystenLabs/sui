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
//! and pairs the static set of column families a schema requires
//! ([`Schema::cfs`]) with the constructor that builds the schema
//! struct from an opened database ([`Schema::open`]).
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
//!     fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
//!         vec![CfDescriptor::new("my_cf", base_options.clone())]
//!     }
//!
//!     fn open(db: &Db) -> Result<Self, OpenError> {
//!         Ok(Self {
//!             _reader: std::marker::PhantomData,
//!             _db: db.clone(),
//!         })
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

use crate::db::Db;
use crate::error::OpenError;
use crate::snapshot::Snapshot;

/// Declares the column families a database needs and constructs the
/// typed handle struct against an opened database at the live tip.
///
/// Implementations are typically hand-written structs parameterized
/// by a [`Reader`](crate::Reader) (defaulted to [`Db`]) whose fields
/// are typed column-family handles. The trait itself is implemented
/// only for the [`Db`]-bound variant of the schema; snapshot-bound
/// variants are constructed by re-binding, not by re-opening.
///
/// The trait has two responsibilities:
///
/// - [`cfs`](Self::cfs) returns the column families this schema
///   requires, with per-CF [`rocksdb::Options`]. It is called once at
///   open time, with the database-level base options as input. The
///   order of entries does not matter.
/// - [`open`](Self::open) constructs the schema struct against an
///   already-opened database. Each column family named by `cfs()` is
///   guaranteed to exist on the database before this is called.
pub trait Schema: Sized {
    /// The column families this schema requires.
    ///
    /// Each entry is a [`CfDescriptor`] carrying a column-family
    /// name (a `&'static str` so the schema's CF set is fixed at
    /// compile time) and its [`rocksdb::Options`] applied at create
    /// time. `base_options` is supplied by [`Db::open`] and is the
    /// database-level options configured on
    /// [`DbOptions::db_options`](crate::DbOptions::db_options);
    /// implementations typically clone it as the starting point for
    /// each CF and layer per-CF tweaks (merge operators, compaction
    /// filters, custom block sizes) on top.
    ///
    /// The default column family (`"default"`) is registered
    /// automatically by [`Db::open`] and need not be included here,
    /// though including it is harmless.
    fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor>;

    /// Construct the schema struct against `db`.
    ///
    /// Implementations typically clone the supplied [`Db`] handle
    /// into each column-family handle they construct (a `Db` clone
    /// is one `Arc` bump). The default implementation in user
    /// schemas is usually a one-line `Self::new(db.clone())` that
    /// delegates to inherent methods on the schema struct.
    fn open(db: &Db) -> Result<Self, OpenError>;
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

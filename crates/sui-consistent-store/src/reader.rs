// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The [`Reader`] trait abstracting the read context a
//! [`DbMap`](crate::DbMap) is bound to.
//!
//! The crate ships exactly two reader types: [`Db`] (live-tip) and
//! [`Snapshot`](crate::Snapshot) (snapshot-bound), each with a
//! blanket impl on its `&` form for the borrowed variant. There is
//! no separate `Live`/`LiveRef` wrapper — a [`Db`] handle *is* the
//! live-tip read context, and a borrow of one *is* the borrowed
//! live-tip context.
//!
//! Every [`DbMap<K, V, R>`](crate::DbMap) is parameterized by a
//! [`Reader`]. The default is [`Db`], so today's call sites
//! (`DbMap::new(db, "items")`, `Db::open::<MySchema>(...)`) keep
//! working unchanged. Re-projecting a schema or a single map at a
//! captured snapshot — via
//! [`SchemaAtSnapshot::at`](crate::SchemaAtSnapshot::at) or
//! [`DbMap::at`](crate::DbMap::at) — produces a parallel handle whose
//! reader is [`Snapshot`](crate::Snapshot).
//!
//! # Why this exists
//!
//! The crate previously routed snapshot reads through an entirely
//! separate type (the snapshot handle's read methods, plus a
//! borrowed `SnapshotView`). That kept the read API duplicated and
//! forced call sites to choose between `map.get(&k)?` (live) and
//! `snap.get(&map, &k)?` (snapshot). With the reader generic, both
//! call sites read identically: the choice of consistency context is
//! made once, at the point the schema or map is bound, and from
//! there every method call is `map.get(&k)?` against whatever reader
//! is in scope.
//!
//! # Cost model
//!
//! Constructing a [`DbMap`](crate::DbMap) bound to a [`Db`] costs
//! one `Arc` bump (the [`Db`] clone). Constructing one bound to
//! [`Snapshot`](crate::Snapshot) costs two (the snapshot's own
//! [`Db`] and `Arc<SnapshotEntry>`). Each call to
//! [`DbMap::at`](crate::DbMap::at) clones the snapshot (two `Arc`
//! bumps); the column-family name is a [`&'static str`](prim@str),
//! so it copies without allocating. Re-projecting an N-CF schema
//! costs 2N `Arc` bumps and N struct constructions per call to
//! [`SchemaAtSnapshot::at`](crate::SchemaAtSnapshot::at). For a
//! per-request handler that projects once and reads many times,
//! this is amortized; for a hot path that projects on every read,
//! project once outside the loop.
//!
//! `&Db` and `&Snapshot` are the zero-`Arc`-bump variants. Use them
//! via `DbMap::new(&db, "cf")` or
//! [`DbMap::at_ref`](crate::DbMap::at_ref) when the returned
//! [`DbMap`](crate::DbMap) is scoped to a single function body and
//! can be tied to a [`Db`] or [`Snapshot`](crate::Snapshot) the
//! caller already holds.

use rocksdb::ReadOptions;

use crate::db::Db;

/// Abstracts the read context a [`DbMap`](crate::DbMap) is bound to.
///
/// Every implementation supplies (1) the [`Db`] handle needed to
/// look up the column-family handle and (2) a fresh [`ReadOptions`]
/// tuned for the reader's consistency context. [`Db`] (and its
/// `&Db` blanket impl) returns [`ReadOptions::default()`];
/// [`Snapshot`](crate::Snapshot) (and its `&Snapshot` blanket impl)
/// returns one with [`set_snapshot`](ReadOptions::set_snapshot)
/// pointed at the captured snapshot.
///
/// # Sealed
///
/// The crate ships two reader types — [`Db`] and
/// [`Snapshot`](crate::Snapshot) — each with a blanket impl on its
/// `&` form. The trait is sealed via a pub(crate) supertrait so
/// downstream code cannot add another — a custom reader could
/// return [`ReadOptions`] referencing a snapshot pointer not
/// co-owned through the [`Db`] handle story, leading to UB inside
/// RocksDB.
pub trait Reader: sealed::Sealed {
    /// The shared database handle the column family lives on.
    fn db(&self) -> &Db;

    /// Construct a fresh [`ReadOptions`] configured for this reader.
    ///
    /// Implementations are expected to be cheap; the returned options
    /// are consumed by a single read and dropped. Callers that issue
    /// many reads in a tight loop pay one fresh allocation per call,
    /// which matches RocksDB's expected pattern.
    fn read_options(&self) -> ReadOptions;
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for Db {}

impl Reader for Db {
    fn db(&self) -> &Db {
        self
    }

    fn read_options(&self) -> ReadOptions {
        ReadOptions::default()
    }
}

impl sealed::Sealed for &Db {}

impl Reader for &Db {
    fn db(&self) -> &Db {
        self
    }

    fn read_options(&self) -> ReadOptions {
        ReadOptions::default()
    }
}

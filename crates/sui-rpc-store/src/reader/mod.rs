// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adapter that exposes [`RpcStoreSchema`] through the trait stack
//! `sui-rpc-api` consumes — [`ObjectStore`], [`ReadStore`],
//! [`ChildObjectResolver`], [`RpcStateReader`], and [`RpcIndexes`].
//!
//! The adapter type, [`RpcStoreReader`], is generic over a
//! [`Reader`] so a single struct serves both tip reads (`R = Db`)
//! and point-in-time reads bound to a snapshot (`R = Snapshot`).
//! Callers requesting "give me the latest" hold the tip reader;
//! callers requesting "show me state at checkpoint X" hold the
//! snapshot-bound one. The choice of consistency context is made
//! once, at the point [`RpcStoreReader::at_snapshot`] is called.
//!
//! [`ObjectStore`]: sui_types::storage::ObjectStore
//! [`ReadStore`]: sui_types::storage::ReadStore
//! [`ChildObjectResolver`]: sui_types::storage::ChildObjectResolver
//! [`RpcStateReader`]: sui_types::storage::RpcStateReader
//! [`RpcIndexes`]: sui_types::storage::RpcIndexes
//! [`Reader`]: sui_consistent_store::reader::Reader

mod child_resolver;
mod indexes;
mod layout;
mod object_store;
mod read_store;
mod state_reader;

use std::sync::Arc;

use sui_consistent_store::Db;
use sui_consistent_store::SchemaAtSnapshot;
use sui_consistent_store::Snapshot;
use sui_consistent_store::reader::Reader;

use crate::RpcStoreSchema;

/// Adapter exposing [`RpcStoreSchema`] through the
/// `sui-rpc-api` reader-trait stack.
///
/// Construct one of two ways:
///
/// - [`RpcStoreReader::new`] binds to tip reads (`R = Db`). Use
///   this for callers that want the latest committed state.
/// - [`RpcStoreReader::at_snapshot`] takes a captured
///   [`Snapshot`] and returns a [`RpcStoreReader<Snapshot>`] whose
///   every read returns the state at that snapshot. Use this for
///   "show me state at checkpoint X" requests.
///
/// `RpcStoreReader` holds an `Arc<RpcStoreSchema<R>>` so trait
/// impls can hand a `&self` to any of the inherent read helpers
/// already defined on the schema. The wrapper itself is `Clone`
/// (cheap, `Arc`-backed) so it can be handed to
/// `sui-rpc-api::StateReader::new(Arc::new(reader))`.
pub struct RpcStoreReader<R: Reader = Db> {
    /// The `Db` handle. Held separately from `schema` so trait
    /// impls that need framework-level access (chain id, pipeline
    /// watermarks) don't have to walk through a typed CF.
    db: Db,

    /// Typed handles to every CF the read paths exercise.
    schema: Arc<RpcStoreSchema<R>>,
}

impl<R: Reader> RpcStoreReader<R> {
    /// Bind the adapter to an existing [`RpcStoreSchema`].
    ///
    /// `db` must be the same [`Db`] the schema was opened against.
    /// Holding both separately is cheap (each is `Arc`-backed) and
    /// avoids a `schema.epochs.reader().db()` style detour inside
    /// hot read paths.
    pub fn new(db: Db, schema: Arc<RpcStoreSchema<R>>) -> Self {
        Self { db, schema }
    }

    /// Borrow the underlying [`Db`] handle. Used by trait impls
    /// that read directly from the framework CFs (chain id,
    /// pipeline watermarks) rather than going through the typed
    /// user schema.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Borrow the typed schema this adapter is bound to. Trait
    /// impls reach through this to call any of the inherent read
    /// helpers `RpcStoreSchema` exposes per-CF.
    pub fn schema(&self) -> &RpcStoreSchema<R> {
        &self.schema
    }
}

impl RpcStoreReader<Db> {
    /// Re-project this reader against a captured [`Snapshot`].
    ///
    /// The returned [`RpcStoreReader<Snapshot>`] reads every CF
    /// through the snapshot's `ReadOptions`, so reads are
    /// consistent with the point in time at which the snapshot
    /// was taken (rather than the tip). The original tip reader
    /// is unaffected.
    pub fn at_snapshot(&self, snap: &Snapshot) -> RpcStoreReader<Snapshot> {
        RpcStoreReader {
            db: self.db.clone(),
            schema: Arc::new(self.schema.at(snap)),
        }
    }
}

impl<R: Reader> Clone for RpcStoreReader<R> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            schema: self.schema.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::DbOptions;

    use super::*;

    #[test]
    fn new_binds_db_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        // Smoke check: both handles are reachable and cloneable.
        let _ = reader.clone();
        assert!(reader.schema().get_pruning_watermarks().unwrap().is_none());
    }

    #[test]
    fn at_snapshot_returns_snapshot_bound_reader() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));

        db.take_snapshot(sui_consistent_store::Watermark::for_checkpoint(0));
        let snap = db.at_snapshot(0).expect("snapshot retained");
        let snap_reader = reader.at_snapshot(&snap);
        // Both readers see the same (empty) state on a fresh DB.
        assert!(reader.schema().get_pruning_watermarks().unwrap().is_none());
        assert!(
            snap_reader
                .schema()
                .get_pruning_watermarks()
                .unwrap()
                .is_none()
        );
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adapter that exposes [`RpcStoreSchema`] through the trait stack
//! `sui-rpc-api` consumes — [`ObjectStore`], [`ReadStore`],
//! [`RuntimeObjectResolver`], [`RpcStateReader`], and [`RpcIndexes`].
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
//! [`RuntimeObjectResolver`]: sui_types::storage::RuntimeObjectResolver
//! [`RpcStateReader`]: sui_types::storage::RpcStateReader
//! [`RpcIndexes`]: sui_types::storage::RpcIndexes
//! [`Reader`]: sui_consistent_store::reader::Reader

mod child_resolver;
mod indexes;
#[cfg(test)]
mod integration_test;
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

    /// The pipelines registered on this deployment, whose watermarks
    /// bound the reported indexed tip
    /// (`get_highest_indexed_checkpoint_seq_number`). Kept explicit
    /// rather than derived from the watermark CF's rows so a stale
    /// row left behind by a pipeline that is no longer registered
    /// cannot pin the reported tip, and so a registered pipeline
    /// that has not committed yet correctly reads as "nothing fully
    /// indexed".
    pipelines: Arc<[&'static str]>,
}

impl<R: Reader> RpcStoreReader<R> {
    /// Bind the adapter to an existing [`RpcStoreSchema`].
    ///
    /// `db` must be the same [`Db`] the schema was opened against.
    /// Holding both separately is cheap (each is `Arc`-backed) and
    /// avoids a `schema.epochs.reader().db()` style detour inside
    /// hot read paths.
    ///
    /// The registered pipeline set defaults to the embedded
    /// fullnode's cohorts ([`LIVE_COHORT`] + [`HISTORY_COHORT`]) —
    /// the only in-tree deployment. A deployment that registers a
    /// different set (e.g. a standalone node running the raw
    /// chain-data pipelines) must override it via
    /// [`Self::with_pipelines`] so the indexed-tip bound covers
    /// exactly what it serves.
    ///
    /// [`LIVE_COHORT`]: crate::LIVE_COHORT
    /// [`HISTORY_COHORT`]: crate::HISTORY_COHORT
    pub fn new(db: Db, schema: Arc<RpcStoreSchema<R>>) -> Self {
        let pipelines = crate::LIVE_COHORT
            .iter()
            .chain(crate::HISTORY_COHORT)
            .copied()
            .collect();
        Self {
            db,
            schema,
            pipelines,
        }
    }

    /// Override the registered pipeline set whose watermarks bound
    /// the reported indexed tip. See [`Self::new`] for the default.
    pub fn with_pipelines(mut self, pipelines: impl IntoIterator<Item = &'static str>) -> Self {
        self.pipelines = pipelines.into_iter().collect();
        self
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

    /// The highest checkpoint the live-object cohort (owned objects, types,
    /// balances) has committed -- `min(checkpoint_hi_inclusive)` across its
    /// pipelines, or `None` if any has no watermark yet.
    ///
    /// The embedded indexer follows the tip asynchronously, so this lags the
    /// executed tip and bounds the checkpoint at which the live-object index
    /// surface is readable. The history cohort backfills independently and is
    /// deliberately excluded here -- its availability is exposed separately.
    pub fn highest_live_committed_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<
        Option<sui_types::messages_checkpoint::CheckpointSequenceNumber>,
    > {
        self.min_committed(crate::LIVE_COHORT.iter().copied())
    }

    /// The highest checkpoint every pipeline in `pipelines` has
    /// committed (`min(checkpoint_hi_inclusive)`), or `None` if any of
    /// them has no watermark yet. Private, but reachable from the
    /// sibling trait-impl modules (children of this module).
    fn min_committed(
        &self,
        pipelines: impl IntoIterator<Item = &'static str>,
    ) -> sui_types::storage::error::Result<
        Option<sui_types::messages_checkpoint::CheckpointSequenceNumber>,
    > {
        let framework = self.db().framework();
        let mut min_hi: Option<u64> = None;
        for name in pipelines {
            let key = sui_consistent_store::PipelineTaskKey::new(name);
            let Some(watermark) = framework
                .watermarks
                .get(&key)
                .map_err(sui_types::storage::error::Error::custom)?
            else {
                return Ok(None);
            };
            let hi = watermark.checkpoint_hi_inclusive;
            min_hi = Some(min_hi.map_or(hi, |m| m.min(hi)));
        }
        Ok(min_hi)
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
            pipelines: self.pipelines.clone(),
        }
    }
}

impl<R: Reader> Clone for RpcStoreReader<R> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            schema: self.schema.clone(),
            pipelines: self.pipelines.clone(),
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

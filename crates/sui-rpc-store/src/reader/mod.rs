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
#[cfg(test)]
mod integration_test;
mod layout;
mod object_store;
mod read_store;
mod state_reader;

use std::sync::Arc;

use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::SchemaAtSnapshot;
use sui_consistent_store::Snapshot;
use sui_consistent_store::reader::Reader;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Kind as StorageErrorKind;
use sui_types::storage::error::Result as StorageResult;
use tracing::debug;
use tracing::error;

use crate::RpcStoreSchema;
use crate::config::AvailabilityConfig;
use crate::config::PipelineAvailability;

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

    /// Per-pipeline serving policies. Trivial by default (every
    /// read served); see [`Self::with_availability`].
    availability: Arc<AvailabilityConfig>,
}

impl<R: Reader> RpcStoreReader<R> {
    /// Bind the adapter to an existing [`RpcStoreSchema`].
    ///
    /// `db` must be the same [`Db`] the schema was opened against.
    /// Holding both separately is cheap (each is `Arc`-backed) and
    /// avoids a `schema.epochs.reader().db()` style detour inside
    /// hot read paths.
    ///
    /// The reader starts with a trivial availability policy (every
    /// read served) — internal bootstrap readers (e.g. the one
    /// `seed_current_epoch_start` builds during restore) rely on
    /// this. Serving deployments opt in via
    /// [`Self::with_availability`].
    pub fn new(db: Db, schema: Arc<RpcStoreSchema<R>>) -> Self {
        Self {
            db,
            schema,
            availability: Arc::new(AvailabilityConfig::default()),
        }
    }

    /// Apply per-pipeline serving policies (see
    /// [`PipelineAvailability`]): reads that need a gated pipeline
    /// fail with an unavailable error, and gated pipelines are
    /// excluded from the cross-pipeline watermark bounds.
    pub fn with_availability(mut self, availability: AvailabilityConfig) -> Self {
        self.availability = Arc::new(availability);
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
    ///
    /// Pipelines gated by the availability policy are excluded from the MIN
    /// so they stop pinning the reported tip (their reads fail as unavailable
    /// instead), and a gated pipeline with no watermark no longer forces
    /// `None`. `Ok(None)` — including the every-live-pipeline-gated case — is
    /// the "no available index data" signal consumers already handle.
    pub fn highest_live_committed_checkpoint(
        &self,
    ) -> StorageResult<Option<sui_types::messages_checkpoint::CheckpointSequenceNumber>> {
        let tip = if self.availability.is_trivial() {
            None
        } else {
            self.watermark_tip()?
        };

        let framework = self.db().framework();
        let mut min_hi: Option<u64> = None;
        for name in crate::LIVE_COHORT {
            if !self.pipeline_available(name, tip)? {
                continue;
            }
            let key = PipelineTaskKey::new(*name);
            let Some(watermark) = framework
                .watermarks
                .get(&key)
                .map_err(StorageError::custom)?
            else {
                return Ok(None);
            };
            let hi = watermark.checkpoint_hi_inclusive;
            min_hi = Some(min_hi.map_or(hi, |m| m.min(hi)));
        }
        Ok(min_hi)
    }

    /// Tip approximation for lag policies: the highest committed watermark
    /// across all registered pipelines, or `None` on a store with no
    /// watermarks yet. Equal to the node's executed tip when a tip-following
    /// pipeline is registered, and the fastest pipeline otherwise.
    fn watermark_tip(&self) -> StorageResult<Option<u64>> {
        // The owned `FrameworkSchema` over `Db`: iteration always reads the
        // tip axis, even for snapshot-bound readers.
        let framework = FrameworkSchema::new(self.db().clone());
        let mut max: Option<u64> = None;
        for entry in framework
            .watermarks
            .iter(..)
            .map_err(StorageError::custom)?
        {
            let (_, watermark) = entry.map_err(StorageError::custom)?;
            let hi = watermark.checkpoint_hi_inclusive;
            max = Some(max.map_or(hi, |m| m.max(hi)));
        }
        Ok(max)
    }

    /// The committed watermark for one pipeline, `None` if it has none yet.
    fn committed_checkpoint(&self, pipeline: &str) -> StorageResult<Option<u64>> {
        let framework = self.db().framework();
        Ok(framework
            .watermarks
            .get(&PipelineTaskKey::new(pipeline))
            .map_err(StorageError::custom)?
            .map(|w| w.checkpoint_hi_inclusive))
    }

    /// Whether `pipeline` may be served, given the memoized watermark `tip`
    /// (`None` = no watermarks in the store).
    fn pipeline_available(&self, pipeline: &str, tip: Option<u64>) -> StorageResult<bool> {
        let Some(policy) = self.availability.policy_for(pipeline) else {
            return Ok(true);
        };
        let committed = match policy {
            PipelineAvailability::MaxCheckpointLag(_) => self.committed_checkpoint(pipeline)?,
            _ => None,
        };
        Ok(policy.is_available(committed, tip.unwrap_or(0)))
    }

    /// `Err(Kind::Unavailable)` unless every named pipeline may be served
    /// under the configured availability policy. Zero-cost when no policy is
    /// configured; the tip is computed only when a named pipeline has a lag
    /// policy.
    pub(crate) fn require_pipelines(&self, pipelines: &[&str]) -> StorageResult<()> {
        if self.availability.is_trivial() {
            return Ok(());
        }

        let mut memo_tip: Option<Option<u64>> = None;
        for name in pipelines {
            match self.availability.policy_for(name) {
                None | Some(PipelineAvailability::Enabled) => {}
                Some(PipelineAvailability::Disabled) => {
                    return Err(StorageError::unavailable(format!(
                        "pipeline {name} is disabled by config"
                    )));
                }
                Some(policy @ PipelineAvailability::MaxCheckpointLag(lag)) => {
                    let tip = match memo_tip {
                        Some(tip) => tip,
                        None => *memo_tip.insert(self.watermark_tip()?),
                    }
                    .unwrap_or(0);
                    let committed = self.committed_checkpoint(name)?;
                    if !policy.is_available(committed, tip) {
                        return Err(StorageError::unavailable(match committed {
                            Some(committed) => format!(
                                "pipeline {name} is {} checkpoints behind the tip \
                                 ({committed} < {tip}, max-checkpoint-lag {lag})",
                                tip.saturating_sub(committed),
                            ),
                            None => format!(
                                "pipeline {name} has no committed watermark yet \
                                 (max-checkpoint-lag {lag})"
                            ),
                        }));
                    }
                }
            }
        }
        Ok(())
    }

    /// Gate for `Option`-returning trait methods, which cannot carry an
    /// error: `false` means the caller must return `None` rather than serve
    /// data from a gated pipeline. Storage errors while evaluating the
    /// policy also gate (and are logged), matching this module's
    /// error-suppression convention for `Option` reads.
    pub(crate) fn pipelines_available(&self, pipelines: &[&str]) -> bool {
        match self.require_pipelines(pipelines) {
            Ok(()) => true,
            Err(e) if e.kind() == StorageErrorKind::Unavailable => {
                debug!("withholding read: {e}");
                false
            }
            Err(e) => {
                error!("failed to evaluate availability policy for {pipelines:?}: {e:?}");
                false
            }
        }
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
            availability: self.availability.clone(),
        }
    }
}

impl<R: Reader> Clone for RpcStoreReader<R> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            schema: self.schema.clone(),
            availability: self.availability.clone(),
        }
    }
}

/// Test helper: write a committed watermark row for `pipeline`, the
/// way the framework does after a batch commits.
#[cfg(test)]
pub(crate) fn seed_watermark(db: &Db, pipeline: &str, checkpoint_hi: u64) {
    let framework = FrameworkSchema::new(db.clone());
    let mut batch = db.batch();
    batch
        .put(
            &framework.watermarks,
            &PipelineTaskKey::new(pipeline),
            &sui_consistent_store::Watermark::for_checkpoint(checkpoint_hi),
        )
        .unwrap();
    batch.commit().unwrap();
}

/// Test helper: an [`AvailabilityConfig`] from a default policy and
/// per-pipeline overrides.
#[cfg(test)]
pub(crate) fn availability(
    default: Option<PipelineAvailability>,
    overrides: &[(&str, PipelineAvailability)],
) -> AvailabilityConfig {
    AvailabilityConfig {
        default,
        pipelines: overrides
            .iter()
            .map(|(name, policy)| (name.to_string(), *policy))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::DbOptions;

    use super::*;

    fn live_reader(watermarks: &[(&str, u64)]) -> (tempfile::TempDir, RpcStoreReader) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        for (name, hi) in watermarks {
            seed_watermark(&db, name, *hi);
        }
        (dir, RpcStoreReader::new(db, Arc::new(schema)))
    }

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

    #[test]
    fn live_min_is_min_across_cohort_without_policies() {
        let (_dir, reader) = live_reader(&[
            ("object_by_owner", 100),
            ("object_by_type", 90),
            ("balance", 50),
        ]);
        assert_eq!(
            reader.highest_live_committed_checkpoint().unwrap(),
            Some(50)
        );
    }

    #[test]
    fn live_min_excludes_disabled_pipeline() {
        let (_dir, reader) = live_reader(&[
            ("object_by_owner", 100),
            ("object_by_type", 90),
            ("balance", 50),
        ]);
        let gated = reader.with_availability(availability(
            None,
            &[("balance", PipelineAvailability::Disabled)],
        ));
        assert_eq!(gated.highest_live_committed_checkpoint().unwrap(), Some(90));
    }

    #[test]
    fn live_min_excludes_lag_gated_pipeline() {
        // The tip is the highest watermark across all rows (100); `balance`
        // lags it by 50 checkpoints.
        let (_dir, reader) = live_reader(&[
            ("object_by_owner", 100),
            ("object_by_type", 90),
            ("balance", 50),
        ]);

        // A budget of 50 keeps it (lag boundary inclusive).
        let within = reader.clone().with_availability(availability(
            Some(PipelineAvailability::MaxCheckpointLag(50)),
            &[],
        ));
        assert_eq!(
            within.highest_live_committed_checkpoint().unwrap(),
            Some(50)
        );

        // A budget of 49 gates it out, advancing the bound.
        let beyond = reader.with_availability(availability(
            Some(PipelineAvailability::MaxCheckpointLag(49)),
            &[],
        ));
        assert_eq!(
            beyond.highest_live_committed_checkpoint().unwrap(),
            Some(90)
        );
    }

    #[test]
    fn live_min_none_when_all_live_gated() {
        let (_dir, reader) = live_reader(&[
            ("object_by_owner", 100),
            ("object_by_type", 90),
            ("balance", 50),
        ]);
        let gated =
            reader.with_availability(availability(Some(PipelineAvailability::Disabled), &[]));
        assert_eq!(gated.highest_live_committed_checkpoint().unwrap(), None);
    }

    #[test]
    fn disabled_live_pipeline_without_watermark_no_longer_forces_none() {
        // `balance` has no watermark: ungated, the live bound is `None`;
        // disabling it excludes it before the watermark read.
        let (_dir, reader) = live_reader(&[("object_by_owner", 100), ("object_by_type", 90)]);
        assert_eq!(reader.highest_live_committed_checkpoint().unwrap(), None);

        let gated = reader.with_availability(availability(
            None,
            &[("balance", PipelineAvailability::Disabled)],
        ));
        assert_eq!(gated.highest_live_committed_checkpoint().unwrap(), Some(90));
    }

    #[test]
    fn require_pipelines_reports_unavailable_kind() {
        let (_dir, reader) = live_reader(&[("object_by_owner", 100), ("balance", 50)]);
        let reader = reader.with_availability(availability(
            Some(PipelineAvailability::MaxCheckpointLag(10)),
            &[
                ("balance", PipelineAvailability::Disabled),
                ("object_by_type", PipelineAvailability::Enabled),
            ],
        ));

        // Disabled override.
        let err = reader.require_pipelines(&["balance"]).unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::Unavailable);
        assert!(format!("{err:?}").contains("disabled by config"), "{err:?}");

        // Lag default: `object_by_owner` is the tip (distance zero).
        reader.require_pipelines(&["object_by_owner"]).unwrap();

        // Lag default: `epochs` has no watermark at all.
        let err = reader.require_pipelines(&["epochs"]).unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::Unavailable);
        assert!(
            format!("{err:?}").contains("no committed watermark"),
            "{err:?}",
        );

        // Enabled override exempts from the gating default.
        reader.require_pipelines(&["object_by_type"]).unwrap();

        // A pipeline with no policy at all... falls back to the default here.
        let err = reader.require_pipelines(&["transactions"]).unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::Unavailable);
    }

    #[test]
    fn at_snapshot_inherits_availability() {
        let (_dir, reader) = live_reader(&[
            ("object_by_owner", 100),
            ("object_by_type", 90),
            ("balance", 50),
        ]);
        let reader = reader.with_availability(availability(
            None,
            &[("balance", PipelineAvailability::Disabled)],
        ));

        reader
            .db()
            .take_snapshot(sui_consistent_store::Watermark::for_checkpoint(0));
        let snap = reader.db().at_snapshot(0).expect("snapshot retained");
        let snap_reader = reader.at_snapshot(&snap);

        let err = snap_reader.require_pipelines(&["balance"]).unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::Unavailable);
        assert_eq!(
            snap_reader.highest_live_committed_checkpoint().unwrap(),
            Some(90)
        );
    }
}

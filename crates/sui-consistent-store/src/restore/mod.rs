// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bulk-load pipeline shape and driver.
//!
//! `sui-consistent-store` separates bulk-load (one-shot, from a
//! [`RestoreSource`]) from tip indexing (continuous, framework-driven).
//! Each `Restore` pipeline is bounded `: Processor` so that one
//! pipeline shares one identity (`Processor::NAME`) and one
//! configuration type across both phases, but the tip impl is
//! independent — `Restore` does not touch tip data.
//!
//! # Method roles
//!
//! - [`Restore::restore`] takes a single live object and stages
//!   writes derived from it onto the shared per-shard [`Batch`].
//!   The driver parallelises across pipelines and partitions but
//!   commits each (pipeline, partition) atomically.
//!
//! Per-object writes that target merge-CFs combine via the
//! registered merge operator across shards (so cross-shard sums,
//! counters, set unions all converge correctly). Writes to put-CFs
//! have last-write-wins semantics; sources that need a different
//! reconciliation strategy across shards should partition their
//! input so that conflicting writes never hit the same key.
//!
//! # Why `anyhow::Error`
//!
//! Matches the indexer-alt framework's `Processor`/`Handler`
//! signatures, so a single pipeline can return one error type from
//! both its restore and tip methods. Pipeline authors who only
//! call [`Batch`] methods (which return [`crate::error::Error`])
//! get free conversion via `?`.

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::object::Object;

use crate::Batch;

pub mod driver;

#[cfg(test)]
pub(crate) mod test_pipeline;

pub use crate::restore::driver::RestoreDriver;
pub use crate::restore::driver::RestoreDriverConfig;
pub use crate::restore::driver::RestoreSource;

/// A pipeline that can be bulk-loaded from a stream of live
/// objects.
///
/// Implementations are typically unit structs whose trait methods
/// are essentially functions; `&self` is supplied for symmetry with
/// [`Processor::process`], so per-instance
/// configuration (a config struct loaded from CLI, a logger handle)
/// can live on the implementing type and be shared by both the
/// processor and restore paths.
pub trait Restore: Processor {
    /// The portion of the schema this pipeline writes to during
    /// restore. Typically a struct of [`DbMap`](crate::DbMap)
    /// fields scoped to the CFs this pipeline owns. The driver
    /// supplies an `&Self::Schema` on every [`restore`](Self::restore)
    /// call.
    type Schema: Send + Sync;

    /// Stage writes derived from `object` onto `batch`.
    ///
    /// Called from worker tasks under a restore driver. Each
    /// worker owns its own `batch`; objects may arrive in any
    /// order within a worker's slice. The pipeline encodes each
    /// object's resulting rows directly via the [`Batch`] API
    /// (typed `put`, `delete`, `merge` against
    /// [`DbMap`](crate::DbMap) handles in `schema`).
    ///
    /// Cross-shard collisions are RocksDB's concern: two shards
    /// that both touch the same key each emit their own ops, and
    /// the registered merge operator (for merge-CFs) or
    /// last-write semantics (for put-CFs) reconcile them.
    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    //! End-to-end exercise of [`Restore::restore`] against an
    //! in-memory `Db`. No restore drivers here — the test calls
    //! [`Restore::restore`] directly to verify the trait shape
    //! compiles and integrates with the [`Batch`] write path.

    use sui_types::base_types::ObjectID;
    use sui_types::object::Object;

    use super::*;
    use crate::restore::test_pipeline::ObjectIdKey;
    use crate::restore::test_pipeline::ObjectVersionPipeline;
    use crate::restore::test_pipeline::U64Be;
    use crate::restore::test_pipeline::open;

    #[test]
    fn restore_writes_each_object_directly() {
        let (_dir, db, schema) = open();
        let pipeline = ObjectVersionPipeline;

        let o1 = Object::immutable_with_id_for_testing(ObjectID::from_single_byte(1));
        let o2 = Object::immutable_with_id_for_testing(ObjectID::from_single_byte(2));

        let mut batch = db.batch();
        pipeline.restore(&schema, &o1, &mut batch).unwrap();
        pipeline.restore(&schema, &o2, &mut batch).unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.versions.get(&ObjectIdKey::new(o1.id())).unwrap(),
            Some(U64Be(o1.version().value())),
        );
        assert_eq!(
            schema.versions.get(&ObjectIdKey::new(o2.id())).unwrap(),
            Some(U64Be(o2.version().value())),
        );
    }

    #[test]
    fn empty_batch_commit_writes_nothing() {
        let (_dir, db, schema) = open();
        let batch = db.batch();
        batch.commit().unwrap();
        let rows = schema.versions.iter(..).unwrap().count();
        assert_eq!(rows, 0);
    }
}

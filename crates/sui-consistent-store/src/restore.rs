// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The [`Restore`] trait — bulk-load shape for an indexing
//! pipeline driven from a stream of live objects (formal snapshot,
//! perpetual store).
//!
//! `Restore` is a sibling shape to the indexer-alt framework's
//! tip-of-chain `Processor` + `sequential::Handler` (or
//! `concurrent::Handler`). It is bounded `: Processor` so that a
//! pipeline shares one identity (`Processor::NAME`) and one
//! configuration type across both bulk-load and tip phases, but the
//! tip impl is independent — `Restore` does not touch tip data.
//!
//! # Method roles
//!
//! - [`restore`](Restore::restore) takes a single live object and
//!   stages writes derived from it onto the shared per-shard
//!   [`Batch`]. Restore drivers parallelize across input objects
//!   however they see fit (e.g. one tokio task per snapshot
//!   partition, one thread per `ObjectID` range). Each worker owns
//!   its own [`Batch`]; the driver commits the batch atomically
//!   alongside any other state it owns (a partition-complete marker
//!   in the `__restore` CF, for example) once the shard is done.
//!
//! Per-object writes that target merge-CFs combine via the
//! registered merge operator across shards (so cross-shard sums,
//! counters, set unions all converge correctly). Writes to put-CFs
//! have last-write-wins semantics; drivers that need a different
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

    use std::sync::Arc;

    use async_trait::async_trait;
    use bytes::Buf;
    use bytes::BufMut;
    use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
    use sui_types::base_types::ObjectID;
    use sui_types::object::Object;
    use tempfile::TempDir;

    use super::*;
    use crate::CfDescriptor;
    use crate::Db;
    use crate::DbMap;
    use crate::DbOptions;
    use crate::Decode;
    use crate::Encode;
    use crate::Schema;
    use crate::error::DecodeError;
    use crate::error::EncodeError;
    use crate::error::OpenError;

    /// Big-endian `ObjectID` newtype, suitable as a typed key.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct ObjectIdKey([u8; ObjectID::LENGTH]);

    impl ObjectIdKey {
        fn new(id: ObjectID) -> Self {
            Self(id.into_bytes())
        }
    }

    impl Encode for ObjectIdKey {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0);
            Ok(())
        }
    }

    impl Decode for ObjectIdKey {
        fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != ObjectID::LENGTH {
                return Err(DecodeError::msg("unexpected ObjectIdKey length"));
            }
            let mut id = [0u8; ObjectID::LENGTH];
            buf.copy_to_slice(&mut id);
            Ok(Self(id))
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64Be(u64);

    impl Encode for U64Be {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for U64Be {
        fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg("expected 8 bytes"));
            }
            Ok(Self(buf.get_u64()))
        }
    }

    #[derive(Debug)]
    struct ObjectVersionSchema {
        versions: DbMap<ObjectIdKey, U64Be>,
    }

    impl Schema for ObjectVersionSchema {
        fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
            vec![CfDescriptor::new("versions", base_options.clone())]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                versions: DbMap::new(db.clone(), "versions")?,
            })
        }
    }

    /// Test pipeline: writes (object_id → version) per object.
    struct ObjectVersionPipeline;

    #[async_trait]
    impl Processor for ObjectVersionPipeline {
        const NAME: &'static str = "object_version";
        type Value = ();

        async fn process(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            // Restore-only test pipeline; tip path not exercised.
            Ok(vec![])
        }
    }

    impl Restore for ObjectVersionPipeline {
        type Schema = ObjectVersionSchema;

        fn restore(
            &self,
            schema: &Self::Schema,
            object: &Object,
            batch: &mut Batch,
        ) -> anyhow::Result<()> {
            batch.put(
                &schema.versions,
                &ObjectIdKey::new(object.id()),
                &U64Be(object.version().value()),
            )?;
            Ok(())
        }
    }

    fn open() -> (TempDir, Db, ObjectVersionSchema) {
        let dir = TempDir::new().unwrap();
        let (db, schema) =
            Db::open::<ObjectVersionSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

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

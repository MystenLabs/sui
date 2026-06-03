// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::objects`](crate::schema::objects) CF: one row per
//! `(ObjectID, version)` written or removed by any transaction in
//! the checkpoint.
//!
//! Every output version is preserved — historical versions accrue
//! so callers can read an object at any version it has ever
//! existed at. Intra-checkpoint intermediate versions (created
//! when a shared object is touched by multiple transactions in
//! the same checkpoint) are all retained.
//!
//! In addition to live versions, every object that was deleted or
//! wrapped by a transaction gets a tombstone row at the
//! transaction's lamport version. Tombstones let version-bounded
//! reads tell "no row at this version" (object did not exist)
//! apart from "tombstone row at this version" (object was
//! removed). `unwrapped_then_deleted` objects also get a `Deleted`
//! tombstone, matching the validator's perpetual-store semantics.

use std::sync::Arc;

use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use crate::RpcStoreSchema;
use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::objects;
use crate::schema::objects::TombstoneKind;

/// Pipeline marker for `objects`.
pub struct Objects;

pub struct Row {
    pub id: ObjectID,
    pub version: SequenceNumber,
    pub value: objects::Value,
}

#[async_trait]
impl Processor for Objects {
    const NAME: &'static str = "objects";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for tx in &checkpoint.transactions {
            for object in tx.output_objects(&checkpoint.object_set) {
                rows.push(Row {
                    id: object.id(),
                    version: object.version(),
                    value: objects::store(object),
                });
            }
            let lamport_version = tx.effects.lamport_version();
            for (id, _) in tx
                .effects
                .deleted()
                .into_iter()
                .chain(tx.effects.unwrapped_then_deleted())
                .map(|oref| (oref.0, oref.1))
            {
                rows.push(Row {
                    id,
                    version: lamport_version,
                    value: objects::tombstone(TombstoneKind::Deleted),
                });
            }
            for oref in tx.effects.wrapped() {
                rows.push(Row {
                    id: oref.0,
                    version: lamport_version,
                    value: objects::tombstone(TombstoneKind::Wrapped),
                });
            }
        }
        Ok(rows)
    }
}

impl Restore for Objects {
    type Schema = RpcStoreSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        // Restoration runs against a live-object snapshot, so each
        // object contributes exactly one `(id, version)` row at its
        // current version. Historical versions are not present in
        // the source and accrue only from tip indexing onward.
        batch.put(
            &schema.objects,
            &objects::Key {
                id: object.id(),
                version: object.version(),
            },
            &objects::store(object),
        )?;
        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for Objects {
    type Store = Store;
    type Batch = Vec<Row>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Row>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().objects;
        for row in batch {
            conn.batch.put(
                cf,
                &objects::Key {
                    id: row.id,
                    version: row.version,
                },
                &row.value,
            )?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::SuiAddress;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = Objects.process(&checkpoint).await.unwrap();
    }

    /// End-to-end: drive a checkpoint that creates an object, then
    /// deletes one and wraps another. The pipeline should emit a
    /// live row for every output object and a tombstone (with the
    /// right kind) for every deleted / wrapped id, all at the
    /// transaction's lamport version.
    #[tokio::test]
    async fn process_emits_tombstones_for_deleted_and_wrapped_objects() {
        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(1)
                .start_transaction(0)
                .create_owned_object(0)
                .create_owned_object(1)
                .finish_transaction()
                .start_transaction(0)
                .delete_object(0)
                .finish_transaction()
                .start_transaction(0)
                .wrap_object(1)
                .finish_transaction()
                .build_checkpoint(),
        );

        let deleted_id = TestCheckpointBuilder::derive_object_id(0);
        let wrapped_id = TestCheckpointBuilder::derive_object_id(1);
        let delete_lamport = checkpoint.transactions[1].effects.lamport_version();
        let wrap_lamport = checkpoint.transactions[2].effects.lamport_version();

        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let rows = Objects.process(&checkpoint).await.unwrap();
        let mut batch = db.batch();
        for row in &rows {
            batch
                .put(
                    &schema.objects,
                    &objects::Key {
                        id: row.id,
                        version: row.version,
                    },
                    &row.value,
                )
                .unwrap();
        }
        batch.commit().unwrap();

        // Tombstones land at the deleting / wrapping tx's lamport
        // version with the right kind.
        assert_eq!(
            schema
                .get_object_status_by_key(deleted_id, delete_lamport)
                .unwrap(),
            Some(objects::Status::Tombstone(TombstoneKind::Deleted)),
        );
        assert_eq!(
            schema
                .get_object_status_by_key(wrapped_id, wrap_lamport)
                .unwrap(),
            Some(objects::Status::Tombstone(TombstoneKind::Wrapped)),
        );

        // `get_object_by_key` flattens tombstones to None, matching
        // its "live object or nothing" contract.
        assert!(
            schema
                .get_object_by_key(deleted_id, delete_lamport)
                .unwrap()
                .is_none(),
        );
        assert!(
            schema
                .get_object_by_key(wrapped_id, wrap_lamport)
                .unwrap()
                .is_none(),
        );
    }

    #[test]
    fn restore_writes_one_row_per_object_version() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let o1 = Object::with_id_owner_for_testing(ObjectID::from_single_byte(1), SuiAddress::ZERO);
        let o2 = Object::with_id_owner_for_testing(ObjectID::from_single_byte(2), SuiAddress::ZERO);

        let mut batch = db.batch();
        Objects.restore(&schema, &o1, &mut batch).unwrap();
        Objects.restore(&schema, &o2, &mut batch).unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_object_by_key(o1.id(), o1.version()).unwrap(),
            Some(o1),
        );
        assert_eq!(
            schema.get_object_by_key(o2.id(), o2.version()).unwrap(),
            Some(o2),
        );
    }
}

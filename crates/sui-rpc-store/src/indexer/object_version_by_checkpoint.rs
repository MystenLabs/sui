// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::object_version_by_checkpoint`](crate::schema::object_version_by_checkpoint)
//! CF: one row per object that changed in the checkpoint, carrying the
//! object's final version at the end of that checkpoint.
//!
//! For an object that is live at the end of the checkpoint the row
//! records its final live version; for an object that was deleted or
//! wrapped (and not re-created) the row records the tombstone version
//! (the removing transaction's lamport version, matching the row the
//! [`objects`](crate::indexer::objects) pipeline writes). A read pinned
//! at the checkpoint therefore resolves to the object's true state --
//! including "removed" -- rather than a stale earlier version.
//!
//! Only the object's *final* version in the checkpoint is recorded;
//! intra-checkpoint intermediate versions remain individually
//! addressable through the version-keyed
//! [`objects`](crate::schema::objects) CF.
//!
//! On restore, a live-object snapshot carries no per-checkpoint
//! history, so each restored object contributes a single row at the
//! restore anchor checkpoint (see [`ObjectVersionByCheckpoint::for_restore`]).
//! That anchor is also the post-restore available-range floor, so no
//! in-range read asks for an earlier checkpoint.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context as _;
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
use crate::indexer::checkpoint_output_objects;
use crate::schema::object_version_by_checkpoint;

/// Pipeline marker for `object_version_by_checkpoint`.
///
/// `restore_checkpoint` is set only for the restore registration: it
/// is the anchor checkpoint every restored live object is attributed
/// to. Tip indexing leaves it `None` and takes each row's checkpoint
/// from the checkpoint being processed.
#[derive(Default)]
pub struct ObjectVersionByCheckpoint {
    restore_checkpoint: Option<u64>,
}

pub struct Row {
    pub id: ObjectID,
    pub checkpoint: u64,
    pub version: SequenceNumber,
}

impl ObjectVersionByCheckpoint {
    /// Restore marker attributing every restored live object to the
    /// snapshot anchor `checkpoint`. A live-object snapshot has no
    /// historical versions, so this is the lowest checkpoint at which
    /// the object's state is known -- and the post-restore available
    /// range starts here, so no in-range read asks for an earlier one.
    pub fn for_restore(checkpoint: u64) -> Self {
        Self {
            restore_checkpoint: Some(checkpoint),
        }
    }
}

#[async_trait]
impl Processor for ObjectVersionByCheckpoint {
    const NAME: &'static str = "object_version_by_checkpoint";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let cp = checkpoint.summary.data().sequence_number;

        // Objects live at the end of the checkpoint, each with its
        // final version.
        let outputs = checkpoint_output_objects(checkpoint)?;
        let mut rows: Vec<Row> = outputs
            .iter()
            .map(|(id, (object, _))| Row {
                id: *id,
                checkpoint: cp,
                version: object.version(),
            })
            .collect();

        // Objects removed (deleted or wrapped) during the checkpoint:
        // record the tombstone version -- the removing transaction's
        // lamport version, where the `objects` pipeline writes the
        // tombstone row -- keeping the highest such version if an id is
        // touched more than once. A read pinned at `cp` then resolves
        // to the tombstone (and thus "no live object") instead of the
        // stale prior version.
        let mut removed: BTreeMap<ObjectID, SequenceNumber> = BTreeMap::new();
        for tx in &checkpoint.transactions {
            let lamport = tx.effects.lamport_version();
            for oref in tx
                .effects
                .deleted()
                .into_iter()
                .chain(tx.effects.unwrapped_then_deleted())
                .chain(tx.effects.wrapped())
            {
                removed
                    .entry(oref.0)
                    .and_modify(|v| *v = (*v).max(lamport))
                    .or_insert(lamport);
            }
        }

        for (id, version) in removed {
            // Removed then re-created within the same checkpoint (e.g.
            // wrapped then unwrapped) -- it is live at the end and
            // already covered by its output row above.
            if outputs.contains_key(&id) {
                continue;
            }
            rows.push(Row {
                id,
                checkpoint: cp,
                version,
            });
        }

        Ok(rows)
    }
}

impl Restore for ObjectVersionByCheckpoint {
    type Schema = RpcStoreSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        // Restoration runs against a live-object snapshot with no
        // per-checkpoint history, so every live object contributes one
        // row at the restore anchor. The anchor is supplied at
        // registration (`for_restore`); a tip-mode marker would never
        // be registered with the restore driver, so its absence is a
        // programmer error.
        let checkpoint = self
            .restore_checkpoint
            .context("object_version_by_checkpoint restored without a restore anchor checkpoint")?;
        let (key, value) =
            object_version_by_checkpoint::store(object.id(), checkpoint, object.version());
        batch.put(&schema.object_version_by_checkpoint, &key, &value)?;
        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for ObjectVersionByCheckpoint {
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
        let cf = &conn.store.schema().object_version_by_checkpoint;
        for row in batch {
            let (k, v) = object_version_by_checkpoint::store(row.id, row.checkpoint, row.version);
            conn.batch.put(cf, &k, &v)?;
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
        let _rows = ObjectVersionByCheckpoint::default()
            .process(&checkpoint)
            .await
            .unwrap();
    }

    /// A live object created in the checkpoint gets one row at the
    /// checkpoint's sequence number, carrying its current version.
    #[tokio::test]
    async fn process_records_final_live_version() {
        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(7)
                .start_transaction(0)
                .create_owned_object(0)
                .finish_transaction()
                .build_checkpoint(),
        );
        let created_id = TestCheckpointBuilder::derive_object_id(0);
        let version = checkpoint.transactions[0].effects.lamport_version();

        let rows = ObjectVersionByCheckpoint::default()
            .process(&checkpoint)
            .await
            .unwrap();
        let row = rows
            .iter()
            .find(|r| r.id == created_id)
            .expect("created object recorded");
        assert_eq!(row.checkpoint, 7);
        assert_eq!(row.version, version);
    }

    /// An object deleted in the checkpoint is recorded at the
    /// tombstone (lamport) version, not its prior live version, so a
    /// read pinned at the checkpoint resolves to "removed".
    #[tokio::test]
    async fn process_records_tombstone_for_deleted_object() {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let _cp0 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let deleted_id = TestCheckpointBuilder::derive_object_id(0);
        let tombstone_version = cp1.transactions[0].effects.lamport_version();

        let rows = ObjectVersionByCheckpoint::default()
            .process(&cp1)
            .await
            .unwrap();
        let row = rows
            .iter()
            .find(|r| r.id == deleted_id)
            .expect("deleted object recorded");
        assert_eq!(row.checkpoint, 1);
        assert_eq!(row.version, tombstone_version);
    }

    /// Restore attributes every live object to the anchor checkpoint.
    #[test]
    fn restore_writes_one_row_at_the_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let object =
            Object::with_id_owner_for_testing(ObjectID::from_single_byte(1), SuiAddress::ZERO);

        let mut batch = db.batch();
        ObjectVersionByCheckpoint::for_restore(123)
            .restore(&schema, &object, &mut batch)
            .unwrap();
        batch.commit().unwrap();

        // The object resolves at and after the anchor, and not before.
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(object.id(), 123)
                .unwrap(),
            Some(object.version()),
        );
        assert_eq!(
            schema
                .get_object_version_at_checkpoint(object.id(), 200)
                .unwrap(),
            Some(object.version()),
        );
        assert!(
            schema
                .get_object_version_at_checkpoint(object.id(), 122)
                .unwrap()
                .is_none()
        );
    }
}

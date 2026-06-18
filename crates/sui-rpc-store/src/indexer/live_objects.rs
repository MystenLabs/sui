// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::live_objects`](crate::schema::live_objects) CF: the
//! `ObjectID → latest_version` pointer per live object.
//!
//! The pipeline reads the checkpoint as a diff and emits two kinds
//! of rows. Inputs (objects that existed prior to the checkpoint)
//! always emit a `Delete`; outputs (objects still live at the end
//! of the checkpoint) always emit a `Put`. RocksDB applies the
//! batch in insertion order, so for an object that was modified
//! but still exists the `Put` wins over the earlier `Delete`. For
//! an object that was deleted or wrapped, only the `Delete`
//! lands.

use std::sync::Arc;

use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use crate::RpcStoreSchema;
use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::keys::U64Varint;
use crate::schema::live_objects;

/// Pipeline marker for `live_objects`.
pub struct LiveObjects;

pub enum Row {
    /// The object was an input to some transaction in the
    /// checkpoint — retract its prior live pointer. A `Put` later
    /// in the batch wins if the object still exists at the end of
    /// the checkpoint.
    Delete { id: ObjectID },
    /// The object is still live at the end of the checkpoint —
    /// set its live pointer to `version`.
    Put {
        id: ObjectID,
        version: SequenceNumber,
    },
}

#[async_trait]
impl Processor for LiveObjects {
    const NAME: &'static str = "live_objects";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for (id, _) in checkpoint_input_objects(checkpoint)? {
            rows.push(Row::Delete { id });
        }
        for (id, (object, _)) in checkpoint_output_objects(checkpoint)? {
            rows.push(Row::Put {
                id,
                version: object.version(),
            });
        }
        Ok(rows)
    }
}

impl Restore for LiveObjects {
    type Schema = RpcStoreSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        batch.put(
            &schema.live_objects,
            &live_objects::Key(object.id()),
            &U64Varint(object.version().value()),
        )?;
        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for LiveObjects {
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
        let cf = &conn.store.schema().live_objects;
        for row in batch {
            match row {
                Row::Delete { id } => {
                    conn.batch.delete(cf, &live_objects::Key(*id))?;
                }
                Row::Put { id, version } => {
                    conn.batch
                        .put(cf, &live_objects::Key(*id), &U64Varint(version.value()))?;
                }
            }
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
        let _rows = LiveObjects.process(&checkpoint).await.unwrap();
    }

    #[test]
    fn restore_writes_live_pointer_per_object() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let o1 = Object::with_id_owner_for_testing(ObjectID::from_single_byte(1), SuiAddress::ZERO);
        let o2 = Object::with_id_owner_for_testing(ObjectID::from_single_byte(2), SuiAddress::ZERO);

        let mut batch = db.batch();
        LiveObjects.restore(&schema, &o1, &mut batch).unwrap();
        LiveObjects.restore(&schema, &o2, &mut batch).unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema.get_live_object_version(o1.id()).unwrap(),
            Some(o1.version()),
        );
        assert_eq!(
            schema.get_live_object_version(o2.id()).unwrap(),
            Some(o2.version()),
        );
    }
}

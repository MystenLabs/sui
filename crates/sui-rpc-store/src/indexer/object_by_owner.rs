// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::object_by_owner`](crate::schema::object_by_owner)
//! index.
//!
//! Like [`live_objects`](crate::indexer::live_objects), the
//! pipeline reads the checkpoint as a diff: inputs emit a
//! `Delete` keyed by the *prior* `(kind, owner, type, balance,
//! id)`, outputs emit a `Put` keyed by the *posterior*
//! `(kind, owner, type, balance, id)`. For an object that was
//! merely modified the two rows land at the same key only if the
//! key components didn't change; otherwise the index correctly
//! moves the row from the old key to the new one.

use std::sync::Arc;

use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use crate::RpcStoreSchema;
use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::keys::U64Varint;
use crate::schema::object_by_owner;

/// Pipeline marker for `object_by_owner`.
pub struct ObjectByOwner;

pub enum Row {
    Delete(object_by_owner::Key),
    Put(object_by_owner::Key, U64Varint),
}

#[async_trait]
impl Processor for ObjectByOwner {
    const NAME: &'static str = "object_by_owner";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for (_, (input, _)) in checkpoint_input_objects(checkpoint)? {
            if let Some((key, _)) = object_by_owner::store(input) {
                rows.push(Row::Delete(key));
            }
        }
        for (_, (output, _)) in checkpoint_output_objects(checkpoint)? {
            if let Some((key, version)) = object_by_owner::store(output) {
                rows.push(Row::Put(key, version));
            }
        }
        Ok(rows)
    }
}

impl Restore for ObjectByOwner {
    type Schema = RpcStoreSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        if let Some((key, value)) = object_by_owner::store(object) {
            batch.put(&schema.object_by_owner, &key, &value)?;
        }
        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for ObjectByOwner {
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
        let cf = &conn.store.schema().object_by_owner;
        for row in batch {
            match row {
                Row::Delete(key) => {
                    conn.batch.delete(cf, key)?;
                }
                Row::Put(key, value) => {
                    conn.batch.put(cf, key, value)?;
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
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = ObjectByOwner.process(&checkpoint).await.unwrap();
    }

    #[test]
    fn restore_indexes_address_owned_object() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let owner = SuiAddress::ZERO;
        let object = Object::with_id_owner_for_testing(ObjectID::from_single_byte(3), owner);

        let mut batch = db.batch();
        ObjectByOwner.restore(&schema, &object, &mut batch).unwrap();
        batch.commit().unwrap();

        let rows: Vec<_> = schema
            .iter_objects_owned_by_address(owner)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.object_id, object.id());
    }
}

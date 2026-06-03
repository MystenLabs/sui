// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::objects`](crate::schema::objects) CF: one row per
//! `(ObjectID, version)` written by any transaction in the
//! checkpoint.
//!
//! Every output version is preserved — historical versions accrue
//! so callers can read an object at any version it has ever
//! existed at. Intra-checkpoint intermediate versions (created
//! when a shared object is touched by multiple transactions in
//! the same checkpoint) are all retained.

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
use crate::schema::objects;

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

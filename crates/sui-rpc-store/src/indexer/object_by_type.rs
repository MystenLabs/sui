// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::object_by_type`](crate::schema::object_by_type)
//! index, using the same `Delete`-then-`Put` diff pattern as
//! [`object_by_owner`](crate::indexer::object_by_owner).

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::keys::U64Varint;
use crate::schema::object_by_type;

/// Pipeline marker for `object_by_type`.
pub struct ObjectByType;

pub enum Row {
    Delete(object_by_type::Key),
    Put(object_by_type::Key, U64Varint),
}

#[async_trait]
impl Processor for ObjectByType {
    const NAME: &'static str = "object_by_type";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for (_, (input, _)) in checkpoint_input_objects(checkpoint)? {
            if let Some((key, _)) = object_by_type::store(input) {
                rows.push(Row::Delete(key));
            }
        }
        for (_, (output, _)) in checkpoint_output_objects(checkpoint)? {
            if let Some((key, version)) = object_by_type::store(output) {
                rows.push(Row::Put(key, version));
            }
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for ObjectByType {
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
        let cf = &conn.store.schema().object_by_type;
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

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = ObjectByType.process(&checkpoint).await.unwrap();
    }
}

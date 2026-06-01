// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::package_versions`](crate::schema::package_versions)
//! CF: one row per `(original_package_id, version)` published in
//! the checkpoint.
//!
//! Pure puts — packages are immutable once written, so a later
//! publish at the same `(original_id, version)` (which would
//! itself be a chain-level error) deterministically overwrites
//! the prior storage id rather than dueling with it.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Data;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_output_objects;
use crate::schema::package_versions;

/// Pipeline marker for `package_versions`.
pub struct PackageVersions;

pub struct Row {
    pub original_id: ObjectID,
    pub version: u64,
    pub storage_id: ObjectID,
}

#[async_trait]
impl Processor for PackageVersions {
    const NAME: &'static str = "package_versions";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for (_, (object, _)) in checkpoint_output_objects(checkpoint)? {
            if let Data::Package(pkg) = &object.data {
                rows.push(Row {
                    original_id: pkg.original_package_id(),
                    version: pkg.version().value(),
                    storage_id: pkg.id(),
                });
            }
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for PackageVersions {
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
        let cf = &conn.store.schema().package_versions;
        for row in batch {
            let (k, v) = package_versions::store(row.original_id, row.version, row.storage_id);
            conn.batch.put(cf, &k, &v)?;
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
        let _rows = PackageVersions.process(&checkpoint).await.unwrap();
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::package_versions`](crate::schema::package_versions)
//! CF: one row per `(original_package_id, version)` published in
//! the checkpoint.
//!
//! Each row records the storage id and the checkpoint at which the
//! version was published. The live-set restore writes its rows via
//! [`store_restored`](crate::schema::package_versions::store_restored),
//! leaving the publish checkpoint unset (a restore floor) for
//! versions that predate the available window.
//!
//! Pure puts — packages are immutable once written, so a later
//! publish at the same `(original_id, version)` (which would
//! itself be a chain-level error) deterministically overwrites
//! the prior storage id rather than dueling with it.

use std::sync::Arc;

use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Data;
use sui_types::object::Object;

use crate::RpcStoreSchema;
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
    pub checkpoint: u64,
}

#[async_trait]
impl Processor for PackageVersions {
    const NAME: &'static str = "package_versions";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let cp = checkpoint.summary.sequence_number;
        let mut rows = Vec::new();
        for (_, (object, _)) in checkpoint_output_objects(checkpoint)? {
            if let Data::Package(pkg) = &object.data {
                rows.push(Row {
                    original_id: pkg.original_package_id(),
                    version: pkg.version().value(),
                    storage_id: pkg.id(),
                    checkpoint: cp,
                });
            }
        }
        Ok(rows)
    }
}

impl Restore for PackageVersions {
    type Schema = RpcStoreSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        // Packages are immutable, so each published version lives
        // forever as its own live object. The live object set
        // therefore contains every package version ever published,
        // each emitting a single row mapping
        // `(original_id, version) → storage_id`.
        let Data::Package(pkg) = &object.data else {
            return Ok(());
        };
        let (key, value) = package_versions::store_restored(
            pkg.original_package_id(),
            pkg.version().value(),
            pkg.id(),
        );
        batch.put(&schema.package_versions, &key, &value)?;
        Ok(())
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
            let (k, v) = package_versions::store(
                row.original_id,
                row.version,
                row.storage_id,
                row.checkpoint,
            );
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
    use sui_types::object::Object;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = PackageVersions.process(&checkpoint).await.unwrap();
    }

    /// Verify the non-package skip branch: ordinary Move objects
    /// have `Data::Move`, not `Data::Package`, and must not produce
    /// a `package_versions` row. The happy-path encoding is shared
    /// with the tip pipeline via [`package_versions::store`] and
    /// covered there.
    #[test]
    fn restore_skips_non_package_objects() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let non_pkg =
            Object::with_id_owner_for_testing(ObjectID::from_single_byte(7), SuiAddress::ZERO);
        let mut batch = db.batch();
        PackageVersions
            .restore(&schema, &non_pkg, &mut batch)
            .unwrap();
        batch.commit().unwrap();

        // No row written: an `iter_package_versions` over the
        // (non-existent) original_id of a non-package returns an
        // empty iterator.
        let rows: Vec<_> = schema
            .iter_package_versions(non_pkg.id())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(rows.is_empty());
    }
}

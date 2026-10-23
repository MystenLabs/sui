// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::package_versions`](crate::schema::package_versions)
//! CF: one row per `(original_package_id, version)` published in
//! the checkpoint.
//!
//! Each row records the storage id and the checkpoint at which the
//! version was published. As a history-cohort member this CF is both
//! restored and backfilled: the live-set restore writes its rows via
//! [`store_restored`](crate::schema::package_versions::store_restored),
//! leaving the publish checkpoint unset (a restore floor), and the
//! embedded backfill of `(L, T]` then re-publishes every version
//! created in that window with its real publish checkpoint, overwriting
//! the floor. Versions that predate the available window are never
//! re-published, so they keep their floor and read as having always
//! existed.
//!
//! The live set holds only the latest version of a package that is
//! upgraded in place under one id (the system packages), so the restore
//! misses the version it had when the window opened if it was upgraded
//! in the window. The backfill recovers it from the upgrade's input:
//! an input package version with no row was published before the
//! history this store holds, so it is written like a restore floor. An
//! input version that already has a row (published in the window, or
//! by a from-genesis build) is left as is.
//!
//! Publishes are pure puts — packages are immutable once written, so a
//! later publish at the same `(original_id, version)` (which would
//! itself be a chain-level error) deterministically overwrites
//! the prior storage id rather than dueling with it. The same
//! overwrite is what lets the backfill replace a restore floor with
//! the real publish checkpoint.

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
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::package_versions;

/// Pipeline marker for `package_versions`.
pub struct PackageVersions;

pub enum Row {
    /// A package version published in `checkpoint`.
    Published {
        original_id: ObjectID,
        version: u64,
        storage_id: ObjectID,
        checkpoint: u64,
    },
    /// A package version upgraded in place, seen as its upgrade's input. Recorded with no
    /// publish checkpoint if the store has no row for it yet.
    UpgradeInput {
        original_id: ObjectID,
        version: u64,
        storage_id: ObjectID,
    },
}

#[async_trait]
impl Processor for PackageVersions {
    const NAME: &'static str = "package_versions";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let cp = checkpoint.summary.sequence_number;
        let mut rows = Vec::new();
        // Packages only appear as inputs when they are upgraded in place; ordinary packages are
        // immutable and never change.
        for (_, (object, _)) in checkpoint_input_objects(checkpoint)? {
            if let Data::Package(pkg) = &object.data {
                rows.push(Row::UpgradeInput {
                    original_id: pkg.original_package_id(),
                    version: pkg.version().value(),
                    storage_id: pkg.id(),
                });
            }
        }
        for (_, (object, _)) in checkpoint_output_objects(checkpoint)? {
            if let Data::Package(pkg) = &object.data {
                rows.push(Row::Published {
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
        // Every version of an ordinary package lives forever as its
        // own immutable object, so the live set holds all of them. A
        // package upgraded in place keeps only its latest version
        // here; the backfill recovers the earlier one (see the module
        // docs).
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
        stage_rows(conn.store.schema(), batch, &mut conn.batch)
    }
}

/// Stage `rows` into `batch`, returning how many rows were written. Each commit covers one
/// checkpoint, so an upgrade input never depends on a publish staged in the same batch.
fn stage_rows(schema: &RpcStoreSchema, rows: &[Row], batch: &mut Batch) -> anyhow::Result<usize> {
    let cf = &schema.package_versions;
    let mut written = 0;
    for row in rows {
        let (key, value) = match *row {
            Row::Published {
                original_id,
                version,
                storage_id,
                checkpoint,
            } => package_versions::store(original_id, version, storage_id, checkpoint),
            Row::UpgradeInput {
                original_id,
                version,
                storage_id,
            } => {
                let (key, value) =
                    package_versions::store_restored(original_id, version, storage_id);
                if cf.get(&key)?.is_some() {
                    continue;
                }
                (key, value)
            }
        };
        batch.put(cf, &key, &value)?;
        written += 1;
    }
    Ok(written)
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

    /// `(version, publish checkpoint)` of every row for `original_id`.
    fn stored(schema: &RpcStoreSchema, original_id: ObjectID) -> Vec<(u64, Option<u64>)> {
        schema
            .iter_package_versions(original_id)
            .unwrap()
            .map(|row| {
                let (key, value) = row.unwrap();
                (key.version, value.into_inner().checkpoint)
            })
            .collect()
    }

    /// A package upgraded in place inside the backfill window: the restore loaded only its
    /// latest version, and the upgrade's input records the version it entered the window with,
    /// so a read below the upgrade resolves to that version instead of finding nothing.
    #[test]
    fn upgrade_input_records_the_version_entering_history() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let original_id = ObjectID::from_single_byte(2);

        // The restore at the anchor loaded v3, the live version.
        let mut batch = db.batch();
        let (key, value) = package_versions::store_restored(original_id, 3, original_id);
        batch.put(&schema.package_versions, &key, &value).unwrap();
        batch.commit().unwrap();

        // The backfill reaches the in-window upgrade from v2 to v3 at checkpoint 50.
        let upgrade = [
            Row::UpgradeInput {
                original_id,
                version: 2,
                storage_id: original_id,
            },
            Row::Published {
                original_id,
                version: 3,
                storage_id: original_id,
                checkpoint: 50,
            },
        ];
        let mut batch = db.batch();
        assert_eq!(stage_rows(&schema, &upgrade, &mut batch).unwrap(), 2);
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);
        assert_eq!(
            schema
                .get_package_at_checkpoint(original_id, 30)
                .unwrap()
                .map(|(version, _)| version),
            Some(2),
        );

        // Re-indexing the same checkpoint leaves the recorded input untouched.
        let mut batch = db.batch();
        assert_eq!(stage_rows(&schema, &upgrade, &mut batch).unwrap(), 1);
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);

        // An input whose publish was already indexed keeps its publish checkpoint.
        let next_upgrade = [Row::UpgradeInput {
            original_id,
            version: 3,
            storage_id: original_id,
        }];
        let mut batch = db.batch();
        assert_eq!(stage_rows(&schema, &next_upgrade, &mut batch).unwrap(), 0);
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);
    }
}

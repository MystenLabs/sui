// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::package_versions`](crate::schema::package_versions)
//! CF: one row per `(original_package_id, version)` published in
//! the checkpoint, plus the matching
//! [`schema::packages_by_checkpoint`](crate::schema::packages_by_checkpoint)
//! row in the same commit batch, so the checkpoint-ordered index
//! cannot drift from `package_versions`.
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
//! by a from-genesis build), or whose publish this run has staged but
//! the store has not written yet, is left as is.
//!
//! Publishes are pure puts — packages are immutable once written, so a
//! later publish at the same `(original_id, version)` (which would
//! itself be a chain-level error) deterministically overwrites
//! the prior storage id rather than dueling with it. The same
//! overwrite is what lets the backfill replace a restore floor with
//! the real publish checkpoint.
//!
//! The live-set restore writes no `packages_by_checkpoint` rows: a
//! restore floor has no publish checkpoint to key it by. The backfill
//! writes that index row when it re-publishes the version with its
//! real checkpoint.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::is_system_package;
use sui_types::object::Data;
use sui_types::object::Object;

use crate::RpcStoreSchema;
use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::package_versions;
use crate::schema::packages_by_checkpoint;

/// Pipeline marker for `package_versions`.
#[derive(Default)]
pub struct PackageVersions {
    /// The restore anchor `T`, or `None` for a store that was never restored. Upgrade inputs
    /// only matter inside the backfill window `[L, T]` (see the module docs), so the processor
    /// only scans checkpoints at or below `T` for them.
    anchor: Option<u64>,
    /// The `(original_id, version)` of every system package version whose publish this run has
    /// staged. The synchronizer can still hold an earlier checkpoint's batch, unwritten, when a
    /// later checkpoint's commit stages, so an upgrade input can miss its publish in the store;
    /// this set keeps such an input from overwriting the publish with a restore floor. Only
    /// packages upgraded in place are ever upgrade inputs, and only system packages are upgraded
    /// in place, so the set holds only those and stays small. The [`Mutex`] only guards interior
    /// mutation behind `&self`; commits are sequential, so it is never contended.
    staged_system_publishes: Mutex<HashSet<(ObjectID, u64)>>,
}

impl PackageVersions {
    /// Marker for the tip/backfill registration, carrying the restore anchor `T` (or `None` for a
    /// store that was never restored) so the processor scopes its upgrade-input scan to the
    /// backfill window `[L, T]`.
    pub fn with_anchor(anchor: Option<u64>) -> Self {
        Self {
            anchor,
            ..Self::default()
        }
    }
}

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
        // immutable and never change. An upgrade input only lacks a row for a version the
        // restore's live set missed, which was live when the backfill window `[L, T]` opened and
        // so is an input at or below `T`. Past `T`, or in a store that was never restored, every
        // input version already has a row, so the scan is skipped.
        if self.anchor.is_some_and(|t| cp <= t) {
            for (_, (object, _)) in checkpoint_input_objects(checkpoint)? {
                if let Data::Package(pkg) = &object.data {
                    rows.push(Row::UpgradeInput {
                        original_id: pkg.original_package_id(),
                        version: pkg.version().value(),
                        storage_id: pkg.id(),
                    });
                }
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
        let mut staged = self
            .staged_system_publishes
            .lock()
            .expect("staged system publishes mutex poisoned");
        stage_rows(conn.store.schema(), batch, &mut staged, &mut conn.batch)
    }
}

/// Stage `rows` into `batch`, returning how many rows were written. A batch can span many
/// checkpoints, so an upgrade input can depend on a publish staged earlier in the same batch, or
/// on one an earlier commit staged that the store has not written yet. `staged` holds the system
/// package publishes staged so far and gains each one as its row is staged, so an input sees
/// every publish in the rows before it.
///
/// A publish also stages its `packages_by_checkpoint` row in the same batch. An upgrade input
/// recorded with no publish checkpoint has nothing to key that index by, so it stages none.
fn stage_rows(
    schema: &RpcStoreSchema,
    rows: &[Row],
    staged: &mut HashSet<(ObjectID, u64)>,
    batch: &mut Batch,
) -> anyhow::Result<usize> {
    let cf = &schema.package_versions;
    let mut written = 0;
    for row in rows {
        let (key, value) = match *row {
            Row::Published {
                original_id,
                version,
                storage_id,
                checkpoint,
            } => {
                if is_system_package(original_id) {
                    staged.insert((original_id, version));
                }
                let (key, value) =
                    packages_by_checkpoint::store(checkpoint, original_id, version, storage_id);
                batch.put(&schema.packages_by_checkpoint, &key, &value)?;
                package_versions::store(original_id, version, storage_id, checkpoint)
            }
            Row::UpgradeInput {
                original_id,
                version,
                storage_id,
            } => {
                let (key, value) =
                    package_versions::store_restored(original_id, version, storage_id);
                if staged.contains(&(original_id, version)) || cf.get(&key)?.is_some() {
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
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_indexer_alt_framework::pipeline::sequential::Handler as _;
    use sui_indexer_alt_framework::store::Store as _;
    use sui_types::base_types::SuiAddress;
    use sui_types::base_types::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::OBJECT_START_VERSION;
    use sui_types::object::Object;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::schema::packages_by_checkpoint::StorageId;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, Store::new(db, Arc::new(schema)))
    }

    /// Every `packages_by_checkpoint` row, in key order.
    fn index_rows(schema: &RpcStoreSchema) -> Vec<(packages_by_checkpoint::Key, ObjectID)> {
        schema
            .packages_by_checkpoint
            .iter(..)
            .unwrap()
            .map(|row| {
                let (key, StorageId(storage_id)) = row.unwrap();
                (key, storage_id)
            })
            .collect()
    }

    fn index_key(
        checkpoint: u64,
        original_id: ObjectID,
        version: u64,
    ) -> packages_by_checkpoint::Key {
        packages_by_checkpoint::Key {
            checkpoint,
            original_id,
            version,
        }
    }

    #[tokio::test]
    async fn commit_stages_both_cfs_in_one_batch() {
        let (_dir, store) = open_store();
        let schema = store.schema();
        let (a, b) = (
            ObjectID::from_single_byte(0xA),
            ObjectID::from_single_byte(0xB),
        );
        let a_v2_storage = ObjectID::from_single_byte(0xA2);
        let rows = vec![
            Row::Published {
                original_id: a,
                version: 2,
                storage_id: a_v2_storage,
                checkpoint: 7,
            },
            Row::Published {
                original_id: b,
                version: 1,
                storage_id: b,
                checkpoint: 7,
            },
        ];

        let mut conn = store.connect().await.unwrap();
        assert_eq!(
            PackageVersions::default()
                .commit(&rows, &mut conn)
                .await
                .unwrap(),
            2
        );
        // Staged, not yet written: neither CF shows the rows until the
        // shared batch commits.
        assert!(index_rows(schema).is_empty());
        assert_eq!(schema.get_package_storage_id(a, 2).unwrap(), None);

        conn.batch.commit().unwrap();
        assert_eq!(
            schema.get_package_storage_id(a, 2).unwrap(),
            Some(a_v2_storage)
        );
        assert_eq!(schema.get_package_storage_id(b, 1).unwrap(), Some(b));
        assert_eq!(
            index_rows(schema),
            vec![(index_key(7, a, 2), a_v2_storage), (index_key(7, b, 1), b)],
        );
    }

    #[tokio::test]
    async fn backfill_over_restore_floor_populates_index() {
        let (_dir, store) = open_store();
        let schema = store.schema();
        let id = ObjectID::from_single_byte(0x5);
        let package = MovePackage::new(
            id,
            OBJECT_START_VERSION,
            BTreeMap::new(),
            u64::MAX,
            vec![],
            BTreeMap::new(),
        )
        .unwrap();
        let object = Object::new_from_package(package, TransactionDigest::genesis_marker());

        // The live-set restore writes a `package_versions` floor (no
        // publish checkpoint) and nothing in the checkpoint index.
        let mut batch = store.db().batch();
        PackageVersions::default()
            .restore(schema, &object, &mut batch)
            .unwrap();
        batch.commit().unwrap();
        let (_, floor) = schema
            .iter_package_versions(id)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(floor.into_inner().checkpoint, None);
        assert!(index_rows(schema).is_empty());

        // The backfill re-publishes the version at its real checkpoint,
        // replacing the floor and filling in the index.
        let mut conn = store.connect().await.unwrap();
        let rows = vec![Row::Published {
            original_id: id,
            version: OBJECT_START_VERSION.value(),
            storage_id: id,
            checkpoint: 42,
        }];
        PackageVersions::default()
            .commit(&rows, &mut conn)
            .await
            .unwrap();
        conn.batch.commit().unwrap();

        let (_, published) = schema
            .iter_package_versions(id)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(published.into_inner().checkpoint, Some(42));
        assert_eq!(
            index_rows(schema),
            vec![(index_key(42, id, OBJECT_START_VERSION.value()), id)],
        );
    }

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = PackageVersions::default()
            .process(&checkpoint)
            .await
            .unwrap();
    }

    /// The processor scans for upgrade inputs only inside the backfill window `[L, T]`, at or
    /// below the restore anchor `T`. Past it, or in a store that was never restored, every input
    /// version already has a row.
    #[tokio::test]
    async fn process_scans_upgrade_inputs_only_up_to_the_restore_anchor() {
        // Checkpoint 1 takes object 0, created in checkpoint 0, as an input. A package at the same
        // id and version replaces that input, standing in for a package upgraded in place.
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let mut checkpoint = builder.build_checkpoint();
        let id = TestCheckpointBuilder::derive_object_id(0);
        let version = checkpoint_input_objects(&checkpoint).unwrap()[&id]
            .0
            .version();
        // A package past its first version names its original id through its modules' address.
        let mut module = move_binary_format::file_format::empty_module();
        module.address_identifiers[0] = move_core_types::account_address::AccountAddress::from(id);
        let package = sui_types::move_package::MovePackage::new_system(version, &[module], []);
        checkpoint.object_set.insert(Object::new_from_package(
            package,
            sui_types::digests::TransactionDigest::genesis_marker(),
        ));
        let checkpoint = Arc::new(checkpoint);
        let cp = checkpoint.summary.sequence_number;

        for (anchor, scanned) in [
            (Some(cp), true),
            (Some(cp + 1), true),
            (Some(cp - 1), false),
            (None, false),
        ] {
            let rows = PackageVersions::with_anchor(anchor)
                .process(&checkpoint)
                .await
                .unwrap();
            let inputs: Vec<_> = rows
                .iter()
                .filter_map(|row| match *row {
                    Row::UpgradeInput {
                        original_id,
                        version,
                        ..
                    } => Some((original_id, version)),
                    Row::Published { .. } => None,
                })
                .collect();
            let expected = if scanned {
                vec![(id, version.value())]
            } else {
                vec![]
            };
            assert_eq!(inputs, expected, "anchor {anchor:?}");
        }
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
        PackageVersions::default()
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
        // Each stage below starts a fresh run, so inputs are checked against the store alone.
        let mut batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &upgrade, &mut HashSet::new(), &mut batch).unwrap(),
            2
        );
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);
        // Only the publish is indexed by checkpoint; the input has no publish checkpoint.
        assert_eq!(
            index_rows(&schema),
            vec![(index_key(50, original_id, 3), original_id)]
        );
        assert_eq!(
            schema
                .get_package_at_checkpoint(original_id, 30)
                .unwrap()
                .map(|(version, _)| version),
            Some(2),
        );

        // Re-indexing the same checkpoint leaves the recorded input untouched.
        let mut batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &upgrade, &mut HashSet::new(), &mut batch).unwrap(),
            1
        );
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);

        // An input whose publish was already indexed keeps its publish checkpoint.
        let next_upgrade = [Row::UpgradeInput {
            original_id,
            version: 3,
            storage_id: original_id,
        }];
        let mut batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &next_upgrade, &mut HashSet::new(), &mut batch).unwrap(),
            0
        );
        batch.commit().unwrap();
        assert_eq!(stored(&schema, original_id), vec![(2, None), (3, Some(50))]);
    }

    /// The synchronizer can still hold an earlier checkpoint's batch, unwritten, when a later
    /// checkpoint's commit stages. A system package published in one checkpoint and upgraded in
    /// place in the next keeps the publish checkpoint of the version the upgrade replaced.
    #[test]
    fn upgrade_input_finds_a_publish_staged_but_not_yet_written() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let original_id = ObjectID::from_single_byte(2);
        let mut staged = HashSet::new();

        // Checkpoint 10 publishes v5, and checkpoint 11 upgrades it in place to v6, before the
        // first batch is written.
        let first = [Row::Published {
            original_id,
            version: 5,
            storage_id: original_id,
            checkpoint: 10,
        }];
        let second = [
            Row::UpgradeInput {
                original_id,
                version: 5,
                storage_id: original_id,
            },
            Row::Published {
                original_id,
                version: 6,
                storage_id: original_id,
                checkpoint: 11,
            },
        ];
        let mut first_batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &first, &mut staged, &mut first_batch).unwrap(),
            1
        );
        let mut second_batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &second, &mut staged, &mut second_batch).unwrap(),
            1
        );
        first_batch.commit().unwrap();
        second_batch.commit().unwrap();

        assert_eq!(
            stored(&schema, original_id),
            vec![(5, Some(10)), (6, Some(11))]
        );
        assert_eq!(
            schema
                .get_package_at_checkpoint(original_id, 9)
                .unwrap()
                .map(|(version, _)| version),
            None,
        );
    }

    /// A batch can span many checkpoints, so a system package published in one checkpoint and
    /// upgraded in place in the next can be staged together. The upgrade input still sees the
    /// publish and keeps its checkpoint.
    #[test]
    fn upgrade_input_finds_a_publish_earlier_in_the_same_batch() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let original_id = ObjectID::from_single_byte(2);

        // Checkpoint 10 publishes v5, and checkpoint 11 upgrades it in place to v6.
        let rows = [
            Row::Published {
                original_id,
                version: 5,
                storage_id: original_id,
                checkpoint: 10,
            },
            Row::UpgradeInput {
                original_id,
                version: 5,
                storage_id: original_id,
            },
            Row::Published {
                original_id,
                version: 6,
                storage_id: original_id,
                checkpoint: 11,
            },
        ];
        let mut batch = db.batch();
        assert_eq!(
            stage_rows(&schema, &rows, &mut HashSet::new(), &mut batch).unwrap(),
            2
        );
        batch.commit().unwrap();

        assert_eq!(
            stored(&schema, original_id),
            vec![(5, Some(10)), (6, Some(11))]
        );
    }
}

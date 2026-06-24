// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::object_version_by_checkpoint`](crate::schema::object_version_by_checkpoint)
//! CF, which resolves an object's version as of a checkpoint.
//!
//! It writes three kinds of rows:
//!
//! - **Change rows** `(id, c) -> final version` -- one per object that
//!   changed in checkpoint `c`, carrying its final version at the end
//!   of `c` (a live version, or the tombstone version for an object
//!   deleted or wrapped and not re-created). Only the final version is
//!   recorded; intra-checkpoint intermediate versions stay addressable
//!   through the version-keyed [`objects`](crate::schema::objects) CF.
//! - **Restore floor rows** `(id, T) -> version`, marked `from_restore`
//!   -- one per live object at the restore anchor `T`, bulk-loaded by
//!   the restore impl. A read below `T` for an object that never
//!   changed in the available window falls back to these.
//! - **Synthetic floor rows** `(id, 0) -> window-entry version` -- for
//!   an object that existed before the available window `[L, T]` and
//!   first changes within it, this records the version it entered the
//!   window with, so a read in `[L, first-change)` resolves to that
//!   instead of the newer restore floor. Written during the embedded
//!   backfill only: past `T` the restore floor already covers
//!   pre-window objects, so the dedup read is skipped. The row is keyed
//!   at checkpoint 0 (below the window, where `L > 0`) so the floor
//!   scan finds it, and the effects-driven pruner retracts it once the
//!   object's first in-window change ages out.

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use sui_consistent_store::Batch;
use sui_consistent_store::Db;
use sui_consistent_store::DbMap;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Restore;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_consistent_store::restore_state;
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
use crate::indexer::checkpoint_input_objects;
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

/// One staged write produced by [`process`](ObjectVersionByCheckpoint::process).
pub enum Row {
    /// Object `id`'s final version at the end of `checkpoint` -- a live
    /// version, or a tombstone version for a removal.
    Change {
        id: ObjectID,
        checkpoint: u64,
        version: SequenceNumber,
    },
    /// Object `id` existed before `checkpoint` and was an input to it,
    /// entering with `version`. A synthetic floor row is written at
    /// `(id, 0)` iff this is the object's first appearance in the
    /// backfill window (so it predates the window).
    Floor {
        id: ObjectID,
        checkpoint: u64,
        version: SequenceNumber,
    },
}

impl ObjectVersionByCheckpoint {
    /// Restore marker attributing every restored live object to the
    /// snapshot anchor `checkpoint`, as a `from_restore` floor row.
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

        // Change rows: objects live at the end of the checkpoint, each
        // with its final version.
        let outputs = checkpoint_output_objects(checkpoint)?;
        let mut rows: Vec<Row> = outputs
            .iter()
            .map(|(id, (object, _))| Row::Change {
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
            rows.push(Row::Change {
                id,
                checkpoint: cp,
                version,
            });
        }

        // Floor candidates: objects that existed *before* this
        // checkpoint and were inputs to it (so they predate any creation
        // this checkpoint), each carrying its incoming version. `commit`
        // turns the first such appearance per object into a synthetic
        // floor row.
        for (id, (input, _)) in checkpoint_input_objects(checkpoint)? {
            rows.push(Row::Floor {
                id,
                checkpoint: cp,
                version: input.version(),
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
        // Mark these as restore-floor rows so a checkpoint-pinned read
        // below the anchor can tell a pre-window object (live) apart
        // from one created in the anchor checkpoint.
        let (key, value) =
            object_version_by_checkpoint::store_restored(object.id(), checkpoint, object.version());
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

        // The restore anchor `T`. Floor rows are only worth computing
        // within the backfill window `[L, T]`; past `T` (tip indexing)
        // the restore floor already covers every pre-window object, so
        // the per-object dedup read is skipped. `None` when the pipeline
        // was never restored (a from-genesis build), where every
        // object's full history is indexed and no floor rows are needed.
        let anchor = restored_anchor(conn.store.db())?;

        let mut count = 0;
        for row in batch {
            match row {
                Row::Change {
                    id,
                    checkpoint,
                    version,
                } => {
                    let (k, v) = object_version_by_checkpoint::store(*id, *checkpoint, *version);
                    conn.batch.put(cf, &k, &v)?;
                    count += 1;
                }
                Row::Floor {
                    id,
                    checkpoint,
                    version,
                } => {
                    if should_floor(cf, anchor, *id, *checkpoint)? {
                        let (k, v) = object_version_by_checkpoint::store(*id, 0, *version);
                        conn.batch.put(cf, &k, &v)?;
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }
}

/// The restore anchor `T` (`__restore.restored_at`) for this pipeline,
/// or `None` if it was never restored.
fn restored_anchor(db: &Db) -> anyhow::Result<Option<u64>> {
    let key = PipelineTaskKey::new(ObjectVersionByCheckpoint::NAME);
    Ok(db
        .framework()
        .restore
        .get(&key)?
        .and_then(|s| match s.state {
            Some(restore_state::State::Complete(c)) => Some(c.restored_at),
            _ => None,
        }))
}

/// Whether a synthetic floor row should be written for object `id`,
/// which entered `checkpoint` from before it.
///
/// True only when (a) the pipeline was restored (so there is a window),
/// (b) `checkpoint` is at or below the restore anchor `T` -- i.e. we
/// are still backfilling `[L, T]`, since past `T` the restore floor
/// covers pre-window objects -- and (c) this is the object's first
/// appearance: it has no row strictly below `checkpoint`. The restore
/// floor sits at `T >= checkpoint`, so it is excluded from that check,
/// as is the change row written for this same checkpoint.
fn should_floor<R: Reader>(
    cf: &DbMap<object_version_by_checkpoint::Key, object_version_by_checkpoint::Value, R>,
    anchor: Option<u64>,
    id: ObjectID,
    checkpoint: u64,
) -> Result<bool, Error> {
    let Some(t) = anchor else {
        return Ok(false);
    };
    if checkpoint > t {
        return Ok(false);
    }
    let lo = object_version_by_checkpoint::Key { id, checkpoint: 0 };
    let hi = object_version_by_checkpoint::Key { id, checkpoint };
    let seen = cf
        .iter_rev((Bound::Included(lo), Bound::Excluded(hi)))?
        .next()
        .is_some();
    Ok(!seen)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_consistent_store::FrameworkSchema;
    use sui_consistent_store::RestoreState;
    use sui_types::base_types::SuiAddress;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    fn fresh_db() -> (tempfile::TempDir, Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    /// Seed this pipeline's `__restore` row as `Complete { restored_at }`.
    fn seed_restored_at(db: &Db, restored_at: u64) {
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch
            .put(
                &framework.restore,
                &PipelineTaskKey::new(ObjectVersionByCheckpoint::NAME),
                &RestoreState {
                    state: Some(restore_state::State::Complete(restore_state::Complete {
                        restored_at,
                    })),
                },
            )
            .unwrap();
        batch.commit().unwrap();
    }

    fn put(schema: &RpcStoreSchema, db: &Db, id: ObjectID, checkpoint: u64, version: u64) {
        let (k, v) =
            object_version_by_checkpoint::store(id, checkpoint, SequenceNumber::from_u64(version));
        let mut batch = db.batch();
        batch
            .put(&schema.object_version_by_checkpoint, &k, &v)
            .unwrap();
        batch.commit().unwrap();
    }

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = ObjectVersionByCheckpoint::default()
            .process(&checkpoint)
            .await
            .unwrap();
    }

    /// A live object created in the checkpoint gets a change row at the
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
        let found = rows.iter().find_map(|r| match r {
            Row::Change {
                id,
                checkpoint,
                version,
            } if *id == created_id => Some((*checkpoint, *version)),
            _ => None,
        });
        assert_eq!(found, Some((7, version)));
        // A freshly created object is not an input to its own checkpoint,
        // so it produces no floor candidate.
        assert!(
            !rows
                .iter()
                .any(|r| matches!(r, Row::Floor { id, .. } if *id == created_id))
        );
    }

    /// An object deleted in the checkpoint is recorded at the tombstone
    /// (lamport) version, not its prior live version.
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
        let found = rows.iter().find_map(|r| match r {
            Row::Change {
                id,
                checkpoint,
                version,
            } if *id == deleted_id => Some((*checkpoint, *version)),
            _ => None,
        });
        assert_eq!(found, Some((1, tombstone_version)));
    }

    /// An object that existed before the checkpoint and is consumed by
    /// it produces a floor candidate carrying its incoming version.
    #[tokio::test]
    async fn process_emits_floor_candidate_for_input_object() {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let cp1 = Arc::new(builder.build_checkpoint());

        let obj = TestCheckpointBuilder::derive_object_id(0);
        let incoming = cp0.transactions[0].effects.lamport_version();

        let rows = ObjectVersionByCheckpoint::default()
            .process(&cp1)
            .await
            .unwrap();
        let floor = rows.iter().find_map(|r| match r {
            Row::Floor {
                id,
                checkpoint,
                version,
            } if *id == obj => Some((*checkpoint, *version)),
            _ => None,
        });
        assert_eq!(floor, Some((1, incoming)), "input object floor candidate");
    }

    /// Restore writes a `from_restore` floor row at the anchor, which
    /// resolves at, above, and (via the fallback) below the anchor.
    #[test]
    fn restore_writes_one_row_at_the_anchor() {
        let (_dir, db, schema) = fresh_db();

        let object =
            Object::with_id_owner_for_testing(ObjectID::from_single_byte(1), SuiAddress::ZERO);

        let mut batch = db.batch();
        ObjectVersionByCheckpoint::for_restore(123)
            .restore(&schema, &object, &mut batch)
            .unwrap();
        batch.commit().unwrap();

        for cp in [122, 123, 200] {
            assert_eq!(
                schema
                    .get_object_version_at_checkpoint(object.id(), cp)
                    .unwrap(),
                Some(object.version()),
            );
        }
    }

    #[test]
    fn restored_anchor_reads_restore_state() {
        let (_dir, db, _schema) = fresh_db();
        // No restore row yet.
        assert_eq!(restored_anchor(&db).unwrap(), None);
        // A completed restore exposes its anchor.
        seed_restored_at(&db, 42);
        assert_eq!(restored_anchor(&db).unwrap(), Some(42));
    }

    #[test]
    fn should_floor_requires_a_restore_anchor() {
        let (_dir, _db, schema) = fresh_db();
        let id = ObjectID::random();
        // No anchor (from-genesis build) -> never floor.
        assert!(!should_floor(&schema.object_version_by_checkpoint, None, id, 50).unwrap());
    }

    #[test]
    fn should_floor_skips_past_the_anchor() {
        let (_dir, _db, schema) = fresh_db();
        let id = ObjectID::random();
        // Tip indexing (checkpoint above the anchor) -> skip the read.
        assert!(!should_floor(&schema.object_version_by_checkpoint, Some(100), id, 150).unwrap(),);
    }

    #[test]
    fn should_floor_true_on_first_appearance_in_window() {
        let (_dir, _db, schema) = fresh_db();
        let id = ObjectID::random();
        // In the window, no prior row -> first appearance, floor it.
        assert!(should_floor(&schema.object_version_by_checkpoint, Some(100), id, 50).unwrap());
    }

    #[test]
    fn should_floor_false_when_already_seen_in_window() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        // A prior in-window change row exists below the checkpoint.
        put(&schema, &db, id, 30, 5);
        assert!(!should_floor(&schema.object_version_by_checkpoint, Some(100), id, 50).unwrap());
    }

    #[test]
    fn should_floor_ignores_the_restore_row_at_the_anchor() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        // The restore floor row sits at the anchor (100), at or above
        // the queried checkpoint, so it must not count as a prior row.
        put(&schema, &db, id, 100, 9);
        assert!(should_floor(&schema.object_version_by_checkpoint, Some(100), id, 50).unwrap());
    }

    /// End-to-end shape for a pre-window object that first changes
    /// inside the window: the rows the restore and backfill would write,
    /// then the reads they enable. Below the first change, the synthetic
    /// floor at `(id, 0)` answers with the pre-window version rather than
    /// the newer restore floor.
    #[test]
    fn synthetic_floor_serves_reads_below_first_change() {
        let (_dir, db, schema) = fresh_db();
        let id = ObjectID::random();
        let anchor = 100; // restore tip T
        let window_entry = SequenceNumber::from_u64(5); // version entering the window
        let v1 = SequenceNumber::from_u64(6); // after the first in-window change (cp 50)

        // The restore floor at T, and the backfilled change row at the
        // object's first in-window change (cp 50).
        let mut batch = db.batch();
        let (rk, rv) = object_version_by_checkpoint::store_restored(id, anchor, v1);
        batch
            .put(&schema.object_version_by_checkpoint, &rk, &rv)
            .unwrap();
        let (ck, cv) = object_version_by_checkpoint::store(id, 50, v1);
        batch
            .put(&schema.object_version_by_checkpoint, &ck, &cv)
            .unwrap();
        batch.commit().unwrap();

        // That change is the object's first appearance in the window, so
        // the backfill writes a synthetic floor at `(id, 0)`.
        assert!(should_floor(&schema.object_version_by_checkpoint, Some(anchor), id, 50).unwrap());
        let mut batch = db.batch();
        let (fk, fv) = object_version_by_checkpoint::store(id, 0, window_entry);
        batch
            .put(&schema.object_version_by_checkpoint, &fk, &fv)
            .unwrap();
        batch.commit().unwrap();

        // Below the first change, the synthetic floor answers with the
        // pre-window version (not the restore floor's newer version).
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 30).unwrap(),
            Some(window_entry),
        );
        // At and after the first change, the change row answers.
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 50).unwrap(),
            Some(v1),
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 75).unwrap(),
            Some(v1),
        );
        assert_eq!(
            schema.get_object_version_at_checkpoint(id, 100).unwrap(),
            Some(v1),
        );
    }
}

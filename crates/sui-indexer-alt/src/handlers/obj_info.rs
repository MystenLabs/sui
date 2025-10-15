// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{ensure, Result};
use diesel::prelude::QueryableByName;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::{base_types::ObjectID, full_checkpoint_content::CheckpointData, object::Object},
    FieldCount,
};
use sui_indexer_alt_schema::{
    objects::{StoredObjInfo, StoredObjInfoDeletionReference},
    schema::{obj_info, obj_info_deletion_reference},
};

use super::checkpoint_input_objects;
use async_trait::async_trait;

pub(crate) struct ObjInfo;

/// Enum to encapsulate different types of updates to be written to the main `obj_info` and
/// reference tables.
pub(crate) enum ProcessedObjInfoUpdate {
    Upsert {
        object: Object,
        /// Indicates whether the object was created/unwrapped in this checkpoint.
        created: bool,
    },
    /// Represents object wrap or deletion. An entry is created on the main table, and two entries
    /// are added to the reference table, one to delete the previous version, and one to delete the
    /// sentinel row itself.
    Delete(ObjectID),
}

pub(crate) struct ProcessedObjInfo {
    pub cp_sequence_number: u64,
    pub update: ProcessedObjInfoUpdate,
}

#[async_trait]
impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = ProcessedObjInfo;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint_input_objects(checkpoint)?;
        let latest_live_output_objects = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect::<BTreeMap<_, _>>();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        for object_id in checkpoint_input_objects.keys() {
            if !latest_live_output_objects.contains_key(object_id) {
                // If an input object is not in the latest live output objects, it must have been
                // deleted or wrapped in this checkpoint. We keep an entry for it in the table. This
                // is necessary when we query objects and iterating over them, so that we don't
                // include the object in the result if it was deleted.
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Delete(*object_id),
                    },
                );
            }
        }
        for (object_id, object) in latest_live_output_objects.iter() {
            // If an object is newly created/unwrapped in this checkpoint, or if the owner changed,
            // we need to insert an entry for it in the table.
            let should_insert = match checkpoint_input_objects.get(object_id) {
                Some(input_object) => input_object.owner() != object.owner(),
                None => true,
            };
            if should_insert {
                // We make note of whether the obj was mutated or created to help with pruning.
                let created = !checkpoint_input_objects.contains_key(object_id);
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Upsert {
                            object: (*object).clone(),
                            created,
                        },
                    },
                );
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait]
impl Handler for ObjInfo {
    type Store = Db;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredObjInfo>>>()?;

        // Entries to commit to the reference table for pruning.
        let mut references = Vec::new();
        for value in values {
            match &value.update {
                ProcessedObjInfoUpdate::Upsert { object, created } => {
                    // Created objects don't have a previous entry in the main table. Unwrapped
                    // objects must have been previously wrapped, and the deletion record for that
                    // wrap will have handled itself. Thus we don't emit an entry to the reference
                    // table.
                    if !created {
                        references.push(StoredObjInfoDeletionReference {
                            object_id: object.id().to_vec(),
                            cp_sequence_number: value.cp_sequence_number as i64,
                        });
                    }
                }
                // Store record of current version to delete previous version, and another to delete
                // itself. When pruning, the deletion record will not be pruned in the
                // `value.cp_sequence_number` checkpoint, but the next one.
                ProcessedObjInfoUpdate::Delete(object_id) => {
                    references.push(StoredObjInfoDeletionReference {
                        object_id: object_id.to_vec(),
                        cp_sequence_number: value.cp_sequence_number as i64,
                    });
                    references.push(StoredObjInfoDeletionReference {
                        object_id: object_id.to_vec(),
                        cp_sequence_number: value.cp_sequence_number as i64 + 1,
                    });
                }
            }
        }

        let count = diesel::insert_into(obj_info::table)
            .values(&stored)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?;

        let deleted_refs = if !references.is_empty() {
            diesel::insert_into(obj_info_deletion_reference::table)
                .values(&references)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?
        } else {
            0
        };

        Ok(count + deleted_refs)
    }

    /// To prune `obj_info`, entries between `[from, to_exclusive)` are read from the reference
    /// table, and the previous versions of the objects are deleted from the main table. Finally,
    /// the reference entries themselves are deleted. The framework guarantees that `to_exclusive <=
    /// checkpoint_hi_inclusive - retention`, so we only prune data that has been committed but is
    /// beyond the retention window.
    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        // This query first deletes from obj_info_deletion_reference and computes predecessors, then
        // deletes from obj_info using the precomputed predecessor information. The inline compute
        // avoids HashAggregate operations and the ensuing materialization overhead.
        //
        // This works best on under 1.5 million object changes, roughly 15k checkpoints. Performance
        // degrades sharply beyond this, since the planner switches to hash joins and full table
        // scans. A HashAggregate approach interestingly becomes more performant in this scenario.
        //
        // If the first call to prune succeeds, subsequent calls will find no records to delete from
        // obj_info_deletion_reference, and consequently no records to delete from the main table.
        // Pruning is thus idempotent after the initial run.
        //
        // TODO: use sui_sql_macro's query!
        let query = format!(
            "
            WITH deletion_refs AS (
                DELETE FROM
                    obj_info_deletion_reference dr
                WHERE
                    {} <= cp_sequence_number AND cp_sequence_number < {}
                RETURNING
                    object_id, (
                    SELECT
                        oi.cp_sequence_number
                    FROM
                        obj_info oi
                    WHERE
                        dr.object_id = oi.object_id
                    AND oi.cp_sequence_number < dr.cp_sequence_number
                    ORDER BY
                        oi.cp_sequence_number DESC
                    LIMIT
                        1
                    ) AS cp_sequence_number
            ),
            deleted_objects AS (
                DELETE FROM
                    obj_info oi
                USING
                    deletion_refs dr
                WHERE
                    oi.object_id = dr.object_id
                AND oi.cp_sequence_number = dr.cp_sequence_number
                RETURNING
                    oi.object_id
            )
            SELECT
                (SELECT COUNT(*) FROM deleted_objects) AS deleted_objects,
                (SELECT COUNT(*) FROM deletion_refs) AS deleted_refs
            ",
            from, to_exclusive
        );

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            deleted_objects: i64,
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            deleted_refs: i64,
        }

        let CountResult {
            deleted_objects,
            deleted_refs,
        } = diesel::sql_query(query)
            .get_result::<CountResult>(conn)
            .await?;

        ensure!(
            deleted_objects == deleted_refs,
            "Deleted objects count ({deleted_objects}) does not match deleted refs count ({deleted_refs})",
        );

        Ok((deleted_objects + deleted_refs) as usize)
    }
}

impl FieldCount for ProcessedObjInfo {
    const FIELD_COUNT: usize = StoredObjInfo::FIELD_COUNT;
}

impl TryInto<StoredObjInfo> for &ProcessedObjInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredObjInfo> {
        match &self.update {
            ProcessedObjInfoUpdate::Upsert { object, .. } => {
                StoredObjInfo::from_object(object, self.cp_sequence_number as i64)
            }
            ProcessedObjInfoUpdate::Delete(object_id) => Ok(StoredObjInfo {
                object_id: object_id.to_vec(),
                cp_sequence_number: self.cp_sequence_number as i64,
                owner_kind: None,
                owner_id: None,
                package: None,
                module: None,
                name: None,
                instantiation: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_indexer_alt_framework::{
        types::{
            base_types::{dbg_addr, SequenceNumber},
            object::Owner,
            test_checkpoint_data_builder::TestCheckpointDataBuilder,
        },
        Indexer,
    };
    use sui_indexer_alt_schema::{objects::StoredOwnerKind, MIGRATIONS};

    use super::*;

    // A helper function to return all entries in the obj_info table sorted by object_id and
    // cp_sequence_number.
    async fn get_all_obj_info(conn: &mut Connection<'_>) -> Result<Vec<StoredObjInfo>> {
        let query = obj_info::table.load(conn).await?;
        Ok(query)
    }

    async fn get_all_obj_info_deletion_references(
        conn: &mut Connection<'_>,
    ) -> Result<Vec<StoredObjInfoDeletionReference>> {
        let query = obj_info_deletion_reference::table.load(conn).await?;
        Ok(query)
    }

    #[tokio::test]
    async fn test_process_basics() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint1)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 0);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: true,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(0, 1, &mut conn).await.unwrap();
        // The object is newly created, so no prior state to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        // No deletion references are created for newly created objects.
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        assert!(all_obj_info_deletion_references.is_empty());
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));

        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint2)).await.unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);
        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        // No new entries are inserted to the table, so no old entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint3)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 2);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: false,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let addr1 = TestCheckpointDataBuilder::derive_address(1);
        assert_eq!(all_obj_info.len(), 2);
        assert_eq!(all_obj_info[1].object_id, object0.to_vec());
        assert_eq!(all_obj_info[1].cp_sequence_number, 2);
        assert_eq!(all_obj_info[1].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[1].owner_id, Some(addr1.to_vec()));

        let rows_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        // The object is transferred, so we prune the old entry. Two counts, one from main table and
        // another from ref.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        // Only the new entry is left in the table.
        assert_eq!(all_obj_info[0].cp_sequence_number, 2);

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint4 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint4)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 3);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        // 1 insertion to main table, 2 to ref table because of delete.
        assert_eq!(rows_inserted, 3);
        let rows_pruned = ObjInfo.prune(3, 4, &mut conn).await.unwrap();
        let delete_row_pruned = ObjInfo.prune(4, 5, &mut conn).await.unwrap();
        // The object is deleted, so we prune both the old entry and the delete entry.
        assert_eq!(rows_pruned + delete_row_pruned, 4);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_process_noop() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        // In this checkpoint, an object is created and deleted in the same checkpoint.
        // We expect that no updates are made to the table.
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        let rows_pruned = ObjInfo.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    /// Tests create (cp 0) -> wrap (cp 1) -> prune -> unwrap (cp 2)
    #[tokio::test]
    async fn test_process_wrap_and_prune_before_unwrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        // 3, not 2, because the wrap entry emits two rows on the ref table
        assert_eq!(rows_inserted, 3);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 2);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert!(all_obj_info[0].owner_kind.is_some());
        assert_eq!(all_obj_info[1].object_id, object0.to_vec());
        assert_eq!(all_obj_info[1].cp_sequence_number, 1);
        assert!(all_obj_info[1].owner_kind.is_none());

        let rows_pruned = ObjInfo.prune(0, 2, &mut conn).await.unwrap();
        let wrapped_row_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        // Both the creation entry and the wrap entry will be pruned.
        assert_eq!(rows_pruned + wrapped_row_pruned, 4);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: true,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        // No entry prior to this checkpoint, so no entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        assert!(all_obj_info_deletion_references.is_empty());
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 2);
        assert!(all_obj_info[0].owner_kind.is_some());
    }

    #[tokio::test]
    async fn test_process_wrap_and_prune_after_unwrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        // 3, not 2, because the wrap entry emits two rows on the ref table
        assert_eq!(rows_inserted, 3);

        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: true,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        // Only one, we don't insert a row to ref table on create or unwrap.
        assert_eq!(rows_inserted, 1);

        let rows_pruned = ObjInfo.prune(0, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 4);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        assert!(all_obj_info_deletion_references.is_empty());
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].cp_sequence_number, 2);
        assert!(all_obj_info[0].owner_kind.is_some());
    }

    #[tokio::test]
    async fn test_process_shared_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_shared_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: true,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(0, 1, &mut conn).await.unwrap();
        // No entry prior to this checkpoint, so no entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Shared));
    }

    #[tokio::test]
    async fn test_process_immutable_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: false,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 2);
        let rows_pruned = ObjInfo.prune(0, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned. Result is 2, 1 from main table and 1 from reference.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Immutable));
    }

    #[tokio::test]
    async fn test_process_object_owned_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::ObjectOwner(dbg_addr(0)))
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: false,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 2);

        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Object));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));
    }

    #[tokio::test]
    async fn test_process_consensus_v2_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .change_object_owner(
                0,
                Owner::ConsensusAddressOwner {
                    start_version: SequenceNumber::from_u64(1),
                    owner: dbg_addr(0),
                },
            )
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Upsert {
                object: _,
                created: false,
            }
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 2);

        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned. Two counts, one from main table and another from ref.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));
    }

    #[tokio::test]
    async fn test_obj_info_batch_prune() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        let rows_pruned = ObjInfo.prune(0, 3, &mut conn).await.unwrap();
        let delete_row_pruned = ObjInfo.prune(3, 4, &mut conn).await.unwrap();
        assert_eq!(rows_pruned + delete_row_pruned, 6);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_obj_info_prune_with_missing_data() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // No entries to prune yet.
        assert_eq!(ObjInfo.prune(0, 2, &mut conn).await.unwrap(), 0);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune both checkpoints 0 and 1.
        ObjInfo.prune(0, 2, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Prune based on new info from checkpoint 2
        assert_eq!(ObjInfo.prune(2, 4, &mut conn).await.unwrap(), 2);

        builder = builder
            .start_transaction(2)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune checkpoint 2, as well as 3.
        ObjInfo.prune(2, 4, &mut conn).await.unwrap();
    }

    /// In our processing logic, we consider objects that appear as input to the checkpoint but not
    /// in the output as wrapped or deleted. This emits a tombstone row. Meanwhile, the remote store
    /// containing `CheckpointData` used to include unchanged consensus objects in the `input_objects`
    /// of a `CheckpointTransaction`. Because these read-only consensus objects were not modified, they
    /// were not included in `output_objects`. But that means within our pipeline, these object
    /// states were incorrectly treated as deleted, and thus every transaction read emitted a
    /// tombstone row. This test validates that unless an object appears as an input object from
    /// `tx.effects.object_changes`, we do not consider it within our pipeline.
    ///
    /// Use the checkpoint builder to create a shared object. Then, remove this from the checkpoint,
    /// and replace it with a transaction that takes the shared object as read-only.
    #[tokio::test]
    async fn test_process_unchanged_consensus_object() {
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_shared_object(1)
            .finish_transaction();

        builder.build_checkpoint();

        builder = builder
            .start_transaction(0)
            .read_shared_object(1)
            .finish_transaction();

        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).await.unwrap();
        assert!(result.is_empty());
    }

    /// Three objects are created in checkpoint 0. All are transferred in checkpoint 1, and
    /// transferred back in checkpoint 2. Prune `[2, 3)` first, then `[1, 2)`, finally `[0, 1)`.
    #[tokio::test]
    async fn test_process_out_of_order_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint0)).await.unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 3);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint1)).await.unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 6);

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint2)).await.unwrap();
        assert_eq!(result.len(), 3);
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 6);

        // Each of the 3 objects will have two entries, one at cp_sequence_number 1, another at 2.
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        assert_eq!(all_obj_info_deletion_references.len(), 6);
        for object in &all_obj_info_deletion_references {
            assert!(object.cp_sequence_number == 1 || object.cp_sequence_number == 2);
        }

        let rows_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        // Each object should have two entries, with cp_sequence_number being either 0 or 2.
        for object in &all_obj_info {
            assert!(object.cp_sequence_number == 0 || object.cp_sequence_number == 2);
        }
        assert_eq!(rows_pruned, 6);
        assert_eq!(all_obj_info.len(), 6);
        // References at cp_sequence_number 2 should be pruned.
        for object in &all_obj_info_deletion_references {
            assert!(object.cp_sequence_number != 2);
        }

        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        // Each object should have a single entry with cp_sequence_number 2.
        for object in &all_obj_info {
            assert_eq!(object.cp_sequence_number, 2);
        }
        assert_eq!(rows_pruned, 6);
        assert_eq!(all_obj_info.len(), 3);
        // References at cp_sequence_number 1 should be pruned.
        for object in &all_obj_info_deletion_references {
            assert_eq!(object.cp_sequence_number, 0);
        }
    }

    /// Test concurrent pruning operations to ensure thread safety and data consistency.
    /// This test creates the same scenario as test_process_out_of_order_pruning but runs
    /// multiple pruning operations concurrently.
    #[tokio::test]
    async fn test_process_concurrent_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Create the same scenario as the out-of-order test
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint0)).await.unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint1)).await.unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint2)).await.unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        // Verify initial state
        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 9); // 3 objects × 3 checkpoints
        let all_obj_info_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();
        assert_eq!(all_obj_info_deletion_references.len(), 6);

        // Run concurrent pruning operations
        let mut handles = Vec::new();

        // Clone the store so each spawned task can own its own connection
        let store = indexer.store().clone();

        // Spawn pruning [2, 3)
        let store1 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store1.connect().await.unwrap();
            ObjInfo.prune(2, 3, &mut conn).await
        }));

        // Spawn pruning [1, 2)
        let store2 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store2.connect().await.unwrap();
            ObjInfo.prune(1, 2, &mut conn).await
        }));

        // Spawn pruning [0, 1)
        let store3 = store.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = store3.connect().await.unwrap();
            ObjInfo.prune(0, 1, &mut conn).await
        }));

        // Wait for all pruning operations to complete
        let results: Vec<Result<usize, anyhow::Error>> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Verify all pruning operations succeeded
        for result in &results {
            assert!(result.is_ok(), "Pruning operation failed: {:?}", result);
        }

        // Verify final state is consistent
        let final_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let final_deletion_references = get_all_obj_info_deletion_references(&mut conn)
            .await
            .unwrap();

        // After all pruning, we should have only the latest versions (cp_sequence_number = 2)
        assert_eq!(final_obj_info.len(), 3);
        for object in &final_obj_info {
            assert_eq!(object.cp_sequence_number, 2);
        }

        // All deletion references should be cleaned up
        assert_eq!(final_deletion_references.len(), 0);

        // Verify the total number of pruned rows matches expectations
        let total_pruned: usize = results.into_iter().map(|r| r.unwrap()).sum();
        assert_eq!(total_pruned, 12);
        for object in &final_obj_info {
            assert_eq!(object.cp_sequence_number, 2);
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::prelude::QueryableByName;
use diesel_async::scoped_futures::ScopedFutureExt;
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

pub(crate) struct ObjInfo;

/// Enum to encapsulate different types of updates to be written to the main `obj_info` and
/// reference tables.
pub(crate) enum ProcessedObjInfoUpdate {
    /// Represents both object creation and unwrapping. An entry is created on the main table only.
    Created(Object),
    /// Any mutation that isn't a wrap or delete. An entry is created on both the main and reference tables.
    Mutated(Object),
    /// Represents object wrap or deletion. An entry is created on the main table, and two entries
    /// are added to the reference table, one to delete the previous version, and one to delete the
    /// sentinel row itself.
    Delete(ObjectID),
}

pub(crate) struct ProcessedObjInfo {
    pub cp_sequence_number: u64,
    pub update: ProcessedObjInfoUpdate,
}

impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = ProcessedObjInfo;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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
                if checkpoint_input_objects.contains_key(object_id) {
                    values.insert(
                        *object_id,
                        ProcessedObjInfo {
                            cp_sequence_number,
                            update: ProcessedObjInfoUpdate::Mutated((*object).clone()),
                        },
                    );
                } else {
                    values.insert(
                        *object_id,
                        ProcessedObjInfo {
                            cp_sequence_number,
                            update: ProcessedObjInfoUpdate::Created((*object).clone()),
                        },
                    );
                }
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfo {
    type Store = Db;

    const PRUNING_REQUIRES_PROCESSED_VALUES: bool = true;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredObjInfo>>>()?;

        // Entries to commit to the reference table for pruning.
        let mut references = Vec::new();
        for value in values {
            match &value.update {
                // Created objects don't have a previous entry in the main table. Unwrapped objects
                // must have been previously wrapped, and the deletion record for that wrap will
                // have handled itself. Thus we don't emit an entry to the reference table.
                ProcessedObjInfoUpdate::Created(_) => {}
                // Store a record to delete previous version
                ProcessedObjInfoUpdate::Mutated(object) => {
                    references.push(StoredObjInfoDeletionReference {
                        object_id: object.id().to_vec(),
                        cp_sequence_number: value.cp_sequence_number as i64,
                    });
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

        use diesel_async::AsyncConnection;
        let obj_info_count = conn
            .transaction(|conn| {
                async move {
                    let count = diesel::insert_into(obj_info::table)
                        .values(&stored)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;

                    if !references.is_empty() {
                        diesel::insert_into(obj_info_deletion_reference::table)
                            .values(&references)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .await?;
                    }

                    Ok::<_, anyhow::Error>(count)
                }
                .scope_boxed()
            })
            .await?;

        Ok(obj_info_count)
    }

    /// To prune `obj_info`, entries between `[from, to_exclusive)` are read from the reference
    /// table, and the previous versions of the objects are deleted from the main table. Finally,
    /// the reference entries themselves are deleted.
    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        // 1. Find all deletion references in range
        // 2. Find and delete previous versions of objects
        // 3. Delete the deletion references themselves
        let query = format!(
            "
            WITH deletion_refs AS (
                SELECT object_id, cp_sequence_number
                FROM obj_info_deletion_reference
                WHERE cp_sequence_number >= {} AND cp_sequence_number < {}
            ),
            deleted_objects AS (
                DELETE FROM obj_info oi
                WHERE EXISTS (
                    SELECT 1 FROM deletion_refs dr
                    WHERE dr.object_id = oi.object_id
                    AND oi.cp_sequence_number = (
                        SELECT oi2.cp_sequence_number
                        FROM obj_info oi2
                        WHERE oi2.object_id = dr.object_id
                        AND oi2.cp_sequence_number < dr.cp_sequence_number
                        ORDER BY oi2.cp_sequence_number DESC
                        LIMIT 1
                    )
                )
                RETURNING oi.object_id
            ),
            deleted_refs AS (
                DELETE FROM obj_info_deletion_reference
                WHERE cp_sequence_number >= {} AND cp_sequence_number < {}
            )
            SELECT count(*) FROM deleted_objects
            ",
            from, to_exclusive, from, to_exclusive
        );

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }

        let result = diesel::sql_query(query)
            .get_result::<CountResult>(conn)
            .await?;

        Ok(result.count as usize)
    }
}

impl FieldCount for ProcessedObjInfo {
    const FIELD_COUNT: usize = StoredObjInfo::FIELD_COUNT;
}

impl TryInto<StoredObjInfo> for &ProcessedObjInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredObjInfo> {
        match &self.update {
            ProcessedObjInfoUpdate::Created(object) | ProcessedObjInfoUpdate::Mutated(object) => {
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
        let result = ObjInfo.process(&Arc::new(checkpoint1)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 0);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Created(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(0, 1, &mut conn).await.unwrap();
        // The object is newly created, so no prior state to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
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
        let result = ObjInfo.process(&Arc::new(checkpoint2)).unwrap();
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
        let result = ObjInfo.process(&Arc::new(checkpoint3)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 2);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Mutated(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let addr1 = TestCheckpointDataBuilder::derive_address(1);
        assert_eq!(all_obj_info.len(), 2);
        assert_eq!(all_obj_info[1].object_id, object0.to_vec());
        assert_eq!(all_obj_info[1].cp_sequence_number, 2);
        assert_eq!(all_obj_info[1].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[1].owner_id, Some(addr1.to_vec()));

        let rows_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        // The object is transferred, so we prune the old entry.
        assert_eq!(rows_pruned, 1);

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
        let result = ObjInfo.process(&Arc::new(checkpoint4)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 3);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(3, 4, &mut conn).await.unwrap();
        let delete_row_pruned = ObjInfo.prune(4, 5, &mut conn).await.unwrap();
        // The object is deleted, so we prune both the old entry and the delete entry.
        assert_eq!(rows_pruned + delete_row_pruned, 2);

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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        let rows_pruned = ObjInfo.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_wrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

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
        assert_eq!(rows_pruned + wrapped_row_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Created(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(2, 3, &mut conn).await.unwrap();
        // No entry prior to this checkpoint, so no entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Created(_)
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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Mutated(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = ObjInfo.prune(0, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::ObjectOwner(dbg_addr(0)))
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Mutated(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Mutated(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 2);

        let rows_pruned = ObjInfo.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

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
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        let rows_pruned = ObjInfo.prune(0, 3, &mut conn).await.unwrap();
        let delete_row_pruned = ObjInfo.prune(3, 4, &mut conn).await.unwrap();
        assert_eq!(rows_pruned + delete_row_pruned, 3);

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
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // No entries to prune yet.
        assert_eq!(ObjInfo.prune(0, 2, &mut conn).await.unwrap(), 0);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune both checkpoints 0 and 1.
        ObjInfo.prune(0, 2, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Prune based on new info from checkpoint 2
        assert_eq!(ObjInfo.prune(2, 4, &mut conn).await.unwrap(), 1);

        builder = builder
            .start_transaction(2)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune checkpoint 2, as well as 3.
        ObjInfo.prune(2, 4, &mut conn).await.unwrap();
    }

    /// In our processing logic, we consider objects that appear as input to the checkpoint but not
    /// in the output as wrapped or deleted. This emits a tombstone row. Meanwhile, the remote store
    /// containing `CheckpointData` used to include unchanged shared objects in the `input_objects`
    /// of a `CheckpointTransaction`. Because these read-only shared objects were not modified, they
    ///were not included in `output_objects`. But that means within our pipeline, these object
    /// states were incorrectly treated as deleted, and thus every transaction read emitted a
    /// tombstone row. This test validates that unless an object appears as an input object from
    /// `tx.effects.object_changes`, we do not consider it within our pipeline.
    ///
    /// Use the checkpoint builder to create a shared object. Then, remove this from the checkpoint,
    /// and replace it with a transaction that takes the shared object as read-only.
    #[tokio::test]
    async fn test_process_unchanged_shared_object() {
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
        let result = ObjInfo.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
    }
}

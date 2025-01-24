// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{objects::StoredObjInfo, schema::obj_info};
use sui_pg_db as db;
use sui_types::{base_types::ObjectID, full_checkpoint_content::CheckpointData, object::Object};

use crate::consistent_pruning::{PruningInfo, PruningLookupTable};

#[derive(Default)]
pub(crate) struct ObjInfo {
    pruning_lookup_table: Arc<PruningLookupTable>,
}

pub(crate) enum ProcessedObjInfoUpdate {
    Insert(Object),
    Delete(ObjectID),
}

pub(crate) struct ProcessedObjInfo {
    pub cp_sequence_number: u64,
    pub update: ProcessedObjInfoUpdate,
}

impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = ProcessedObjInfo;

    // TODO: Add tests for this function and the pruner.
    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect::<BTreeMap<_, _>>();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        let mut prune_info = PruningInfo::new();
        for object_id in checkpoint_input_objects.keys() {
            if !latest_live_output_objects.contains_key(object_id) {
                // If an input object is not in the latest live output objects, it must have been deleted
                // or wrapped in this checkpoint. We keep an entry for it in the table.
                // This is necessary when we query objects and iterating over them, so that we don't
                // include the object in the result if it was deleted.
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Delete(*object_id),
                    },
                );
                prune_info.add_deleted_object(*object_id);
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
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Insert((*object).clone()),
                    },
                );
                // We do not need to prune if the object was created in this checkpoint,
                // because this object would not have been in the table prior to this checkpoint.
                if checkpoint_input_objects.contains_key(object_id) {
                    prune_info.add_mutated_object(*object_id);
                }
            }
        }
        self.pruning_lookup_table
            .insert(cp_sequence_number, prune_info);

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfo {
    const PRUNING_REQUIRES_PROCESSED_VALUES: bool = true;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredObjInfo>>>()?;
        Ok(diesel::insert_into(obj_info::table)
            .values(stored)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    // TODO: Add tests for this function.
    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        use sui_indexer_alt_schema::schema::obj_info::dsl;

        let to_prune = self
            .pruning_lookup_table
            .get_prune_info(from, to_exclusive)?;

        if to_prune.is_empty() {
            self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
            return Ok(0);
        }

        // For each (object_id, cp_sequence_number_exclusive), delete all entries in obj_info with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.

        let values = to_prune
            .iter()
            .map(|(object_id, seq_number)| {
                let object_id_hex = hex::encode(object_id);
                format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, seq_number)
            })
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "
            WITH to_prune_data (object_id, cp_sequence_number_exclusive) AS (
                VALUES {}
            )
            DELETE FROM obj_info
            USING to_prune_data
            WHERE obj_info.{:?} = to_prune_data.object_id
              AND obj_info.{:?} < to_prune_data.cp_sequence_number_exclusive
            ",
            values,
            dsl::object_id,
            dsl::cp_sequence_number,
        );
        let rows_deleted = sql_query(query).execute(conn).await?;
        self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
        Ok(rows_deleted)
    }
}

impl FieldCount for ProcessedObjInfo {
    const FIELD_COUNT: usize = StoredObjInfo::FIELD_COUNT;
}

impl TryInto<StoredObjInfo> for &ProcessedObjInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredObjInfo> {
        match &self.update {
            ProcessedObjInfoUpdate::Insert(object) => {
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
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::{objects::StoredOwnerKind, MIGRATIONS};
    use sui_types::{
        base_types::{dbg_addr, SequenceNumber},
        object::{Authenticator, Owner},
        test_checkpoint_data_builder::TestCheckpointDataBuilder,
    };

    use super::*;

    // A helper function to return all entries in the obj_info table sorted by object_id and
    // cp_sequence_number.
    async fn get_all_obj_info(conn: &mut db::Connection<'_>) -> Result<Vec<StoredObjInfo>> {
        let query = obj_info::table.load(conn).await?;
        Ok(query)
    }

    #[tokio::test]
    async fn test_process_basics() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint1)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 0);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
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
            .mutate_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint2)).unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);
        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
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
        let result = obj_info.process(&Arc::new(checkpoint3)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 2);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
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

        let rows_pruned = obj_info.prune(2, 3, &mut conn).await.unwrap();
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
        let result = obj_info.process(&Arc::new(checkpoint4)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 3);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(3, 4, &mut conn).await.unwrap();
        // The object is deleted, so we prune both the old entry and the delete entry.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_process_noop() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
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
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_wrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
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

        let rows_pruned = obj_info.prune(0, 2, &mut conn).await.unwrap();
        // Both the creation entry and the wrap entry will be pruned.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(2, 3, &mut conn).await.unwrap();
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
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_shared_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
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
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 2, &mut conn).await.unwrap();
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
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::ObjectOwner(dbg_addr(0)))
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
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
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .change_object_owner(
                0,
                Owner::ConsensusV2 {
                    start_version: SequenceNumber::from_u64(1),
                    authenticator: Box::new(Authenticator::SingleOwner(dbg_addr(0))),
                },
            )
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 2);

        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
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
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        let rows_pruned = obj_info.prune(0, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_obj_info_prune_with_missing_data() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Cannot prune checkpoint 1 yet since we haven't processed the checkpoint 1 data.
        // This should not yet remove the prune info for checkpoint 0.
        assert!(obj_info.prune(0, 2, &mut conn).await.is_err());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune both checkpoints 0 and 1.
        obj_info.prune(0, 2, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Checkpoint 3 is missing, so we can not prune it.
        assert!(obj_info.prune(2, 4, &mut conn).await.is_err());

        builder = builder
            .start_transaction(2)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune checkpoint 2, as well as 3.
        obj_info.prune(2, 4, &mut conn).await.unwrap();
    }
}

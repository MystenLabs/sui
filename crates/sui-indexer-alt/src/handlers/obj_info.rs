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

        let to_prune = self.pruning_lookup_table.take(from, to_exclusive)?;

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
    use sui_types::{
        base_types::{dbg_addr, SequenceNumber},
        object::{Authenticator, Owner},
        test_checkpoint_data_builder::TestCheckpointDataBuilder,
    };

    use super::*;

    #[test]
    fn test_process_basics() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint1)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 1);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));

        builder = builder
            .start_transaction(0)
            .mutate_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint2)).unwrap();
        assert!(result.is_empty());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint3)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 3);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint4 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint4)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 4);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
    }

    #[test]
    fn test_process_noop() {
        let obj_info = ObjInfo::default();
        // In this checkpoint, an object is created and deleted in the same checkpoint.
        // We expect that no updates are made to the table.
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_process_wrap() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        builder.build_checkpoint();

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
    }

    #[test]
    fn test_process_shared_object() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1)
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
    }

    #[test]
    fn test_process_immutable_object() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        builder.build_checkpoint();

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
    }

    #[test]
    fn test_process_object_owned_object() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        builder.build_checkpoint();

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
    }

    #[test]
    fn test_process_consensus_v2_object() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        builder.build_checkpoint();

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
    }
}

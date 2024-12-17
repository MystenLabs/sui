// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_pg_db as db;
use sui_types::{base_types::ObjectID, full_checkpoint_content::CheckpointData};

pub(crate) struct ObjInfoPruner;

pub(crate) struct ObjInfoToBePruned {
    pub object_id: ObjectID,
    pub cp_sequence_number_exclusive: u64,
}

impl Processor for ObjInfoPruner {
    const NAME: &'static str = "obj_info_pruner";
    type Value = ObjInfoToBePruned;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect::<BTreeMap<_, _>>();
        let mut values = Vec::with_capacity(checkpoint_input_objects.len());
        // We only need to prune if an object is removed, or its owner changed.
        // We do not need to prune when an object is created or unwrapped, since there would have not
        // been an entry for it in the table prior to this checkpoint.
        // This makes the logic different from the one in obj_info.rs.
        for (object_id, input_object) in checkpoint_input_objects {
            if let Some(output_object) = latest_live_output_objects.get(&object_id) {
                if output_object.owner() != input_object.owner() {
                    values.push(ObjInfoToBePruned {
                        object_id,
                        cp_sequence_number_exclusive: cp_sequence_number,
                    });
                }
            } else {
                values.push(ObjInfoToBePruned {
                    object_id,
                    cp_sequence_number_exclusive: cp_sequence_number + 1,
                });
            }
        }
        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfoPruner {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        use sui_indexer_alt_schema::schema::obj_info::dsl;

        // For each (object_id, cp_sequence_number_exclusive), delete all entries in obj_info with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.

        // Minor optimization:For each object_id, we first get the highest cp_sequence_number_exclusive.
        let mut to_prune = BTreeMap::new();
        for v in values {
            let cp = to_prune.entry(v.object_id).or_default();
            *cp = std::cmp::max(*cp, v.cp_sequence_number_exclusive);
        }
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

impl FieldCount for ObjInfoToBePruned {
    // This does not really matter since we are not limited by postgres' bound variable limit, because
    // we don't bind parameters in the deletion statement.
    const FIELD_COUNT: usize = 1;
}

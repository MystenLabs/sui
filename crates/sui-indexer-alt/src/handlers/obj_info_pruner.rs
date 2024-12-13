// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

use super::obj_info::{ObjInfo, ProcessedObjInfo, ProcessedObjInfoUpdate};

pub(crate) struct ObjInfoPruner;

impl Processor for ObjInfoPruner {
    const NAME: &'static str = "obj_info_pruner";
    type Value = ProcessedObjInfo;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        ObjInfo.process(checkpoint)
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfoPruner {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        use sui_indexer_alt_schema::schema::obj_info::dsl;

        // For each (object_id, cp_sequence_number_exclusive), delete all entries in obj_info with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.
        // For each object_id, we first get the highest cp_sequence_number_exclusive.
        let mut to_prune = BTreeMap::new();
        for v in values {
            let object_id = v.object_id();
            let cp_sequence_number_exclusive = match v.update {
                ProcessedObjInfoUpdate::Insert(_) => v.cp_sequence_number,
                ProcessedObjInfoUpdate::Delete(_) => v.cp_sequence_number + 1,
            } as i64;
            let cp = to_prune.entry(object_id).or_default();
            *cp = std::cmp::max(*cp, cp_sequence_number_exclusive);
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

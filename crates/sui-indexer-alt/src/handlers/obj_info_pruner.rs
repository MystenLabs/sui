// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::ExpressionMethods;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::full_checkpoint_content::CheckpointData;

use crate::schema::obj_info;

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
        // For each (object_id, cp_sequence_number_exclusive), delete all entries in obj_info with
        // cp_sequence_number less than cp_sequence_number_exclusive that match the object_id.
        // For each object_id, we first get the highest cp_sequence_number_exclusive.
        // TODO: We could consider make this more efficient by doing some grouping in the collector
        // so that we could merge as many objects as possible across checkpoints.
        let to_prune = values.iter().fold(BTreeMap::new(), |mut acc, v| {
            let object_id = v.object_id();
            let cp_sequence_number_exclusive = match v.update {
                ProcessedObjInfoUpdate::Insert(_) => v.cp_sequence_number,
                ProcessedObjInfoUpdate::Delete(_) => v.cp_sequence_number + 1,
            } as i64;
            let cp = acc.entry(object_id).or_default();
            *cp = std::cmp::max(*cp, cp_sequence_number_exclusive);
            acc
        });
        let mut committed_rows = 0;
        for (object_id, cp_sequence_number_exclusive) in to_prune {
            committed_rows += diesel::delete(obj_info::table)
                .filter(obj_info::object_id.eq(object_id.as_slice()))
                .filter(obj_info::cp_sequence_number.lt(cp_sequence_number_exclusive))
                .execute(conn)
                .await?;
        }
        Ok(committed_rows)
    }
}

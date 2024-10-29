// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{
    db, models::checkpoints::StoredCheckpoint, pipeline::concurrent::Handler, pipeline::Processor,
    schema::kv_checkpoints,
};

pub struct KvCheckpoints;

impl Processor for KvCheckpoints {
    const NAME: &'static str = "kv_checkpoints";

    type Value = StoredCheckpoint;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let sequence_number = checkpoint.checkpoint_summary.sequence_number as i64;
        Ok(vec![StoredCheckpoint {
            sequence_number,
            certified_checkpoint: bcs::to_bytes(&checkpoint.checkpoint_summary)
                .with_context(|| format!("Serializing checkpoint {sequence_number} summary"))?,
            checkpoint_contents: bcs::to_bytes(&checkpoint.checkpoint_contents)
                .with_context(|| format!("Serializing checkpoint {sequence_number} contents"))?,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for KvCheckpoints {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_checkpoints::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

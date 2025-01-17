// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{checkpoints::StoredCheckpoint, schema::kv_checkpoints};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct KvCheckpoints;

impl Processor for KvCheckpoints {
    const NAME: &'static str = "kv_checkpoints";

    type Value = StoredCheckpoint;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let sequence_number = checkpoint.checkpoint_summary.sequence_number as i64;
        let checkpoint_summary = checkpoint.checkpoint_summary.data();
        let signatures = checkpoint.checkpoint_summary.auth_sig();
        Ok(vec![StoredCheckpoint {
            sequence_number,
            checkpoint_contents: bcs::to_bytes(&checkpoint.checkpoint_contents)
                .with_context(|| format!("Serializing checkpoint {sequence_number} contents"))?,
            checkpoint_summary: bcs::to_bytes(checkpoint_summary)
                .with_context(|| format!("Serializing checkpoint {sequence_number} summary"))?,
            validator_signatures: bcs::to_bytes(signatures)
                .with_context(|| format!("Serializing checkpoint {sequence_number} signatures"))?,
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

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        let filter = kv_checkpoints::table
            .filter(kv_checkpoints::sequence_number.between(from as i64, to_exclusive as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

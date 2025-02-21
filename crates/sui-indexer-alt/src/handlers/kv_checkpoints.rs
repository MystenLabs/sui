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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    async fn get_all_kv_checkpoints(
        conn: &mut db::Connection<'_>,
    ) -> Result<Vec<StoredCheckpoint>> {
        let query = kv_checkpoints::table.load(conn).await?;
        Ok(query)
    }

    /// The kv_checkpoints pruner does not require cp_sequence_numbers, it can prune directly with the
    /// checkpoint sequence number range.
    #[tokio::test]
    async fn test_kv_checkpoints_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        // Create 3 checkpoints
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvCheckpoints.process(&checkpoint).unwrap();
        KvCheckpoints::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvCheckpoints.process(&checkpoint).unwrap();
        KvCheckpoints::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvCheckpoints.process(&checkpoint).unwrap();
        KvCheckpoints::commit(&values, &mut conn).await.unwrap();

        // Prune checkpoints from `[0, 2)`
        let rows_pruned = KvCheckpoints.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);

        // Checkpoint 2 remains
        let remaining_checkpoints = get_all_kv_checkpoints(&mut conn).await.unwrap();
        assert_eq!(remaining_checkpoints.len(), 1);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_schema::checkpoints::StoredCpDigest;
use sui_indexer_alt_schema::schema::cp_digests;

pub(crate) struct CpDigests;

#[async_trait]
impl Processor for CpDigests {
    const NAME: &'static str = "cp_digests";

    type Value = StoredCpDigest;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.summary.sequence_number as i64;
        let cp_digest = checkpoint.summary.digest().inner().to_vec();
        Ok(vec![StoredCpDigest {
            cp_sequence_number,
            cp_digest,
        }])
    }
}

#[async_trait]
impl Handler for CpDigests {
    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(cp_digests::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        let filter = cp_digests::table
            .filter(cp_digests::cp_sequence_number.between(from as i64, to_exclusive as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_framework::types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_indexer_alt_schema::MIGRATIONS;

    use super::*;

    async fn get_all_cp_digests(conn: &mut Connection<'_>) -> Result<Vec<StoredCpDigest>> {
        let query = cp_digests::table.load(conn).await?;
        Ok(query)
    }

    /// Verify that the processor extracts the (sequence_number, digest) pair for each checkpoint
    /// and that the pruner removes entries by sequence-number range.
    #[tokio::test]
    async fn test_cp_digests_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // Build and commit 3 checkpoints so we have three rows with distinct digests.
        let mut builder = TestCheckpointBuilder::new(0);
        for _ in 0..3 {
            builder = builder.start_transaction(0).finish_transaction();
            let checkpoint = Arc::new(builder.build_checkpoint());
            let values = CpDigests.process(&checkpoint).await.unwrap();
            CpDigests::commit(&values, &mut conn).await.unwrap();
        }

        // Prune checkpoints from `[0, 2)`.
        let rows_pruned = CpDigests.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 2);

        // Only the entry for checkpoint 2 should remain.
        let remaining = get_all_cp_digests(&mut conn).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].cp_sequence_number, 2);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::ingestion_backfills::IngestionBackfillTrait;
use crate::database::ConnectionPool;
use crate::models::raw_checkpoints::StoredRawCheckpoint;
use crate::schema::raw_checkpoints::dsl::raw_checkpoints;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

pub struct RawCheckpointsBackFill;

#[async_trait::async_trait]
impl IngestionBackfillTrait for RawCheckpointsBackFill {
    type ProcessedType = StoredRawCheckpoint;

    fn process_checkpoint(checkpoint: &CheckpointData) -> Vec<Self::ProcessedType> {
        vec![StoredRawCheckpoint {
            sequence_number: checkpoint.checkpoint_summary.sequence_number as i64,
            certified_checkpoint: bcs::to_bytes(&checkpoint.checkpoint_summary).unwrap(),
            checkpoint_contents: bcs::to_bytes(&checkpoint.checkpoint_contents).unwrap(),
        }]
    }

    async fn commit_chunk(pool: ConnectionPool, processed_data: Vec<Self::ProcessedType>) {
        let mut conn = pool.get().await.unwrap();
        diesel::insert_into(raw_checkpoints)
            .values(processed_data)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await
            .unwrap();
    }
}

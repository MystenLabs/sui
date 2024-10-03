// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod ingestion_backfill_task;
pub mod raw_checkpoints;

use crate::database::ConnectionPool;
use sui_types::full_checkpoint_content::CheckpointData;

#[async_trait::async_trait]
pub trait IngestionBackfillTrait: Send + Sync {
    type ProcessedType: Send + Sync;

    fn process_checkpoint(checkpoint: &CheckpointData) -> Vec<Self::ProcessedType>;
    async fn commit_chunk(pool: ConnectionPool, processed_data: Vec<Self::ProcessedType>);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::ConnectionPool;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[allow(dead_code)]
#[async_trait::async_trait]
pub trait IndexerHandlerTrait: Send + Sync {
    type ProcessedType: Send + Sync + 'static;

    fn get_name() -> &'static str;
    async fn get_progress() -> CheckpointSequenceNumber;
    async fn update_progress(last_processed: CheckpointSequenceNumber);
    async fn process_checkpoint(checkpoint: Arc<CheckpointData>) -> Vec<Self::ProcessedType>;
    async fn commit_chunk(pool: &mut ConnectionPool, chunk: Vec<Self::ProcessedType>);
}

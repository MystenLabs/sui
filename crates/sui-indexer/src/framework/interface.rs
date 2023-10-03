// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_rest_api::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[async_trait::async_trait]
pub trait Handler: Send {
    fn name(&self) -> &str;
    async fn process_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        self.process_checkpoints(&[checkpoint.clone()]).await
    }
    async fn process_checkpoints(&mut self, checkpoints: &[CheckpointData]) -> Result<()> {
        for checkpoint in checkpoints {
            self.process_checkpoint(checkpoint).await?;
        }
        Ok(())
    }
}

pub trait BackfillHandler: Handler {
    fn last_processed_checkpoint(&self) -> Option<CheckpointSequenceNumber>;
}

#[async_trait::async_trait]
pub trait OutOfOrderHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn process_checkpoints(&self, checkpoints: &[CheckpointData]) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: OutOfOrderHandler> Handler for T {
    fn name(&self) -> &str {
        OutOfOrderHandler::name(self)
    }
    async fn process_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()> {
        OutOfOrderHandler::process_checkpoints(self, &[checkpoint.clone()]).await
    }
    async fn process_checkpoints(&mut self, checkpoints: &[CheckpointData]) -> Result<()> {
        OutOfOrderHandler::process_checkpoints(self, checkpoints).await
    }
}

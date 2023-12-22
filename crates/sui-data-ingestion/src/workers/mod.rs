// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use sui_types::full_checkpoint_content::CheckpointData;
mod kv_store;
mod s3;
pub use kv_store::{KVStoreTaskConfig, KVStoreWorker};
pub use s3::{S3TaskConfig, S3Worker};

#[async_trait]
pub trait Worker: Send + Sync + Clone {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()>;
    fn name(&self) -> &'static str;
}

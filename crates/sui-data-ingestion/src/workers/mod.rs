// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use sui_types::full_checkpoint_content::CheckpointData;
mod blob;
mod kv_store;
pub use blob::{BlobTaskConfig, BlobWorker};
pub use kv_store::{KVStoreTaskConfig, KVStoreWorker};

#[async_trait]
pub trait Worker: Send + Sync {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()>;
}

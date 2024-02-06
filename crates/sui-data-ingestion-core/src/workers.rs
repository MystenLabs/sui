// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use sui_types::full_checkpoint_content::CheckpointData;

#[async_trait]
pub trait Worker: Send + Sync {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()>;
    async fn save_progress(&self) -> bool {
        true
    }
}

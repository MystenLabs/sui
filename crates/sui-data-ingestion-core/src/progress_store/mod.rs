// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
mod file;
pub use file::FileProgressStore;

pub type ExecutorProgress = HashMap<String, CheckpointSequenceNumber>;

#[async_trait]
pub trait ProgressStore: Send {
    async fn load(&mut self, task_name: String) -> Result<CheckpointSequenceNumber>;
    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<()>;
}

pub struct ProgressStoreWrapper<P> {
    progress_store: P,
    pending_state: ExecutorProgress,
}

#[async_trait]
impl<P: ProgressStore> ProgressStore for ProgressStoreWrapper<P> {
    async fn load(&mut self, task_name: String) -> Result<CheckpointSequenceNumber> {
        let watermark = self.progress_store.load(task_name.clone()).await?;
        self.pending_state.insert(task_name, watermark);
        Ok(watermark)
    }

    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<()> {
        self.progress_store
            .save(task_name.clone(), checkpoint_number)
            .await?;
        self.pending_state.insert(task_name, checkpoint_number);
        Ok(())
    }
}

impl<P: ProgressStore> ProgressStoreWrapper<P> {
    pub fn new(progress_store: P) -> Self {
        Self {
            progress_store,
            pending_state: HashMap::new(),
        }
    }

    pub fn min_watermark(&self) -> Result<CheckpointSequenceNumber> {
        self.pending_state
            .values()
            .min()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("pools can't be empty"))
    }

    pub fn stats(&self) -> ExecutorProgress {
        self.pending_state.clone()
    }
}

pub struct ShimProgressStore(pub u64);

#[async_trait]
impl ProgressStore for ShimProgressStore {
    async fn load(&mut self, _: String) -> Result<CheckpointSequenceNumber> {
        Ok(self.0)
    }
    async fn save(&mut self, _: String, _: CheckpointSequenceNumber) -> Result<()> {
        Ok(())
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod executor;
mod metrics;
mod progress_store;
mod reader;
mod reducer;
#[cfg(test)]
mod tests;
mod util;
mod worker_pool;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
pub use executor::{setup_single_workflow, IndexerExecutor, MAX_CHECKPOINTS_IN_PROGRESS};
pub use metrics::DataIngestionMetrics;
pub use progress_store::{
    ExecutorProgress, FileProgressStore, ProgressStore, ShimIndexerProgressStore, ShimProgressStore,
};
pub use reader::{CheckpointReader, ReaderOptions};
use sui_types::full_checkpoint_content::CheckpointData;
pub use util::{create_remote_store_client, end_of_epoch_data};
pub use worker_pool::WorkerPool;

#[async_trait]
pub trait Worker: Send + Sync {
    type Result: Send + Sync + Clone;
    async fn process_checkpoint_arc(
        &self,
        checkpoint: &Arc<CheckpointData>,
    ) -> Result<Self::Result> {
        self.process_checkpoint(checkpoint).await
    }
    /// There is no need to implement this if you implement process_checkpoint_arc. The WorkerPool
    /// will only call process_checkpoint_arc. This method was left in place for backwards
    /// compatibiity.
    async fn process_checkpoint(&self, _checkpoint_data: &CheckpointData) -> Result<Self::Result> {
        panic!("process_checkpoint not implemented")
    }

    fn preprocess_hook(&self, _: &CheckpointData) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait Reducer<R: Send + Sync>: Send + Sync {
    async fn commit(&self, batch: Vec<R>) -> Result<()>;

    fn should_close_batch(&self, _batch: &[R], next_item: Option<&R>) -> bool {
        next_item.is_none()
    }
}

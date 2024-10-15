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

use anyhow::Result;
use async_trait::async_trait;
pub use executor::{setup_single_workflow, IndexerExecutor, MAX_CHECKPOINTS_IN_PROGRESS};
pub use metrics::DataIngestionMetrics;
pub use progress_store::{FileProgressStore, ProgressStore, ShimProgressStore};
pub use reader::ReaderOptions;
use sui_types::full_checkpoint_content::CheckpointData;
pub use util::create_remote_store_client;
pub use worker_pool::WorkerPool;

#[async_trait]
pub trait Worker: Send + Sync {
    type Result: Send + Sync;
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<Self::Result>;

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

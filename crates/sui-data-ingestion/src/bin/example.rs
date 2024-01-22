// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use prometheus::Registry;
use std::path::PathBuf;
use sui_data_ingestion::{
    DataIngestionMetrics, FileProgressStore, IndexerExecutor, Worker, WorkerPool,
};
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::sync::oneshot;

struct DummyWorker;

#[async_trait]
impl Worker for DummyWorker {
    async fn process_checkpoint(&self, _checkpoint: CheckpointData) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (_exit_sender, exit_receiver) = oneshot::channel();
    let progress_store = FileProgressStore::new(PathBuf::from("/tmp/backfill_progress"));
    let metrics = DataIngestionMetrics::new(&Registry::new());

    let mut executor = IndexerExecutor::new(progress_store, 1, metrics);
    let worker_pool = WorkerPool::new(
        DummyWorker,
        "task_name".to_string(), /* task name used as a key in the progress store */
        100,                     /* concurrency */
    );
    executor.register(worker_pool).await?;
    executor
        .run(
            PathBuf::from("/tmp/checkpoints"), /* directory should exist but can be empty */
            Some("https://s3.us-west-2.amazonaws.com/mysten-mainnet-checkpoints".to_string()),
            vec![
                (
                    "aws_access_key_id".to_string(),
                    "put_real_key_here".to_string(),
                ),
                (
                    "aws_secret_access_key".to_string(),
                    "put_real_key_here".to_string(),
                ),
            ],
            exit_receiver,
        )
        .await?;
    Ok(())
}

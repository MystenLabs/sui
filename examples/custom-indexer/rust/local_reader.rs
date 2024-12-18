// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio::sync::oneshot;
use anyhow::Result;
use async_trait::async_trait;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_data_ingestion_core as sdic;
use sdic::{Worker, WorkerPool, ReaderOptions};
use sdic::{DataIngestionMetrics, FileProgressStore, IndexerExecutor};
use prometheus::Registry;
use std::path::PathBuf;
use std::env;

struct CustomWorker;

#[async_trait]
impl Worker for CustomWorker {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        // custom processing logic
        println!("Processing Local checkpoint: {}", checkpoint.checkpoint_summary.to_string());
        Ok(())
    }
}
        
#[tokio::main]
async fn main() -> Result<()> {
    let concurrency = 5;
    let (exit_sender, exit_receiver) = oneshot::channel();
    let metrics = DataIngestionMetrics::new(&Registry::new());
    let backfill_progress_file_path =
        env::var("BACKFILL_PROGRESS_FILE_PATH").unwrap_or("/tmp/local_reader_progress".to_string());
    let progress_store = FileProgressStore::new(PathBuf::from(backfill_progress_file_path));
    let mut executor = IndexerExecutor::new(progress_store, 1 /* number of workflow types */, metrics);
    let worker_pool = WorkerPool::new(CustomWorker, "local_reader".to_string(), concurrency);

    executor.register(worker_pool).await?;
    executor.run(
        PathBuf::from("./chk".to_string()), // path to a local directory
        None,
        vec![], // optional remote store access options
        ReaderOptions::default(),       /* remote_read_batch_size */
        exit_receiver,
        ).await?;
    Ok(())
}

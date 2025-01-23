// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::{setup_single_workflow, Worker};
use sui_types::full_checkpoint_content::CheckpointData;

struct CustomWorker;

#[async_trait]
impl Worker for CustomWorker {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        // custom processing logic
        // print out the checkpoint number
        println!(
            "Processing checkpoint: {}",
            checkpoint.checkpoint_summary.to_string()
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let (executor, term_sender) = setup_single_workflow(
        CustomWorker,
        "https://checkpoints.testnet.sui.io".to_string(),
        0,    /* initial checkpoint number */
        5,    /* concurrency */
        None, /* extra reader options */
    )
    .await?;
    executor.await?;
    Ok(())
}

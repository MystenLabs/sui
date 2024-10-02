// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sui_data_ingestion::{ArchivalConfig, ArchivalWorker};
use sui_data_ingestion_core::setup_single_workflow;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    remote_store_url: String,
    archive_url: String,
    archive_remote_store_options: Vec<(String, String)>,
    #[serde(default = "default_commit_file_size")]
    commit_file_size: usize,
    #[serde(default = "default_commit_duration_seconds")]
    commit_duration_seconds: u64,
}

fn default_commit_file_size() -> usize {
    268435456
}

fn default_commit_duration_seconds() -> u64 {
    600
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");
    let config: Config = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;

    let archival_config = ArchivalConfig {
        remote_url: config.archive_url,
        remote_store_options: config.archive_remote_store_options,
        commit_file_size: config.commit_file_size,
        commit_duration_seconds: config.commit_duration_seconds,
    };
    let worker = ArchivalWorker::new(archival_config).await?;
    let initial_checkpoint_number = worker.initial_checkpoint_number().await;

    let (executor, _exit_sender) = setup_single_workflow(
        worker,
        config.remote_store_url,
        initial_checkpoint_number,
        1,
        None,
    )
    .await?;
    executor.await?;
    Ok(())
}

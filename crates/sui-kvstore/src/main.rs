// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use prometheus::Registry;
use sui_data_ingestion_core::{DataIngestionMetrics, IndexerExecutor, ReaderOptions, WorkerPool};
use sui_kvstore::{BigTableClient, BigTableProgressStore, KvWorker};
use telemetry_subscribers::TelemetryConfig;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Please provide BigTable instance id and network name");
        std::process::exit(1);
    }
    let instance_id = args[1].to_string();
    let network = args[2].to_string();
    assert!(
        network == "mainnet" || network == "testnet",
        "Invalid network name"
    );
    let client = BigTableClient::new_local(instance_id).await?;

    let (_exit_sender, exit_receiver) = oneshot::channel();
    let mut executor = IndexerExecutor::new(
        BigTableProgressStore::new(client.clone()),
        1,
        DataIngestionMetrics::new(&Registry::new()),
    );
    let worker_pool = WorkerPool::new(KvWorker { client }, "bigtable".to_string(), 50);
    executor.register(worker_pool).await?;
    executor
        .run(
            tempfile::tempdir()?.into_path(),
            Some(format!("https://checkpoints.{}.sui.io", network)),
            vec![],
            ReaderOptions::default(),
            exit_receiver,
        )
        .await?;
    Ok(())
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use sui_data_ingestion_core::setup_single_workflow;
use sui_kvstore::BigTableClient;
use sui_kvstore::KvWorker;
use telemetry_subscribers::TelemetryConfig;

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

    let client = BigTableClient::new_remote(instance_id, false, None).await?;
    let (executor, _term_sender) = setup_single_workflow(
        KvWorker { client },
        format!("https://checkpoints.{}.sui.io", network),
        0,
        1,
        None,
    )
    .await?;
    executor.await?;
    Ok(())
}

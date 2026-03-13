// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::{Context, Result};
use sui_forking::StartupSeeding;

use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn concurrent_status_and_advance_checkpoint_calls_remain_monotonic_without_panics()
-> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("concurrency-monotonic");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let client = forking.client();
    let status_before = client.status().await?;

    let mut tasks = Vec::new();
    for _ in 0..4 {
        let concurrent_client = client.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..2 {
                concurrent_client
                    .status()
                    .await
                    .context("concurrent status call failed")?;
                concurrent_client
                    .advance_checkpoint()
                    .await
                    .context("concurrent advance_checkpoint call failed")?;
            }
            Ok::<(), anyhow::Error>(())
        }));
    }

    for task in tasks {
        task.await.context("concurrency task join failure")??;
    }

    let status_after = client.status().await?;
    assert!(
        status_after.checkpoint >= status_before.checkpoint,
        "checkpoint regressed under concurrent control flow: before={}, after={}",
        status_before.checkpoint,
        status_after.checkpoint
    );
    assert!(
        status_after.epoch >= status_before.epoch,
        "epoch regressed under concurrent control flow: before={}, after={}",
        status_before.epoch,
        status_after.epoch
    );

    forking.shutdown().await?;
    Ok(())
}

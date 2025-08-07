// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::StartupSeeding;

use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn restart_resumes_from_persisted_local_checkpoint_state() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let fork_checkpoint = source.fork_checkpoint();
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("persistence-resume");

    let initial = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir.clone(),
    )
    .await?;
    initial.client().advance_checkpoint().await?;
    let status_before_shutdown = initial.client().status().await?;
    initial.shutdown().await?;

    let resumed = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir,
    )
    .await?;
    let resumed_status = resumed.client().status().await?;
    assert!(
        resumed_status.checkpoint >= status_before_shutdown.checkpoint,
        "resume checkpoint regressed: resumed={}, before_shutdown={}",
        resumed_status.checkpoint,
        status_before_shutdown.checkpoint
    );

    resumed.shutdown().await?;
    Ok(())
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::StartupSeeding;

use harness::assertions::assert_monotonic_status;
use harness::fixtures;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn status_returns_checkpoint_and_epoch() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("control-status");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let status = forking.client().status().await?;
    assert!(status.checkpoint >= source.fork_checkpoint());

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn advance_checkpoint_increases_checkpoint() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("control-advance-checkpoint");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let client = forking.client();
    let before = client.status().await?;
    client.advance_checkpoint().await?;
    let after = client.status().await?;
    assert!(after.checkpoint > before.checkpoint);

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn advance_clock_succeeds_without_checkpoint_regression() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("control-advance-clock");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let client = forking.client();
    let before = client.status().await?;
    client.advance_clock(7).await?;
    let after = client.status().await?;
    assert_monotonic_status(&before, &after);

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn advance_epoch_preserves_monotonic_epoch_semantics() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("control-advance-epoch");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let client = forking.client();
    let before = client.status().await?;
    client.advance_epoch().await?;
    let after = client.status().await?;
    assert!(
        after.epoch >= before.epoch,
        "epoch regressed after advance_epoch: before={}, after={}",
        before.epoch,
        after.epoch
    );

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn faucet_returns_effects_payload() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("control-faucet");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let recipient = fixtures::deterministic_address(41);
    let faucet_result = forking.client().faucet(recipient, 1_000).await?;
    assert!(
        !faucet_result.effects.is_empty(),
        "faucet transaction should return non-empty effects payload"
    );

    forking.shutdown().await?;
    Ok(())
}

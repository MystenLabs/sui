// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::{ForkingClient, StartupSeeding};

use harness::assertions::assert_data_dir_contains_forking_namespace;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn readiness_returns_after_health_endpoint_is_reachable() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("forking-readiness");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let health_url = forking.base_url().join("health")?;
    let health_response = reqwest::get(health_url).await?;
    assert!(health_response.status().is_success());
    assert_eq!(health_response.text().await?, "OK");

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn explicit_data_dir_is_used() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("forking-explicit-data-dir");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir.clone(),
    )
    .await?;

    assert_eq!(forking.data_dir(), data_dir.as_path());
    assert_data_dir_contains_forking_namespace(forking.data_dir());

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn shutdown_terminates_server_cleanly() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("forking-shutdown");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let control_client = ForkingClient::new(forking.base_url().clone());
    forking.shutdown().await?;

    let status_error = control_client.status().await;
    assert!(
        status_error.is_err(),
        "status should fail after shutdown because server is no longer reachable"
    );

    Ok(())
}

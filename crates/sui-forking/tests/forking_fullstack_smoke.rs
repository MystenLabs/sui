// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;

use harness::assertions::assert_monotonic_status;
use harness::fixtures;
use harness::forking_runtime::{ForkingHarness, wait_for_subscription_message};
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn fork_e2e() -> Result<()> {
    let mut source = SourceNetworkHarness::full_stack().await?;
    let fork_checkpoint = source.fork_checkpoint();
    source.produce_source_activity(1).await?;

    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("fullstack-smoke-data");

    let forking = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir.clone(),
    )
    .await?;

    let client = forking.client();
    let status_before = client.status().await?;

    // -- faucet ---------------------------------------------------------
    let recipient = fixtures::deterministic_address(41);
    let faucet = client.faucet(recipient, 1_000).await?;
    assert!(
        faucet.error.is_none(),
        "faucet should not return an error: {:?}",
        faucet.error
    );
    let effects_bytes = STANDARD
        .decode(&faucet.effects)
        .expect("faucet effects should be valid base64");
    assert!(
        effects_bytes.len() > 50,
        "faucet effects payload suspiciously small: {} bytes",
        effects_bytes.len()
    );

    // -- advance_clock --------------------------------------------------
    let before_clock = client.status().await?;
    client.advance_clock(500).await?;
    let after_clock = client.status().await?;
    assert!(
        after_clock.clock_timestamp_ms > before_clock.clock_timestamp_ms,
        "clock should have advanced: before={}, after={}",
        before_clock.clock_timestamp_ms,
        after_clock.clock_timestamp_ms
    );

    // -- advance_epoch --------------------------------------------------
    let before_epoch = client.status().await?;
    client.advance_epoch().await?;
    let after_epoch = client.status().await?;
    assert!(
        after_epoch.epoch >= before_epoch.epoch,
        "epoch regressed after advance_epoch: before={}, after={}",
        before_epoch.epoch,
        after_epoch.epoch
    );

    // -- subscription + advance_checkpoint ------------------------------
    let mut subscriptions = SubscriptionServiceClient::connect(forking.grpc_endpoint()).await?;
    let mut stream = subscriptions
        .subscribe_checkpoints(fixtures::subscribe_checkpoints_request())
        .await?
        .into_inner();

    client.advance_checkpoint().await?;
    let subscription_event = wait_for_subscription_message(stream.message()).await?;
    assert!(subscription_event.is_some());

    let status_after = client.status().await?;
    assert!(
        status_after.checkpoint > status_before.checkpoint,
        "checkpoint should have increased after advance_checkpoint: before={}, after={}",
        status_before.checkpoint,
        status_after.checkpoint
    );
    assert_monotonic_status(&status_before, &status_after);

    // -- resume from persistence ----------------------------------------
    forking.shutdown().await?;

    let resumed = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let resumed_status = resumed.client().status().await?;
    assert_eq!(
        resumed_status.checkpoint, status_after.checkpoint,
        "resume should restore exact checkpoint: resumed={}, before_shutdown={}",
        resumed_status.checkpoint,
        status_after.checkpoint
    );

    resumed.shutdown().await?;
    Ok(())
}

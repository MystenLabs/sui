// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use std::time::Duration;

use anyhow::Result;
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;

use harness::OPERATION_TIMEOUT_SECS;
use harness::fixtures;
use harness::forking_runtime::{ForkingHarness, wait_for_subscription_message};
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn subscription_emits_checkpoint_after_advance_checkpoint() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-subscription-emits");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut subscriptions = SubscriptionServiceClient::connect(forking.grpc_endpoint()).await?;
    let mut stream = subscriptions
        .subscribe_checkpoints(fixtures::subscribe_checkpoints_request())
        .await?
        .into_inner();

    forking.client().advance_checkpoint().await?;
    let message = wait_for_subscription_message(stream.message()).await?;
    assert!(message.is_some());

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn subscription_closes_without_hang_on_shutdown() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-subscription-shutdown");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut subscriptions = SubscriptionServiceClient::connect(forking.grpc_endpoint()).await?;
    let stream = subscriptions
        .subscribe_checkpoints(fixtures::subscribe_checkpoints_request())
        .await?
        .into_inner();

    tokio::time::timeout(
        Duration::from_secs(OPERATION_TIMEOUT_SECS),
        forking.shutdown(),
    )
    .await??;
    drop(stream);

    Ok(())
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;

use harness::fixtures;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn list_owned_objects_rejects_missing_owner() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-state-missing-owner");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut state = StateServiceClient::connect(forking.grpc_endpoint()).await?;
    let error = state
        .list_owned_objects(fixtures::list_owned_objects_missing_owner_request())
        .await
        .expect_err("owner is required");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn list_owned_objects_returns_objects_for_seeded_owner() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-state-seeded-owner");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::Accounts(vec![source.seed_address()]),
        data_dir,
    )
    .await?;

    let mut state = StateServiceClient::connect(forking.grpc_endpoint()).await?;
    let response = state
        .list_owned_objects(fixtures::list_owned_objects_request(
            source.seed_address(),
            64,
        ))
        .await?
        .into_inner();
    assert!(
        !response.objects.is_empty(),
        "seeded owner should return owned objects"
    );

    forking.shutdown().await?;
    Ok(())
}

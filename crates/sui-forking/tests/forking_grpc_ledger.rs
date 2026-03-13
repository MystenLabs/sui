// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{GetEpochRequest, GetServiceInfoRequest};

use harness::fixtures;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn get_checkpoint_returns_not_found_for_post_fork_sequence_before_local_advance() -> Result<()>
{
    let mut source = SourceNetworkHarness::fast().await?;
    let fork_checkpoint = source.fork_checkpoint();
    source.produce_source_activity(1).await?;

    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-ledger-boundary-pre");

    let forking = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut ledger = LedgerServiceClient::connect(forking.grpc_endpoint()).await?;
    let error = ledger
        .get_checkpoint(fixtures::checkpoint_request(fork_checkpoint + 1))
        .await
        .expect_err("post-fork checkpoint should be local-only before local creation");
    assert_eq!(error.code(), tonic::Code::NotFound);

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn get_checkpoint_returns_data_after_local_advance() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let fork_checkpoint = source.fork_checkpoint();
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-ledger-boundary-post");

    let forking = ForkingHarness::start_programmatic(
        &source,
        fork_checkpoint,
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    forking.client().advance_checkpoint().await?;

    let mut ledger = LedgerServiceClient::connect(forking.grpc_endpoint()).await?;
    let response = ledger
        .get_checkpoint(fixtures::checkpoint_request(fork_checkpoint + 1))
        .await?
        .into_inner();
    assert!(response.checkpoint.is_some());

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn get_service_info_includes_chain_id() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-ledger-service-info");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut ledger = LedgerServiceClient::connect(forking.grpc_endpoint()).await?;
    let service_info = ledger
        .get_service_info(GetServiceInfoRequest::default())
        .await?
        .into_inner();
    assert!(service_info.chain_id.is_some());

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn get_epoch_includes_epoch_value() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-ledger-epoch");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut ledger = LedgerServiceClient::connect(forking.grpc_endpoint()).await?;
    let epoch_response = ledger
        .get_epoch(GetEpochRequest::default())
        .await?
        .into_inner();
    assert!(epoch_response.epoch.is_some());

    forking.shutdown().await?;
    Ok(())
}

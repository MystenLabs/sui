// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::Result;
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;

use harness::fixtures;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn execute_transaction_rejects_missing_transaction() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-tx-execute-missing");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut tx_service =
        TransactionExecutionServiceClient::connect(forking.grpc_endpoint()).await?;
    let error = tx_service
        .execute_transaction(fixtures::execute_transaction_missing_transaction_request())
        .await
        .expect_err("execute_transaction without transaction must fail");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn simulate_transaction_rejects_missing_transaction() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("grpc-tx-simulate-missing");

    let forking = ForkingHarness::start_programmatic(
        &source,
        source.fork_checkpoint(),
        StartupSeeding::None,
        data_dir,
    )
    .await?;

    let mut tx_service =
        TransactionExecutionServiceClient::connect(forking.grpc_endpoint()).await?;
    let error = tx_service
        .simulate_transaction(fixtures::simulate_transaction_missing_transaction_request())
        .await
        .expect_err("simulate_transaction without transaction must fail");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    forking.shutdown().await?;
    Ok(())
}

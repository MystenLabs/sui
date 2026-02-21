// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod harness;

use anyhow::{Result, bail};
use sui_forking::StartupSeeding;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;

use harness::fixtures;
use harness::forking_runtime::ForkingHarness;
use harness::source_network::SourceNetworkHarness;

#[tokio::test]
async fn startup_seeding_accounts_succeeds_for_eligible_checkpoint() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let checkpoint = source.fork_checkpoint();
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("seeding-accounts-success");

    let forking = ForkingHarness::start_programmatic(
        &source,
        checkpoint,
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
        "account seeding should expose owned objects for the configured address"
    );

    forking.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn startup_seeding_objects_fails_for_missing_ids() -> Result<()> {
    let source = SourceNetworkHarness::fast().await?;
    let checkpoint = source.fork_checkpoint();
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("seeding-objects-failure");
    let missing_object_id = fixtures::deterministic_missing_object_id(7);

    let error = match ForkingHarness::start_programmatic(
        &source,
        checkpoint,
        StartupSeeding::Objects(vec![missing_object_id]),
        data_dir,
    )
    .await
    {
        Ok(_) => bail!("missing explicit object should fail startup"),
        Err(error) => error,
    };

    let error_text = format!("{error:#}");
    assert!(
        error_text.contains("Missing object IDs")
            || error_text.contains("Failed to prefetch explicit startup objects"),
        "unexpected startup error: {error_text}"
    );

    Ok(())
}

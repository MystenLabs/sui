// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_indexer_alt::config::{IndexerConfig, PipelineLayer};
use sui_indexer_alt_e2e_tests::{dummy_stored_genesis, OffchainCluster, OffchainClusterConfig};
use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

#[tokio::test]
async fn advance_epoch_safe_mode() -> anyhow::Result<()> {
    telemetry_subscribers::init_for_testing();

    let offchain = OffchainCluster::new(OffchainClusterConfig {
        indexer_config: IndexerConfig {
            pipeline: PipelineLayer {
                cp_sequence_numbers: Some(Default::default()),
                kv_epoch_ends: Some(Default::default()),
                kv_epoch_starts: Some(Default::default()),
                ..Default::default()
            },
            ..IndexerConfig::default()
        },
        stored_genesis: Some(dummy_stored_genesis()),
        ..OffchainClusterConfig::with_local_ingestion()
    })
    .await?;

    let checkpoint_data = TestCheckpointDataBuilder::new(0).advance_epoch(false);
    offchain.write_checkpoint(checkpoint_data).await?;

    offchain
        .wait_for_indexer(0, Duration::from_secs(10))
        .await?;

    offchain
        .wait_for_graphql(0, Duration::from_secs(10))
        .await?;

    //offchain.epoch_safe_mode(0).await?;

    offchain.stopped().await;

    Ok(())
}

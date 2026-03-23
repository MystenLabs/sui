// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use test_cluster::TestClusterBuilder;
use tracing::info;

#[sim_test]
async fn test_checkpoint_rate() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.disable_randomize_checkpoint_tx_limit_for_testing();
        config
    });

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(60000)
        .build()
        .await;

    // Checkpoint execution can be bursty during startup, so wait for steady state.
    tokio::time::sleep(Duration::from_secs(30)).await;

    let start_checkpoint = test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap_or(0)
    });

    let measurement_duration_secs = 10;
    tokio::time::sleep(Duration::from_secs(measurement_duration_secs)).await;

    let end_checkpoint = test_cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap_or(0)
    });

    let checkpoints_created = end_checkpoint - start_checkpoint;
    let rate = checkpoints_created as f64 / measurement_duration_secs as f64;

    info!(
        start_checkpoint,
        end_checkpoint, checkpoints_created, rate, "Checkpoint rate measurement"
    );

    assert!(
        rate <= 5.0,
        "Checkpoint rate should be <= 5/sec, got {rate}"
    );
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::sui_system_state::SuiSystemStateTrait;
use test_cluster::TestClusterBuilder;
use tracing::info;

#[sui_macros::sim_test]
async fn test_cluster_smoke() {
    info!("Starting TestCluster...");
    let test_cluster = TestClusterBuilder::new().build().await;

    info!("TestCluster started, triggering reconfiguration to epoch 1...");
    test_cluster.trigger_reconfiguration().await;

    let system_state = test_cluster.wait_for_epoch(Some(1)).await;
    assert_eq!(system_state.epoch(), 1);
    info!("Successfully reached epoch 1");
}

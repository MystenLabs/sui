// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::quorum_driver::reconfig_observer::OnsiteReconfigObserver;
use sui_core::quorum_driver::reconfig_observer::ReconfigObserver;
use sui_core::safe_client::SafeClientMetricsBase;
use test_utils::network::TestClusterBuilder;
use tracing::info;

use sui_macros::sim_test;

#[sim_test]
async fn test_onsite_reconfig_observer_basic() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .build()
        .await
        .unwrap();

    let fullnode = &test_cluster.fullnode_handle.sui_node;

    let qd = fullnode
        .transaction_orchestrator()
        .unwrap()
        .clone_quorum_driver();
    assert_eq!(qd.current_epoch(), 0);
    let rx = fullnode.subscribe_to_epoch_change();
    let registry = Registry::new();
    let mut observer = OnsiteReconfigObserver::new(
        rx,
        fullnode.clone_authority_store(),
        fullnode.clone_committee_store(),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    );
    let qd_clone = qd.clone_quorum_driver();
    let _observer_handle = tokio::task::spawn(async move { observer.run(qd_clone).await });

    // Wait for all nodes to reach the next epoch.
    info!("Waiting for nodes to advance to epoch 1");
    test_cluster.wait_for_epoch(Some(1)).await;

    // Give it some time for the update to happen
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    let qd = fullnode
        .transaction_orchestrator()
        .unwrap()
        .clone_quorum_driver();
    assert_eq!(qd.current_epoch(), 1);
    assert_eq!(
        fullnode
            .clone_authority_aggregator()
            .unwrap()
            .committee
            .epoch,
        1
    );
}

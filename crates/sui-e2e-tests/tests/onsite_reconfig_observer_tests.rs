// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::quorum_driver::reconfig_observer::OnsiteReconfigObserver;
use sui_core::quorum_driver::reconfig_observer::ReconfigObserver;
use sui_core::safe_client::SafeClientMetricsBase;
use test_cluster::TestClusterBuilder;
use tracing::info;

use sui_macros::sim_test;

#[sim_test]
async fn test_onsite_reconfig_observer_basic() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .build()
        .await;

    let fullnode = &test_cluster.fullnode_handle.sui_node;

    let qd = fullnode.with(|node| {
        node.transaction_orchestrator()
            .unwrap()
            .clone_quorum_driver()
    });
    assert_eq!(qd.current_epoch(), 0);
    let rx = fullnode.with(|node| node.subscribe_to_epoch_change());
    let registry = Registry::new();
    let mut observer = OnsiteReconfigObserver::new(
        rx,
        fullnode.with(|node| node.state().get_object_cache_reader().clone()),
        fullnode.with(|node| node.clone_committee_store()),
        SafeClientMetricsBase::new(&registry),
        AuthAggMetrics::new(&registry),
    );
    let qd_clone = qd.clone_quorum_driver();
    let observer_handle = tokio::task::spawn(async move { observer.run(qd_clone).await });

    // Wait for all nodes to reach the next epoch.
    info!("Waiting for nodes to advance to epoch 1");
    test_cluster.wait_for_epoch(Some(1)).await;

    // Give it some time for the update to happen
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    let qd = fullnode.with(|node| {
        node.transaction_orchestrator()
            .unwrap()
            .clone_quorum_driver()
    });
    assert_eq!(qd.current_epoch(), 1);
    assert_eq!(
        fullnode.with(|node| node.clone_authority_aggregator().unwrap().committee.epoch),
        1
    );
    // The observer thread is not managed by simtest, and hence we must abort it manually to make sure
    // it stops running first. Otherwise it may lead to unexpected channel close issue.
    observer_handle.abort();
}

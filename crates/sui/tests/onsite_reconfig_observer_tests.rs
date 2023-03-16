// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::async_yields_async)]
use prometheus::Registry;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::quorum_driver::reconfig_observer::OnsiteReconfigObserver;
use sui_core::quorum_driver::reconfig_observer::ReconfigObserver;
use sui_core::safe_client::SafeClientMetricsBase;
use test_utils::authority::{spawn_fullnode, spawn_test_authorities, test_authority_configs};
use test_utils::network::wait_for_nodes_transition_to_epoch;
use tracing::info;

use sui_macros::sim_test;

#[sim_test]
async fn test_onsite_reconfig_observer_basic() {
    telemetry_subscribers::init_for_testing();
    let config = test_authority_configs();
    let authorities = spawn_test_authorities(&config).await;
    let fullnode = spawn_fullnode(&config, None).await;

    let _observer_handle = fullnode
        .with_async(|node| async {
            let qd = node
                .transaction_orchestrator()
                .unwrap()
                .clone_quorum_driver();
            assert_eq!(qd.current_epoch(), 0);
            let rx = node.subscribe_to_epoch_change();
            let registry = Registry::new();
            let mut observer = OnsiteReconfigObserver::new(
                rx,
                node.clone_authority_store(),
                node.clone_committee_store(),
                SafeClientMetricsBase::new(&registry),
                AuthAggMetrics::new(&registry),
            );
            let qd_clone = qd.clone_quorum_driver();
            tokio::task::spawn(async move { observer.run(qd_clone).await })
        })
        .await;
    info!("Shutting down epoch 0");
    for handle in &authorities {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }
    // Wait for all nodes to reach the next epoch.
    info!("Waiting for nodes to advance to epoch 1");
    wait_for_nodes_transition_to_epoch(authorities.iter().chain(std::iter::once(&fullnode)), 1)
        .await;

    // Give it some time for the update to happen
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    fullnode.with(|node| {
        let qd = node
            .transaction_orchestrator()
            .unwrap()
            .clone_quorum_driver();
        assert_eq!(qd.current_epoch(), 1);
        assert_eq!(
            node.clone_authority_aggregator().unwrap().committee.epoch,
            1
        );
    });
}

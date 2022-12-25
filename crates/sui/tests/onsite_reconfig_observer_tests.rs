// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::quorum_driver::reconfig_observer::OnsiteReconfigObserver;
use sui_core::quorum_driver::reconfig_observer::ReconfigObserver;
use sui_core::safe_client::SafeClientMetricsBase;
use test_utils::authority::{spawn_test_authorities_with_fullnodes, test_authority_configs};
use test_utils::network::wait_for_nodes_transition_to_epoch;

#[tokio::test]
async fn test_onsite_reconfig_observer_basic() {
    telemetry_subscribers::init_for_testing();
    let (authorities, fullnodes) =
        spawn_test_authorities_with_fullnodes([].into_iter(), &test_authority_configs(), 1).await;
    let fullnode = &fullnodes[0];
    let qd = fullnode.with(|node| {
        let qd = node
            .transaction_orchestrator()
            .unwrap()
            .clone_quorum_driver();
        assert_eq!(qd.current_epoch(), 0);
        qd
    });
    let qd_clone = qd.clone_quorum_driver();
    let registry = Registry::new();
    let mut observer: OnsiteReconfigObserver = fullnode
        .with_async(|node| async {
            let rx = node.subscribe_to_epoch_change().await;
            OnsiteReconfigObserver::new(
                rx,
                node.clone_authority_store(),
                node.clone_committee_store(),
                SafeClientMetricsBase::new(&registry),
                AuthAggMetrics::new(&registry),
            )
        })
        .await;

    let _observer_handle = tokio::task::spawn(async move { observer.run(qd_clone).await });

    for handle in &authorities {
        handle.with(|node| node.close_epoch().unwrap());
    }
    // Wait for all nodes to reach the next epoch.
    wait_for_nodes_transition_to_epoch(authorities.iter().chain(fullnodes.iter()), 1).await;

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

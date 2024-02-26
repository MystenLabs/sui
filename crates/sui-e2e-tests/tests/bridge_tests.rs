// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_api::BridgeReadApiClient;
use sui_macros::sim_test;
use sui_types::SUI_BRIDGE_OBJECT_ID;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_create_bridge_state_object() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(35.into())
        .with_epoch_duration_ms(10000)
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the bridge state object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .get_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .unwrap()
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster.wait_for_protocol_version(36.into()).await;
    // wait until next epoch - authenticator state object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .get_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .unwrap()
                .expect("auth state object should exist");
        });
    }
}

// A test to make sure bridge_api is compatible
#[tokio::test]
async fn test_bridge_api_get_latest_bridge() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(37.into())
        .build()
        .await;

    let client = test_cluster.rpc_client();
    client.get_latest_bridge().await.unwrap();
    // TODO: assert fields in summary
}
